// ============================================================
// Verifier — deterministic verification of tool outcomes.
//
// A Tool returning ok=true does NOT mean the goal was achieved.
// After execution we re-observe the real state and decide
// success / failure / should_replan. No LLM judge in v1.
// ============================================================

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::observation::{observe_file, observe_process_by_pid, Observation};
use crate::tools::trait_def::ToolResult;

/// Outcome of verifying one tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub success: bool,
    pub reason: String,
    pub should_replan: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation: Option<Observation>,
}

impl VerificationResult {
    pub fn success(reason: impl Into<String>, observation: Option<Observation>) -> Self {
        Self {
            success: true,
            reason: reason.into(),
            should_replan: false,
            observation,
        }
    }

    pub fn failure(reason: impl Into<String>, observation: Option<Observation>) -> Self {
        Self {
            success: false,
            reason: reason.into(),
            should_replan: true,
            observation,
        }
    }
}

/// Abstract verifier. The engine calls this after every tool execution.
#[async_trait]
pub trait Verifier: Send + Sync {
    async fn verify(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        tool_result: &ToolResult,
    ) -> VerificationResult;
}

/// Deterministic, rule-based verifier for v1.
pub struct DefaultVerifier {
    workspace_root: String,
}

impl DefaultVerifier {
    pub fn new(workspace_root: &str) -> Self {
        Self {
            workspace_root: workspace_root.to_string(),
        }
    }

    /// Resolve a tool path argument relative to the workspace root.
    fn resolve_path(&self, path: &str) -> std::path::PathBuf {
        let p = std::path::Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::path::Path::new(&self.workspace_root).join(p)
        }
    }

    fn verify_default(&self, tool_result: &ToolResult) -> VerificationResult {
        if tool_result.ok {
            VerificationResult::success("工具执行成功，但当前没有额外确定性验证规则", None)
        } else {
            VerificationResult::failure("工具执行失败", None)
        }
    }

    fn verify_write_file(
        &self,
        args: &serde_json::Value,
        tool_result: &ToolResult,
    ) -> VerificationResult {
        if !tool_result.ok {
            return VerificationResult::failure("工具执行失败", None);
        }
        let path_str = args["path"].as_str().unwrap_or("");
        if path_str.is_empty() {
            return VerificationResult::failure("无法验证：缺少路径参数", None);
        }
        let obs = observe_file(&self.resolve_path(path_str));

        if !obs.exists {
            return VerificationResult::failure("目标文件不存在", Some(Observation::file(obs)));
        }

        // When content was written, confirm the expected text landed (text files only).
        if let Some(expected) = args["content"].as_str() {
            if !expected.is_empty() {
                match &obs.content_preview {
                    Some(preview) if preview.contains(expected) => {}
                    Some(_) => {
                        return VerificationResult::failure(
                            "文件内容与预期不符",
                            Some(Observation::file(obs)),
                        )
                    }
                    None => {
                        // Binary or unreadable preview — cannot confirm content.
                        return VerificationResult::failure(
                            "无法读取文件内容进行验证",
                            Some(Observation::file(obs)),
                        );
                    }
                }
            }
        }

        VerificationResult::success("文件存在且内容符合预期", Some(Observation::file(obs)))
    }

    fn verify_edit_file(
        &self,
        args: &serde_json::Value,
        tool_result: &ToolResult,
    ) -> VerificationResult {
        if !tool_result.ok {
            return VerificationResult::failure("工具执行失败", None);
        }
        let path_str = args["path"].as_str().unwrap_or("");
        if path_str.is_empty() {
            return VerificationResult::failure("无法验证：缺少路径参数", None);
        }
        let obs = observe_file(&self.resolve_path(path_str));

        if !obs.exists {
            return VerificationResult::failure("目标文件不存在", Some(Observation::file(obs)));
        }

        // Confirm the replacement text is present.
        if let Some(replacement) = args["replace"].as_str() {
            if !replacement.is_empty() {
                match &obs.content_preview {
                    Some(preview) if preview.contains(replacement) => {}
                    Some(_) => {
                        return VerificationResult::failure(
                            "替换后的文本未在文件中找到",
                            Some(Observation::file(obs)),
                        )
                    }
                    None => {
                        return VerificationResult::failure(
                            "无法读取文件内容进行验证",
                            Some(Observation::file(obs)),
                        )
                    }
                }
            }
        }

        VerificationResult::success("文件存在且替换文本已生效", Some(Observation::file(obs)))
    }

    fn verify_process(
        &self,
        args: &serde_json::Value,
        tool_result: &ToolResult,
    ) -> VerificationResult {
        if !tool_result.ok {
            return VerificationResult::failure("工具执行失败", None);
        }
        let action = args["action"].as_str().unwrap_or("list");
        match action {
            // kill → the target process should no longer be running.
            "kill" => {
                let pid = args["pid"].as_u64().unwrap_or(0) as u32;
                if pid == 0 {
                    return VerificationResult::failure("无法验证：缺少进程 PID", None);
                }
                let obs = observe_process_by_pid(pid);
                if obs.running {
                    VerificationResult::failure("目标进程仍在运行", Some(Observation::process(obs)))
                } else {
                    VerificationResult::success("目标进程已停止", Some(Observation::process(obs)))
                }
            }
            _ => self.verify_default(tool_result),
        }
    }

    fn verify_gui_launch(&self, tool_result: &ToolResult) -> VerificationResult {
        if tool_result.ok {
            VerificationResult::success("结构化启动工具已确认目标窗口可见且位于桌面前台", None)
        } else {
            VerificationResult::failure("目标窗口未能显示在桌面前台", None)
        }
    }
}

