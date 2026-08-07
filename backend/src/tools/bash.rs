// ============================================================
// Bash tool — execute shell commands on the host system
// ============================================================

use async_trait::async_trait;
use std::process::Command;
use std::time::Duration;

use super::trait_def::{RiskLevel, Tool, ToolResult};

pub struct BashTool {
    workspace_root: String,
}

impl BashTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "在沙箱内执行shell命令。用于构建/运行/git/系统操作。找文件请用grep/glob工具。Windows上使用PowerShell执行。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要执行的shell命令"
                },
                "timeout_ms": {
                    "type": "number",
                    "description": "超时毫秒，默认30000"
                }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let command = args["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return ToolResult::error("command不能为空");
        }

        let timeout_ms = args["timeout_ms"]
            .as_u64()
            .unwrap_or(30000)
            .max(1000)
            .min(300000); // 1s ~ 5min

        // Build the command
        let output = if cfg!(target_os = "windows") {
            Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", command])
                .current_dir(&self.workspace_root)
                .output()
        } else {
            Command::new("bash")
                .args(["-c", command])
                .current_dir(&self.workspace_root)
                .output()
        };

        // Apply timeout via a separate blocking thread (simplified approach)
        match tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            tokio::task::spawn_blocking(move || output),
        )
        .await
        {
            Ok(Ok(Ok(output))) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push_str("\n[stderr]\n");
                    }
                    result.push_str(&stderr);
                }

                if result.is_empty() {
                    result = "(no output)".to_string();
                }

                if output.status.success() {
                    // Truncate if too long
                    if result.len() > 20000 {
                        result.truncate(20000);
                        result.push_str("\n... (输出已截断)");
                    }
                    ToolResult::success(result)
                } else {
                    let code = output.status.code().unwrap_or(-1);
                    let mut error_msg = format!("Exit code: {}\n{}", code, result);
                    if error_msg.len() > 20000 {
                        error_msg.truncate(20000);
                        error_msg.push_str("\n... (输出已截断)");
                    }
                    ToolResult::error(error_msg)
                }
            }
            Ok(Ok(Err(e))) => ToolResult::error(format!("命令执行失败: {}", e)),
            Ok(Err(e)) => ToolResult::error(format!("线程错误: {}", e)),
            Err(_) => ToolResult::error(format!("命令超时 ({}ms)", timeout_ms)),
        }
    }
}
