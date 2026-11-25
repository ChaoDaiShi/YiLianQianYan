use crate::integrations::acp::client::AcpError;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

pub(super) fn parse_expected_outputs(
    value: Option<&Value>,
    cwd: &Path,
) -> Result<Vec<PathBuf>, AcpError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let outputs = value.as_array().ok_or_else(|| {
        AcpError::InvalidConfig("expected_outputs must be an array of relative paths".to_string())
    })?;
    outputs
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AcpError::InvalidConfig(
                        "expected_outputs must contain non-empty relative paths".to_string(),
                    )
                })?;
            let path = Path::new(value);
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(AcpError::InvalidConfig(
                    "expected_outputs must stay beneath cwd".to_string(),
                ));
            }
            Ok(cwd.join(path))
        })
        .collect()
}

pub(super) fn build_task_prompt(task: &str, expected_outputs: &[PathBuf], cwd: &Path) -> String {
    if expected_outputs.is_empty() {
        return task.to_string();
    }
    let outputs = expected_outputs
        .iter()
        .filter_map(|path| {
            path.strip_prefix(cwd)
                .ok()
                .map(|relative| relative.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{task}\n\nExpected output files relative to the configured task working directory: {outputs}")
}

pub(super) async fn collect_expected_outputs(cwd: &Path, expected: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(root) = tokio::fs::canonicalize(cwd).await else {
        return found;
    };
    for path in expected {
        let Ok(actual) = tokio::fs::canonicalize(path).await else {
            continue;
        };
        if actual.starts_with(&root)
            && tokio::fs::metadata(&actual)
                .await
                .is_ok_and(|metadata| metadata.is_file())
        {
            found.push(actual);
        }
    }
    found
}
