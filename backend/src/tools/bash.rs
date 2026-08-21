// ============================================================
// Bash tool — execute shell commands via the managed process runner.
//
// The child runs with a sanitized environment (no inherited secrets) and a
// real async timeout; on timeout the whole process tree is terminated.
// ============================================================

use async_trait::async_trait;
use std::collections::BTreeMap;

use super::trait_def::{RiskLevel, Tool, ToolExecutionContext, ToolResult};

fn gui_launch_tool_for_command(command: &str) -> Option<&'static str> {
    let normalized = command.to_ascii_lowercase();
    let contains_web_url = normalized.contains("http://") || normalized.contains("https://");
    let browser_launcher = ["start-process", "explorer.exe", "cmd /c start", "rundll32"]
        .iter()
        .any(|launcher| normalized.contains(launcher));
    if contains_web_url && browser_launcher {
        return Some("open_url");
    }

    if normalized.contains("explorer.exe") || normalized.contains("shell:appsfolder") {
        return Some("open_application");
    }
    if normalized.contains("cmd /c start") && !normalized.contains("cmd /c start /b") {
        return Some("open_application");
    }
    if normalized.contains("start-process") {
        let explicitly_background = normalized.contains("-windowstyle hidden")
            || normalized.contains("-redirectstandardoutput")
            || normalized.contains("-redirectstandarderror");
        let known_cli_target = [
            "powershell",
            "pwsh",
            "cmd.exe",
            "npm",
            "node",
            "cargo",
            "python",
        ]
        .iter()
        .any(|target| normalized.contains(target));
        if !explicitly_background && !known_cli_target {
            return Some("open_application");
        }
    }
    None
}

pub struct BashTool {
    workspace_root: String,
}

impl BashTool {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }

    async fn execute_managed(
        &self,
        args: serde_json::Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        let command = args["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return ToolResult::error("command不能为空");
        }
        if let Some(tool) = gui_launch_tool_for_command(command) {
            return ToolResult::error(format!(
                "检测到桌面 GUI 启动命令。请使用 {tool}，只有目标窗口真实显示在桌面前台后才能报告成功。"
            ));
        }

        let timeout_ms = args["timeout_ms"]
            .as_u64()
            .unwrap_or(30000)
            .max(1000)
            .min(300000);

        let (program, argv): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
            (
                "powershell.exe",
                vec!["-NoProfile", "-NonInteractive", "-Command", command],
            )
        } else {
            ("bash", vec!["-c", command])
        };

        let result = crate::isolation::run_managed_process_with_registry(
            program,
            &argv,
            &self.workspace_root,
            &BTreeMap::new(),
            timeout_ms,
            &context.managed_process_registry,
            Some(context.tool_call_id.clone()),
            "bash",
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

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "在受控进程中执行shell命令。用于构建/运行/git/系统操作。找文件请用grep/glob；打开网站或桌面GUI应用请分别使用open_url/open_application。Windows上使用PowerShell执行。"
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
        let _ = args;
        ToolResult::error("bash requires SecurityExecutionGateway trusted context")
    }

    async fn execute_with_context(
        &self,
        args: serde_json::Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        self.execute_managed(args, context).await
    }
}

#[cfg(test)]
mod gui_launch_guard_tests {
    use super::gui_launch_tool_for_command;

    #[test]
    fn routes_shell_browser_launches_to_open_url() {
        assert_eq!(
            gui_launch_tool_for_command("Start-Process 'https://www.bilibili.com/'"),
            Some("open_url")
        );
        assert_eq!(
            gui_launch_tool_for_command("explorer.exe https://www.bilibili.com/"),
            Some("open_url")
        );
    }

    #[test]
    fn routes_detached_desktop_launches_to_open_application() {
        assert_eq!(
            gui_launch_tool_for_command("Start-Process QQ"),
            Some("open_application")
        );
        assert_eq!(
            gui_launch_tool_for_command("cmd /c start QQ"),
            Some("open_application")
        );
        assert_eq!(
            gui_launch_tool_for_command("explorer.exe shell:AppsFolder\\Tencent.QQ"),
            Some("open_application")
        );
    }

    #[test]
    fn keeps_non_launch_commands_available() {
        assert_eq!(
            gui_launch_tool_for_command("Invoke-WebRequest https://example.com/api"),
            None
        );
        assert_eq!(gui_launch_tool_for_command("Get-Process QQ"), None);
        assert_eq!(gui_launch_tool_for_command("npm run build"), None);
    }
}
