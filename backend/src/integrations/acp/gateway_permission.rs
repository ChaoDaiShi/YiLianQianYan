use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::client::{AcpError, AcpPermissionHandler};
use super::model::{AcpPermissionDecision, AcpPermissionRequest};
use crate::safety::{
    PolicyDecision, SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject,
};
use crate::tools::trait_def::RiskLevel;

/// Routes ACP's nested file permission requests through the existing security
/// Gateway. The Gateway is only asked to evaluate each operation; this adapter
/// never invokes the corresponding file tool itself.
pub struct GatewayAcpPermissionHandler {
    gateway: Arc<SecurityExecutionGateway>,
    cwd: PathBuf,
    subject: SecuritySubject,
}

impl GatewayAcpPermissionHandler {
    /// The supplied Gateway should have live resource-grant enforcement,
    /// database role resolution, and the existing audit recorder configured.
    pub fn new(
        gateway: Arc<SecurityExecutionGateway>,
        cwd: impl Into<PathBuf>,
        subject: SecuritySubject,
    ) -> Self {
        Self {
            gateway,
            cwd: cwd.into(),
            subject,
        }
    }
}

#[async_trait]
impl AcpPermissionHandler for GatewayAcpPermissionHandler {
    async fn request_permission(
        &self,
        request: AcpPermissionRequest,
        cancel: CancellationToken,
    ) -> Result<AcpPermissionDecision, AcpError> {
        if cancel.is_cancelled() || request.session_id.trim().is_empty() {
            return Ok(AcpPermissionDecision::Cancelled);
        }

        let Some(option_id) = find_allow_once_option(&request.options) else {
            return Ok(AcpPermissionDecision::Cancelled);
        };
        let Some((operation, paths)) = parse_file_request(&request.tool_call) else {
            return Ok(AcpPermissionDecision::Cancelled);
        };

        for raw_path in paths {
            if cancel.is_cancelled() {
                return Ok(AcpPermissionDecision::Cancelled);
            }
            let Some(path) = resolve_scoped_path(&self.cwd, &raw_path, operation) else {
                return Ok(AcpPermissionDecision::Cancelled);
            };
            let (tool_name, risk) = operation.gateway_tool();
            let execution = SecurityExecutionRequest {
                conversation_id: request.session_id.clone(),
                tool_call_id: Uuid::new_v4().to_string(),
                tool_name: tool_name.to_string(),
                arguments: json!({ "path": path.to_string_lossy() }),
                subject: self.subject.clone(),
            };
            match self.gateway.evaluate(&execution, risk) {
                Ok(PolicyDecision::Allow(_)) if !cancel.is_cancelled() => {}
                _ => return Ok(AcpPermissionDecision::Cancelled),
            }
        }

        if cancel.is_cancelled() {
            return Ok(AcpPermissionDecision::Cancelled);
        }
        Ok(AcpPermissionDecision::Selected(option_id))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileOperation {
    Read,
    Edit,
}

impl FileOperation {
    fn gateway_tool(self) -> (&'static str, RiskLevel) {
        match self {
            Self::Read => ("read_file", RiskLevel::Low),
            Self::Edit => ("edit_file", RiskLevel::Medium),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallPermissionMetadata {
    kind: FileOperation,
    locations: Vec<ToolCallLocation>,
}

#[derive(Deserialize)]
struct ToolCallLocation {
    path: String,
}

fn parse_file_request(tool_call: &Value) -> Option<(FileOperation, Vec<String>)> {
    let metadata: ToolCallPermissionMetadata = serde_json::from_value(tool_call.clone()).ok()?;
    if metadata.locations.is_empty() {
        return None;
    }
    Some((
        metadata.kind,
        metadata
            .locations
            .into_iter()
            .map(|location| location.path)
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
    #[serde(other)]
    Unsupported,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionOption {
    option_id: String,
    kind: PermissionOptionKind,
}

fn find_allow_once_option(options: &[Value]) -> Option<String> {
    options.iter().find_map(|value| {
        let option: PermissionOption = serde_json::from_value(value.clone()).ok()?;
        if !matches!(option.kind, PermissionOptionKind::AllowOnce)
            || option.option_id.trim().is_empty()
        {
            return None;
        }

        let occurrence_count = options
            .iter()
            .filter(|candidate| {
                candidate.get("optionId").and_then(Value::as_str) == Some(option.option_id.as_str())
            })
            .count();
        (occurrence_count == 1).then_some(option.option_id)
    })
}

fn resolve_scoped_path(cwd: &Path, raw_path: &str, operation: FileOperation) -> Option<PathBuf> {
    if raw_path.trim().is_empty() {
        return None;
    }

    let cwd = std::fs::canonicalize(cwd).ok()?;
    if !cwd.is_dir() {
        return None;
    }
    let input = Path::new(raw_path);
    #[cfg(windows)]
    if !input.is_absolute()
        && matches!(
            input.components().next(),
            Some(std::path::Component::Prefix(_))
        )
    {
        return None;
    }
    let normalized = crate::safety::normalize_path(&cwd, input).ok()?;
    if !crate::safety::is_within_root(&cwd, &normalized) {
        return None;
    }

    match operation {
        FileOperation::Read => {
            let canonical = std::fs::canonicalize(&normalized).ok()?;
            (crate::safety::is_within_root(&cwd, &canonical) && canonical.is_file())
                .then_some(canonical)
        }
        FileOperation::Edit => {
            let target = match std::fs::canonicalize(&normalized) {
                Ok(canonical) if canonical.is_file() => canonical,
                Ok(_) => return None,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if std::fs::symlink_metadata(&normalized).is_ok() {
                        return None;
                    }
                    crate::safety::resolve_write_target(&cwd, &normalized).ok()?
                }
                Err(_) => return None,
            };
            crate::safety::is_within_root(&cwd, &target).then_some(target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{find_allow_once_option, parse_file_request, resolve_scoped_path, FileOperation};
    use serde_json::json;

    #[test]
    fn chooses_allow_once_by_kind_and_rejects_ambiguous_option_ids() {
        let options = vec![
            json!({ "optionId": "once", "name": "Proceed", "kind": "allow_once" }),
            json!({ "optionId": "always", "name": "Allow once", "kind": "allow_always" }),
        ];
        assert_eq!(find_allow_once_option(&options).as_deref(), Some("once"));

        let duplicate = vec![
            json!({ "optionId": "choice", "name": "Proceed", "kind": "allow_once" }),
            json!({ "optionId": "choice", "name": "Remember", "kind": "allow_always" }),
        ];
        assert_eq!(find_allow_once_option(&duplicate), None);
    }

    #[test]
    fn accepts_only_read_and_edit_calls_with_concrete_locations() {
        let (operation, paths) = parse_file_request(&json!({
            "kind": "edit",
            "locations": [{ "path": "src/main.rs", "line": 12 }]
        }))
        .expect("edit request should parse");
        assert!(matches!(operation, FileOperation::Edit));
        assert_eq!(paths, vec!["src/main.rs".to_string()]);

        assert!(parse_file_request(&json!({
            "kind": "execute",
            "locations": [{ "path": "src/main.rs" }]
        }))
        .is_none());
        assert!(parse_file_request(&json!({ "kind": "read" })).is_none());
    }

    #[test]
    fn resolves_file_locations_inside_cwd_and_rejects_escape_paths() {
        let root = std::env::temp_dir().join(format!("acp-gateway-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src")).expect("create test workspace");
        std::fs::write(root.join("src/main.rs"), "fn main() {}").expect("create test file");
        let cwd = std::fs::canonicalize(&root).expect("canonicalize test workspace");

        assert_eq!(
            resolve_scoped_path(&cwd, "src/main.rs", FileOperation::Read),
            Some(std::fs::canonicalize(root.join("src/main.rs")).expect("canonical file"))
        );
        assert!(resolve_scoped_path(&cwd, "../outside.txt", FileOperation::Edit).is_none());
        assert_eq!(
            resolve_scoped_path(&cwd, "new/output.txt", FileOperation::Edit),
            Some(cwd.join("new/output.txt"))
        );

        std::fs::remove_dir_all(root).expect("remove test workspace");
    }
}
