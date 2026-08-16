// ============================================================
// Bash tool — execute shell commands via the managed process runner.
//
// The child runs with a sanitized environment (no inherited secrets) and a
// real async timeout; on timeout the whole process tree is terminated.
// ============================================================

use async_trait::async_trait;
use std::collections::BTreeMap;

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
        "在受控进程中执行shell命令。用于构建/运行/git/系统操作。找文件请用grep/glob工具。Windows上使用PowerShell执行。"
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

        let (program, argv): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
            (
                "powershell.exe",
                vec!["-NoProfile", "-NonInteractive", "-Command", command],
            )
        } else {
            ("bash", vec!["-c", command])
        };

        // No explicit env → sanitized allowlist env only (secrets stripped).
        let result = crate::isolation::run_managed_process(
            program,
            &argv,
            &self.workspace_root,
            &BTreeMap::new(),
            timeout_ms,
        )
        .await;

        if result.timed_out {
            return ToolResult::error(format!("命令超时 ({}ms)，进程树已终止", timeout_ms));
        }

        let mut output = String::new();
        if !result.stdout.is_empty() {
            output.push_str(&result.stdout);
        }
        if !result.stderr.is_empty() {
            if !output.is_empty() {
                output.push_str("\n[stderr]\n");
            }
            output.push_str(&result.stderr);
        }
        if output.is_empty() {
            output = "(no output)".to_string();
        }

        match result.exit_code {
            Some(0) => ToolResult::success(output),
            Some(code) => ToolResult::error(format!("Exit code: {}\n{}", code, output)),
            None => ToolResult::error(output),
        }
    }
}