#[async_trait]
impl Verifier for DefaultVerifier {
    async fn verify(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        tool_result: &ToolResult,
    ) -> VerificationResult {
        // Any error path degrades to an explicit failure (never a false success).
        match tool_name {
            "write_file" => self.verify_write_file(args, tool_result),
            "edit_file" => self.verify_edit_file(args, tool_result),
            "process" => self.verify_process(args, tool_result),
            "open_url" | "open_application" => self.verify_gui_launch(tool_result),
            _ => self.verify_default(tool_result),
        }
    }
}

/// Convenience: an observation-less failure message for a replan.
pub fn replan_message(tool_name: &str, reason: &str) -> String {
    format!(
        "工具执行完成，但结果验证失败。\n\n工具：{}\n验证结果：{}\n建议：重新规划并尝试其他方案",
        tool_name, reason
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::trait_def::ToolResult;

    fn verifier() -> DefaultVerifier {
        DefaultVerifier::new(std::env::temp_dir().to_str().unwrap_or("."))
    }

    fn args(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
    }

    #[tokio::test]
    async fn write_file_verified_success() {
        let path = std::env::temp_dir().join(format!("yilian-ver-ok-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&path, "YiLian verification success").unwrap();
        let a = serde_json::json!({ "path": path.display().to_string(), "content": "YiLian verification success" });
        let r = verifier()
            .verify("write_file", &a, &ToolResult::success("ok"))
            .await;
        assert!(r.success, "reason: {}", r.reason);
        assert!(!r.should_replan);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn write_file_ok_but_missing_fails() {
        // ToolResult.ok = true, but the target file does not exist.
        let a = args(r#"{"path": "C:/definitely/not/here-yilian.txt", "content": "x"}"#);
        let r = verifier()
            .verify("write_file", &a, &ToolResult::success("ok"))
            .await;
        assert!(!r.success);
        assert!(r.should_replan);
    }

    #[tokio::test]
    async fn write_file_content_mismatch_fails() {
        let path =
            std::env::temp_dir().join(format!("yilian-ver-mismatch-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&path, "different content").unwrap();
        let a =
            serde_json::json!({ "path": path.display().to_string(), "content": "expected text" });
        let r = verifier()
            .verify("write_file", &a, &ToolResult::success("ok"))
            .await;
        assert!(!r.success);
        assert!(r.should_replan);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn edit_file_replacement_present_succeeds() {
        let path =
            std::env::temp_dir().join(format!("yilian-ver-edit-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&path, "hello new text world").unwrap();
        let a = serde_json::json!({ "path": path.display().to_string(), "find": "old", "replace": "new text" });
        let r = verifier()
            .verify("edit_file", &a, &ToolResult::success("ok"))
            .await;
        assert!(r.success, "reason: {}", r.reason);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn unknown_tool_default_verifier_no_panic() {
        // ok → success
        let ok = verifier()
            .verify("some_tool", &args("{}"), &ToolResult::success("ok"))
            .await;
        assert!(ok.success);
        // not ok → failure + replan
        let fail = verifier()
            .verify("some_tool", &args("{}"), &ToolResult::error("boom"))
            .await;
        assert!(!fail.success);
        assert!(fail.should_replan);
    }

    #[tokio::test]
    async fn structured_gui_tools_report_their_built_in_foreground_verification() {
        let result = verifier()
            .verify(
                "open_application",
                &serde_json::json!({"application": "QQ"}),
                &ToolResult::success("QQ窗口已显示在桌面前台"),
            )
            .await;

        assert!(result.success);
        assert!(result.reason.contains("可见且位于桌面前台"));
    }

    #[tokio::test]
    async fn verifier_degrades_gracefully_on_weird_input() {
        // Directory path: exists but no text preview — no panic, explicit failure.
        let dir = std::env::temp_dir();
        let a = serde_json::json!({ "path": dir.display().to_string(), "content": "x" });
        let r = verifier()
            .verify("write_file", &a, &ToolResult::success("ok"))
            .await;
        assert!(!r.success); // cannot confirm content → failure, not a false success
    }

    #[tokio::test]
    async fn process_kill_running_process_fails() {
        // Kill on our own (running) pid → "仍在运行" → failure.
        let pid = std::process::id();
        let a = args(&format!(r#"{{"action": "kill", "pid": {}}}"#, pid));
        let r = verifier()
            .verify("process", &a, &ToolResult::success("ok"))
            .await;
        assert!(!r.success);
        assert!(r.should_replan);
    }

    #[tokio::test]
    async fn process_kill_missing_pid_fails() {
        let a = args(r#"{"action": "kill"}"#);
        let r = verifier()
            .verify("process", &a, &ToolResult::success("ok"))
            .await;
        assert!(!r.success);
    }

    #[test]
    fn replan_message_is_actionable() {
        let m = replan_message("write_file", "目标文件不存在");
        assert!(m.contains("验证失败"));
        assert!(m.contains("重新规划"));
    }
}
