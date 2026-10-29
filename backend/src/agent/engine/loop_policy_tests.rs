use super::*;
use crate::{
    agent::verifier::{VerificationResult, Verifier},
    config::types::{ModelConfig, PermissionMode, SandboxConfig, SandboxProfile},
    integrations::secret::{InMemorySecretStore, SecretResolver},
    tools::{Tool, ToolResult},
};
use async_trait::async_trait;
use axum::{extract::State, http::header::CONTENT_TYPE, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct KeyboardFixture(Arc<AtomicUsize>);

#[async_trait]
impl Tool for KeyboardFixture {
    fn name(&self) -> &str {
        "keyboard"
    }
    fn description(&self) -> &str {
        "Controlled keyboard fixture; no desktop input"
    }
    fn parameters(&self) -> Value {
        json!({"type":"object"})
    }
    async fn execute(&self, _: Value) -> ToolResult {
        self.0.fetch_add(1, Ordering::SeqCst);
        ToolResult::success("input action accepted")
    }
}

struct FixtureVerifier(bool);

#[async_trait]
impl Verifier for FixtureVerifier {
    async fn verify(&self, _: &str, _: &Value, _: &ToolResult) -> VerificationResult {
        if self.0 {
            VerificationResult::success("fixture operation observed", None)
        } else {
            VerificationResult::failure("target still unavailable", None)
        }
    }
}

struct ScriptedModel {
    rounds: Vec<Vec<Value>>,
    requests: AtomicUsize,
}

async fn model(
    State(model): State<Arc<ScriptedModel>>,
    Json(body): Json<Value>,
) -> impl axum::response::IntoResponse {
    let messages: Vec<crate::integrations::llm::types::ChatMessage> =
        serde_json::from_value(body["messages"].clone()).unwrap();
    super::super::state::tool_protocol::validate(&messages).unwrap();
    let index = model.requests.fetch_add(1, Ordering::SeqCst);
    let (delta, finish) = if let Some(round) = model.rounds.get(index) {
        let calls: Vec<Value> = round
            .iter()
            .enumerate()
            .map(|(offset, args)| {
                json!({
                    "index":offset,"id":format!("call-{index}-{offset}"),"type":"function",
                    "function":{"name":"keyboard","arguments":args.to_string()}
                })
            })
            .collect();
        (json!({"tool_calls":calls}), "tool_calls")
    } else {
        (json!({"content":"Operation sequence finished"}), "stop")
    };
    (
        [(CONTENT_TYPE, "text/event-stream")],
        format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({
                "id":format!("response-{index}"),
                "choices":[{"index":0,"delta":delta,"finish_reason":finish}],
                "usage":{"prompt_tokens":20,"completion_tokens":10,"total_tokens":30}
            })
        ),
    )
}

struct Scenario {
    outcome: Result<RunOutcome, String>,
    state: AgentState,
    executed: usize,
    requests: usize,
    events: Vec<AgentEvent>,
}

