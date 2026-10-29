use serde_json::Value;

// A repeated tool name is normal for multi-step input, reads and edits.
// Stop early only when the same request keeps producing the same failure.
const MAX_IDENTICAL_TOOL_FAILURES: usize = 3;

#[derive(Default)]
pub(super) struct ToolLoopGuard {
    last_failure: Option<(String, Value, Value)>,
    consecutive_failures: usize,
}

impl ToolLoopGuard {
    pub(super) fn observe(
        &mut self,
        tool: &str,
        arguments: &Value,
        failure: Option<Value>,
    ) -> Result<(), String> {
        let Some(failure) = failure else {
            self.last_failure = None;
            self.consecutive_failures = 0;
            return Ok(());
        };
        let current = (tool.to_owned(), arguments.clone(), failure);
        if self.last_failure.as_ref() == Some(&current) {
            self.consecutive_failures += 1;
        } else {
            self.last_failure = Some(current);
            self.consecutive_failures = 1;
        }
        if self.consecutive_failures >= MAX_IDENTICAL_TOOL_FAILURES {
            return Err(format!(
                "工具 {tool} 的相同调用连续 {} 次未成功，且失败结果相同，已停止本轮执行；请根据工具结果调整操作后重试",
                self.consecutive_failures
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_successful_actions_can_repeat() {
        let mut guard = ToolLoopGuard::default();
        for _ in 0..12 {
            guard
                .observe("keyboard", &json!({"action":"press","key":"Down"}), None)
                .unwrap();
        }
    }

    #[test]
    fn identical_failed_requests_stop_after_three_actual_results() {
        let mut guard = ToolLoopGuard::default();
        for _ in 0..2 {
            guard
                .observe(
                    "read_file",
                    &json!({"path":"missing.txt"}),
                    Some(json!("not found")),
                )
                .unwrap();
        }
        let error = guard
            .observe(
                "read_file",
                &json!({"path":"missing.txt"}),
                Some(json!("not found")),
            )
            .unwrap_err();
        assert!(error.contains("连续 3 次未成功"));
        assert!(!error.contains("missing.txt"));
    }

    #[test]
    fn different_parameters_results_or_tools_start_a_new_failure_sequence() {
        let mut guard = ToolLoopGuard::default();
        for (tool, args, failure) in [
            ("read_file", json!({"path":"a"}), json!("not found")),
            ("read_file", json!({"path":"a"}), json!("not found")),
            ("read_file", json!({"path":"b"}), json!("not found")),
            ("read_file", json!({"path":"b"}), json!("not found")),
            ("read_file", json!({"path":"b"}), json!("access denied")),
            ("read_file", json!({"path":"b"}), json!("access denied")),
            ("list_dir", json!({"path":"b"}), json!("access denied")),
            ("list_dir", json!({"path":"b"}), json!("access denied")),
        ] {
            guard.observe(tool, &args, Some(failure)).unwrap();
        }
    }

    #[test]
    fn successful_operation_resets_previous_failures() {
        let mut guard = ToolLoopGuard::default();
        for _ in 0..2 {
            guard
                .observe("read_file", &json!({}), Some(json!("missing")))
                .unwrap();
        }
        guard.observe("keyboard", &json!({}), None).unwrap();
        for _ in 0..2 {
            guard
                .observe("read_file", &json!({}), Some(json!("missing")))
                .unwrap();
        }
        assert!(guard
            .observe("read_file", &json!({}), Some(json!("missing")))
            .is_err());
    }

    #[test]
    fn json_whitespace_and_key_order_do_not_disguise_identical_failures() {
        let mut guard = ToolLoopGuard::default();
        for text in [
            r#"{"action":"press","key":"Enter"}"#,
            r#"{ "key": "Enter", "action": "press" }"#,
        ] {
            guard
                .observe(
                    "keyboard",
                    &serde_json::from_str(text).unwrap(),
                    Some(json!("not focused")),
                )
                .unwrap();
        }
        assert!(guard
            .observe(
                "keyboard",
                &json!({"action":"press","key":"Enter"}),
                Some(json!("not focused"))
            )
            .is_err());
    }
}
