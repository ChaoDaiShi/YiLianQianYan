use crate::{
    db::Database,
    safety::{DescriptorError, ToolSecurityDescriptor},
    tools::{RiskLevel, Tool, ToolExecutionContext, ToolResult},
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::Path, process::Output};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWriteRequest {
    pub workspace_id: String,
    pub expected_root: String,
    pub expected_head: Option<String>,
    pub action: String,
    #[serde(default)]
    pub paths: Vec<String>,
    pub message: Option<String>,
    pub remote: Option<String>,
    pub branch: Option<String>,
}
pub struct WorkspaceGitWriteTool {
    db: Database,
}
impl WorkspaceGitWriteTool {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

async fn command(root: &Path, args: &[String]) -> Result<Output, String> {
    let mut command = tokio::process::Command::new("git");
    command.kill_on_drop(true);
    command
        .args(["--literal-pathspecs", "-C"])
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    #[cfg(windows)]
    {
        command.creation_flags(0x08000000);
    }
    tokio::time::timeout(std::time::Duration::from_secs(60), command.output())
        .await
        .map_err(|_| "Git 操作超过60秒，进程已停止".to_string())?
        .map_err(|e| format!("Git 启动失败：{e}"))
}
async fn text(root: &Path, args: &[&str]) -> Result<String, String> {
    let result = command(
        root,
        &args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>(),
    )
    .await?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().into())
}
fn arguments(input: &GitWriteRequest) -> Result<Vec<String>, String> {
    let args = match input.action.as_str() {
        "stage" | "unstage" => {
            if input.paths.is_empty() {
                return Err("请选择至少一个文件".into());
            }
            for path in &input.paths {
                super::relative_path(Path::new("."), path)?;
            }
            let mut args: Vec<String> = match (input.action.as_str(), input.expected_head.is_some())
            {
                ("stage", _) => vec!["add".into(), "--".into()],
                ("unstage", true) => vec!["restore".into(), "--staged".into(), "--".into()],
                _ => vec!["rm".into(), "--cached".into(), "--".into()],
            };
            args.extend(input.paths.clone());
            args
        }
        "commit" => {
            let message = input
                .message
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or("提交说明不能为空")?;
            vec!["commit".into(), "-m".into(), message.into()]
        }
        "push" => {
            let commit = input
                .expected_head
                .as_deref()
                .filter(|value| {
                    matches!(value.len(), 40 | 64)
                        && value.chars().all(|character| character.is_ascii_hexdigit())
                })
                .ok_or("推送需要当前已确认提交的完整 SHA，请刷新仓库状态")?;
            let remote = input
                .remote
                .as_deref()
                .filter(|value| {
                    !value.is_empty()
                        && !value.starts_with('-')
                        && !value.contains(char::is_whitespace)
                })
                .ok_or("请填写远端名称")?;
            let branch = input
                .branch
                .as_deref()
                .filter(|value| !value.is_empty() && !value.starts_with('-'))
                .ok_or("请填写目标分支")?;
            vec![
                "push".into(),
                remote.into(),
                format!("{commit}:refs/heads/{branch}"),
            ]
        }
        _ => return Err("不支持的 Git 操作".into()),
    };
    Ok(args)
}
async fn execute(db: Database, input: GitWriteRequest) -> Result<Value, String> {
    let root = super::workspace_root(&db, &input.workspace_id)?;
    if root
        != Path::new(&input.expected_root)
            .canonicalize()
            .map_err(|e| e.to_string())?
    {
        return Err("工作空间目录已改变，请刷新后重试".into());
    }
    let repository = text(&root, &["rev-parse", "--show-toplevel"]).await?;
    if Path::new(&repository)
        .canonicalize()
        .map_err(|e| e.to_string())?
        != root
    {
        return Err("请选择仓库根目录作为工作空间".into());
    }
    let head_result = command(
        &root,
        &[
            "rev-parse".into(),
            "--verify".into(),
            "--quiet".into(),
            "HEAD".into(),
        ],
    )
    .await?;
    let head = if head_result.status.success() {
        Some(
            String::from_utf8_lossy(&head_result.stdout)
                .trim()
                .to_string(),
        )
    } else if head_result.status.code() == Some(1) {
        None
    } else {
        return Err("无法读取当前 HEAD".into());
    };
    if head != input.expected_head {
        return Err("仓库 HEAD 已改变，请刷新差异后重新提交操作".into());
    }
    let args = arguments(&input)?;
    if input.action == "push" {
        text(
            &root,
            &[
                "check-ref-format",
                &format!("refs/heads/{}", input.branch.as_deref().unwrap()),
            ],
        )
        .await?;
    }
    let result = command(&root, &args).await?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).trim().to_string());
    }
    let verification = match input.action.as_str() {
        "commit" => {
            let after = text(&root, &["rev-parse", "HEAD"]).await?;
            if Some(&after) == input.expected_head.as_ref() {
                return Err("提交后 HEAD 未变化".into());
            }
            json!({"head":after})
        }
        "push" => {
            let branch = format!("refs/heads/{}", input.branch.as_deref().unwrap());
            let remote = text(
                &root,
                &[
                    "ls-remote",
                    "--heads",
                    input.remote.as_deref().unwrap(),
                    &branch,
                ],
            )
            .await?;
            if remote.split_whitespace().next() != input.expected_head.as_deref() {
                return Err("推送后远端分支未匹配本地 HEAD".into());
            }
            json!({"remote_head":input.expected_head,"branch":branch})
        }
        _ => {
            let mut verify = vec![
                "diff".into(),
                "--cached".into(),
                "--name-only".into(),
                "--".into(),
            ];
            verify.extend(input.paths.clone());
            let state = command(&root, &verify).await?;
            if !state.status.success() {
                return Err("无法读取暂存区验证结果".into());
            }
            let staged = String::from_utf8_lossy(&state.stdout).trim().to_string();
            if input.action == "unstage" && !staged.is_empty() {
                return Err("所选文件仍存在暂存差异".into());
            }
            if input.action == "stage" && staged.is_empty() {
                return Err("所选文件没有产生暂存差异".into());
            }
            json!({"staged_paths":staged})
        }
    };
    Ok(
        json!({"action":input.action,"workspace_id":input.workspace_id,"verified":true,"verification":verification,"output":String::from_utf8_lossy(&result.stdout).trim()}),
    )
}
#[async_trait]
impl Tool for WorkspaceGitWriteTool {
    fn name(&self) -> &str {
        "workspace_git_write"
    }
    fn description(&self) -> &str {
        "对已注册工作空间执行明确的Git暂存、取消暂存、提交或非强制推送，经过审批并验证结果。"
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{"workspace_id":{"type":"string"},"expected_root":{"type":"string"},"expected_head":{"type":["string","null"]},"action":{"enum":["stage","unstage","commit","push"]},"paths":{"type":"array","items":{"type":"string"}},"message":{"type":"string"},"remote":{"type":"string"},"branch":{"type":"string"}},"required":["workspace_id","expected_root","expected_head","action"],"additionalProperties":false})
    }
    fn security_descriptor(&self, args: &Value) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let input: GitWriteRequest = serde_json::from_value(args.clone())
            .map_err(|e| DescriptorError::InvalidDescriptor(e.to_string()))?;
        let root = super::workspace_root(&self.db, &input.workspace_id)
            .map_err(DescriptorError::InvalidDescriptor)?;
        let args = arguments(&input).map_err(DescriptorError::InvalidDescriptor)?;
        super::descriptor(
            self.name(),
            &root,
            format!(
                "git {}",
                serde_json::to_string(&args)
                    .map_err(|e| DescriptorError::InvalidDescriptor(e.to_string()))?
            ),
        )
    }
    async fn execute(&self, _args: Value) -> ToolResult {
        ToolResult::error("Git 写操作必须通过安全执行网关")
    }
    async fn execute_with_context(
        &self,
        args: Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult {
        let input = match serde_json::from_value(args) {
            Ok(input) => input,
            Err(error) => return ToolResult::error(error.to_string()),
        };
        let db = self.db.clone();
        match execute(db, input).await {
            Ok(value) => ToolResult::success_with_structured(value.to_string(), Some(value)),
            Err(error) => ToolResult::error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(value: Value) -> GitWriteRequest {
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn stages_only_literal_selected_paths_and_rejects_traversal() {
        let input = request(
            json!({"workspace_id":"w","expected_root":".","expected_head":null,"action":"stage","paths":["file with spaces.ts"]}),
        );
        assert_eq!(
            arguments(&input).unwrap(),
            vec!["add", "--", "file with spaces.ts"]
        );
        let invalid = request(
            json!({"workspace_id":"w","expected_root":".","expected_head":null,"action":"stage","paths":["../outside"]}),
        );
        assert!(arguments(&invalid).is_err());
    }
    #[test]
    fn push_has_explicit_refspec_and_no_force_option() {
        let commit = "a".repeat(40);
        let input = request(
            json!({"workspace_id":"w","expected_root":".","expected_head":commit,"action":"push","remote":"origin","branch":"feature"}),
        );
        assert_eq!(
            arguments(&input).unwrap(),
            vec![
                "push".to_string(),
                "origin".to_string(),
                format!("{commit}:refs/heads/feature")
            ]
        );
        let descriptor = super::super::descriptor(
            "workspace_git_write",
            Path::new("."),
            format!("git push origin {commit}:refs/heads/feature"),
        )
        .unwrap();
        assert_eq!(descriptor.default_risk, RiskLevel::High);
    }
    #[test]
    fn push_rejects_missing_or_symbolic_commit() {
        for head in [Value::Null, json!("HEAD"), json!("--force"), json!("abc")] {
            let input = request(
                json!({"workspace_id":"w","expected_root":".","expected_head":head,"action":"push","remote":"origin","branch":"feature"}),
            );
            assert!(arguments(&input).is_err());
        }
    }
}
