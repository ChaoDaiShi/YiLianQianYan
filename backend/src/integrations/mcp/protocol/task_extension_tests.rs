use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};

const TASK_CREATE_FIXTURE: &str = r#"{"resultType":"task","taskId":"786512e2-9e0d-44bd-8f29-789f320fe840","status":"working","pollIntervalMs":5000}"#;
const TASK_CREATE_FIXTURE_SHA256: &str =
    "f8d328f7f96ae26f8476b89c4fc866b2811cda53f489d05a5747093217d87773";

// Reduced CreateTaskResult fixture based on the pinned 2026-07-28 Tasks spec:
// https://modelcontextprotocol.github.io/ext-tasks/specification/2026-07-28/tasks.html

#[test]
fn legacy_and_complete_tool_results_keep_the_ordinary_result_path() {
    let legacy = json!({"content": [{"type":"text","text":"done"}]});
    let complete = json!({"resultType":"complete","content":[]});
    assert!(
        matches!(parse_tool_call_result(legacy.clone()), McpTaskAugmentedResult::Complete(value) if value == legacy)
    );
    assert!(
        matches!(parse_tool_call_result(complete.clone()), McpTaskAugmentedResult::Complete(value) if value == complete)
    );
}

#[test]
fn task_create_result_becomes_a_handle_and_keeps_poll_interval() {
    let fixture: Value = serde_json::from_str(TASK_CREATE_FIXTURE).unwrap();
    assert!(
        matches!(parse_tool_call_result(fixture), McpTaskAugmentedResult::Task(McpTaskHandle {
        task_id, status: McpTaskStatus::Working, poll_interval_ms: Some(5000)
    }) if task_id == "786512e2-9e0d-44bd-8f29-789f320fe840")
    );
    let digest = format!("{:x}", Sha256::digest(TASK_CREATE_FIXTURE.as_bytes()));
    assert_eq!(
        digest, TASK_CREATE_FIXTURE_SHA256,
        "pinned official wire fixture changed"
    );
}

#[test]
fn get_results_preserve_input_and_terminal_payloads() {
    let waiting = json!({
        "resultType":"complete", "taskId":"task-1", "status":"input_required",
        "inputRequests":{"approval":{"method":"elicitation/create","params":{"message":"Continue?"}}}
    });
    let completed = json!({
        "resultType":"complete", "taskId":"task-1", "status":"completed",
        "result":{"content":[{"type":"text","text":"done"}]}
    });
    let waiting = parse_task_snapshot(&waiting).unwrap();
    assert_eq!(waiting.status, McpTaskStatus::InputRequired);
    assert!(waiting.input_requests.is_some());
    assert!(!waiting.status.is_terminal());
    let completed = parse_task_snapshot(&completed).unwrap();
    assert_eq!(completed.status, McpTaskStatus::Completed);
    assert!(completed.status.is_terminal());
    assert!(completed.result.is_some());
}

#[test]
fn task_methods_carry_the_extension_capability_and_preserve_other_metadata() {
    let mut params = tasks_get_params("task-1");
    params["_meta"]["trace"] = json!("request-7");
    attach_tasks_capability(&mut params);
    assert_eq!(params["taskId"], "task-1");
    assert_eq!(params["_meta"]["trace"], "request-7");
    assert_eq!(
        params["_meta"]["io.modelcontextprotocol/clientCapabilities"]["extensions"]
            [TASKS_EXTENSION_ID],
        json!({})
    );
    let update = tasks_update_params(
        "task-1",
        json!({"answer":{"action":"accept","content":{"input":"yes"}}}),
    );
    assert_eq!(update["inputResponses"]["answer"]["action"], "accept");
    assert_eq!(tasks_cancel_params("task-1")["taskId"], "task-1");
}