async fn run(rounds: Vec<Vec<Value>>, verified: bool, cancelled: bool) -> Scenario {
    let model_state = Arc::new(ScriptedModel {
        rounds,
        requests: AtomicUsize::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/chat/completions", post(model))
        .with_state(Arc::clone(&model_state));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = LlmClient::new(
        &ModelConfig {
            provider: "lmstudio".into(),
            name: "tool-loop-fixture".into(),
            base_url: format!("http://{address}"),
            api_key: String::new(),
            api_key_env: String::new(),
            api_key_ref: None,
            invoke_timeout_ms: 5000,
            ..ModelConfig::default()
        },
        Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new()))),
    );
    let count = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(KeyboardFixture(Arc::clone(&count))));
    let registry = Arc::new(registry);
    let mut config = AppConfig::default();
    // Only the inert test keyboard is authorized here; production approval stays intact.
    config.permissions.mode = PermissionMode::FullAccess;
    let gateway = SecurityExecutionGateway::with_sandbox_registry_and_verifier(
        SandboxConfig {
            profile: SandboxProfile::Open,
            writable_paths: vec![],
            denied_write_paths: vec![],
        },
        ".",
        Arc::clone(&registry),
        Arc::new(FixtureVerifier(verified)),
    )
    .with_permissions_config(Arc::new(parking_lot::RwLock::new(config.clone())))
    .with_db(Arc::new(
        crate::db::Database::new(std::path::Path::new(":memory:")).unwrap(),
    ));
    let mut state = AgentState::new("Execute the requested operations, then respond.".into());
    state.add_user_message("Run the keyboard action sequence".into());
    let token = CancellationToken::new();
    if cancelled {
        token.cancel();
    }
    let (sender, mut receiver) = tokio::sync::mpsc::channel(512);
    let outcome = run_react_loop_with_channel(
        &mut state,
        &client,
        &registry,
        &ApprovalStore::new(),
        &gateway,
        &config,
        "loop-fixture",
        &token,
        &sender,
        &LogBuffer::new(64),
    )
    .await;
    server.abort();
    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    super::super::state::tool_protocol::validate(&state.messages).unwrap();
    Scenario {
        outcome,
        state,
        executed: count.load(Ordering::SeqCst),
        requests: model_state.requests.load(Ordering::SeqCst),
        events,
    }
}

fn navigation_actions() -> Vec<Value> {
    vec![
        json!({"action":"combo","keys":["ctrl","l"]}),
        json!({"action":"type","text":"https://example.com","target_application":"Edge"}),
        json!({"action":"press","key":"Enter"}),
        json!({"action":"press","key":"Down"}),
    ]
}

#[tokio::test]
async fn keyboard_navigation_completes_across_separate_model_rounds_and_a_single_batch() {
    for rounds in [
        navigation_actions()
            .into_iter()
            .map(|args| vec![args])
            .collect(),
        vec![navigation_actions()],
    ] {
        let scenario = run(rounds, true, false).await;
        assert!(matches!(scenario.outcome, Ok(RunOutcome::Done { .. })));
        assert_eq!(scenario.executed, 4);
        assert_eq!(
            scenario
                .events
                .iter()
                .filter(|event| event.event_type == "tool_end")
                .count(),
            4
        );
    }
}

#[tokio::test]
async fn successful_identical_keypresses_can_repeat_more_than_three_times() {
    let scenario = run(
        vec![vec![json!({"action":"press","key":"Down"})]; 8],
        true,
        false,
    )
    .await;
    assert!(matches!(scenario.outcome, Ok(RunOutcome::Done { .. })));
    assert_eq!(scenario.executed, 8);
}

#[tokio::test]
async fn identical_verification_failures_stop_after_recording_three_executed_results() {
    let scenario = run(
        vec![vec![json!({"action":"press","key":"Enter"})]; 5],
        false,
        false,
    )
    .await;
    assert!(scenario.outcome.err().unwrap().contains("连续 3 次未成功"));
    assert_eq!(scenario.executed, 3);
    assert_eq!(scenario.requests, 3);
    assert_eq!(
        scenario
            .events
            .iter()
            .filter(|event| event.event_type == "verification"
                && event.verification_success == Some(false))
            .count(),
        3
    );
    assert_eq!(
        scenario
            .state
            .messages
            .iter()
            .filter(|message| message.role == "tool")
            .count(),
        3
    );
}

#[tokio::test]
async fn successful_repetition_still_obeys_the_total_round_limit() {
    let scenario = run(
        vec![vec![json!({"action":"press","key":"Down"})]; MAX_ITERATIONS + 1],
        true,
        false,
    )
    .await;
    assert!(scenario.outcome.err().unwrap().contains("20 轮执行上限"));
    assert_eq!(scenario.executed, MAX_ITERATIONS);
    assert_eq!(scenario.requests, MAX_ITERATIONS);
}

#[tokio::test]
async fn cancellation_stops_before_model_or_tool_execution() {
    let scenario = run(vec![navigation_actions()], true, true).await;
    assert_eq!(scenario.outcome.err().unwrap(), "已取消");
    assert_eq!(scenario.executed, 0);
    assert_eq!(scenario.requests, 0);
}
