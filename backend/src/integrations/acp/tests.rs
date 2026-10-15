use super::model::*;
use serde_json::json;
use std::path::Path;

#[test]
fn initialization_uses_acp_v1_without_unimplemented_client_capabilities() {
    let params = initialize_params("YiLianQianYan", "v1.2-adapter-pilot");
    assert_eq!(params["protocolVersion"], 1);
    assert_eq!(params["clientCapabilities"], json!({}));
    assert_eq!(params["clientInfo"]["name"], "YiLianQianYan");
    assert!(params["clientCapabilities"].get("fs").is_none());
    assert!(params["clientCapabilities"].get("terminal").is_none());
}

#[test]
fn session_and_prompt_params_use_the_task_cwd_and_text_content_shape() {
    let cwd = Path::new("F:/task-sandbox");
    let session = new_session_params(cwd);
    assert_eq!(session["cwd"], "F:/task-sandbox");
    assert_eq!(session["mcpServers"], json!([]));
    let prompt = prompt_params("session-7", "write the expected file");
    assert_eq!(prompt["sessionId"], "session-7");
    assert_eq!(
        prompt["prompt"][0],
        json!({"type":"text","text":"write the expected file"})
    );
    assert_eq!(cancel_params("session-7"), json!({"sessionId":"session-7"}));
}

#[test]
fn usage_update_is_context_usage_and_does_not_claim_total_tokens() {
    let usage = parse_context_usage(&json!({
        "sessionUpdate":"usage_update", "used":53000, "size":200000,
        "cost":{"amount":0.045,"currency":"USD"}
    }))
    .unwrap();
    assert_eq!(usage.context_used, 53_000);
    assert_eq!(usage.context_size, 200_000);
    assert_eq!(usage.cost_amount, Some(0.045));
    assert_eq!(usage.currency.as_deref(), Some("USD"));
    assert_eq!(usage.total_tokens, None);
}

#[test]
fn agent_text_is_read_only_from_agent_message_chunks() {
    let update = json!({
        "sessionUpdate":"agent_message_chunk",
        "content":{"type":"text","text":"Created report.txt"}
    });
    assert_eq!(
        agent_message_text(&update).as_deref(),
        Some("Created report.txt")
    );
    assert_eq!(agent_message_text(&json!({"sessionUpdate":"plan"})), None);
}

#[test]
fn exposed_tool_name_uses_the_existing_subagent_security_profile() {
    assert_eq!(
        exposed_tool_name("Codex-Agent"),
        Some("subagent_acp_codex_agent_run".to_string())
    );
    assert_eq!(exposed_tool_name("---"), None);
}
