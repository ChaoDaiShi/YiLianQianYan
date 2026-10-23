use super::*;
use crate::integrations::llm::types::ToolCallFunction;

fn call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        call_type: "function".into(),
        function: ToolCallFunction {
            name: "read_file".into(),
            arguments: "{}".into(),
        },
    }
}

#[test]
fn interrupted_batch_distinguishes_never_started_from_started_without_result() {
    let mut state = AgentState::new("fixture".into());
    state.add_user_message("request".into());
    state.add_assistant_message(
        None,
        Some(vec![call("finished"), call("started"), call("not-started")]),
    );
    state.mark_tool_started("finished");
    state.add_tool_result(
        "finished".into(),
        "read_file".into(),
        "actual result".into(),
    );
    state.mark_tool_started("started");
    state.close_interrupted_tool_batch();
    assert!(validate(&state.messages).is_ok());
    assert_eq!(
        state
            .messages
            .iter()
            .filter(|message| message.tool_call_id.as_deref() == Some("finished"))
            .count(),
        1
    );
    for (id, expected) in [
        ("started", "result_unknown"),
        ("not-started", "not_executed"),
    ] {
        let message = state
            .messages
            .iter()
            .find(|message| message.tool_call_id.as_deref() == Some(id))
            .unwrap();
        let result: serde_json::Value =
            serde_json::from_str(message.content.as_deref().unwrap()).unwrap();
        assert_eq!(result["status"], expected);
    }
    let count = state.messages.len();
    state.close_interrupted_tool_batch();
    assert_eq!(state.messages.len(), count);
}

#[test]
fn loop_guard_tail_is_closed_but_corrupt_old_prefix_is_never_repaired() {
    let mut state = AgentState::new("fixture".into());
    state.add_user_message("question".into());
    state.add_assistant_message(None, Some(vec![call("loop-guard-call")]));
    state.close_interrupted_tool_batch();
    assert!(validate(&state.messages).is_ok());
    assert!(state
        .messages
        .last()
        .unwrap()
        .content
        .as_deref()
        .unwrap()
        .contains("not_executed"));
    let mut old = AgentState::new("fixture".into());
    old.add_assistant_message(None, Some(vec![call("old-open-call")]));
    old.add_user_message("a later retry".into());
    let count = old.messages.len();
    assert!(validate(&old.messages).is_err());
    old.close_interrupted_tool_batch();
    assert_eq!(old.messages.len(), count);
}

#[test]
fn pending_approval_tail_is_rejected_until_its_real_tool_result_is_present() {
    let mut state = AgentState::new("fixture".into());
    state.add_user_message("request".into());
    state.add_assistant_message(None, Some(vec![call("approval-call")]));
    assert!(validate(&state.messages).is_err());
    state.add_tool_result(
        "approval-call".into(),
        "read_file".into(),
        "actual approved result".into(),
    );
    assert!(validate(&state.messages).is_ok());
    state.add_tool_result(
        "approval-call".into(),
        "read_file".into(),
        "duplicate".into(),
    );
    assert!(validate(&state.messages).is_err());
}
