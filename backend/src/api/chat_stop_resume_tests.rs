use super::*;
use axum::{response::IntoResponse, routing::post, Router};
use std::sync::atomic::{AtomicUsize, Ordering};

fn tab() -> crate::browser::BrowserTab {
    crate::browser::BrowserTab {
        session_id: "fixture-session".into(),
        tab_id: "fixture-tab".into(),
        navigation_id: 1,
        workspace_id: None,
        site_id: None,
        url: "http://127.0.0.1:18765/form".into(),
        title: "Local controlled form".into(),
        mode: "embedded".into(),
        loading: false,
        can_go_back: false,
        can_go_forward: false,
        error: None,
        ephemeral: true,
    }
}
struct CountingBrowserHost(AtomicUsize);
#[async_trait::async_trait]
impl crate::browser::BrowserHost for CountingBrowserHost {
    fn tabs(&self) -> Vec<crate::browser::BrowserTab> {
        vec![tab()]
    }
    async fn dispatch(
        &self,
        request: crate::browser::BrowserRequest,
        _cancel: CancellationToken,
    ) -> Result<crate::browser::BrowserReply, crate::browser::BrowserError> {
        assert_eq!(request.action, crate::browser::BrowserAction::Fill);
        assert_eq!(request.target, Some(tab().target()));
        assert_eq!(request.payload["observation_id"], "fixture-observation");
        assert_eq!(request.payload["element_id"], "fixture-input");
        assert_eq!(request.payload["value"], "controlled value");
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(crate::browser::BrowserReply::verified(
            json!({"tab":tab(),"filled":true}),
        ))
    }
}
async fn run_continuation(stop_before_poll: bool) {
    let f = Fixture::new();
    f.turn();
    let host = Arc::new(CountingBrowserHost(AtomicUsize::new(0)));
    f.server.browser_runtime.install_host(host.clone()).unwrap();
    let args = json!({"target":tab().target(),"observation_id":"fixture-observation","element_id":"fixture-input","value":"controlled value"});
    // A real paused turn persists the assistant's original tool call before
    // creating approval. Without this pair, the engine correctly rejects an
    // orphan tool result before contacting the model.
    f.server.db.add_message(&MessageRow {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: f.conversation.clone(),
        role: "assistant".into(), content: String::new(),
        tool_calls: Some(json!([{"id":"controlled-fill-call","type":"function","function":{"name":"visible_browser_fill","arguments":args.to_string()}}]).to_string()),
        tool_call_id: None, tool_name: None, tool_result: None,
        created_at: chrono::Utc::now().timestamp_millis(),
    }).unwrap();
    // Both controls use the same valid binding, target generation and original call.
    let a = f.server.approval_store.create(
        f.conversation.clone(),
        "controlled-fill-call".into(),
        "visible_browser_fill".into(),
        args,
        RiskLevel::High,
        "controlled fixture".into(),
        "local-user".into(),
    );
    let model_calls = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router=Router::new().route("/chat/completions",post({let calls=model_calls.clone();move ||{let calls=calls.clone();async move {
        calls.fetch_add(1,Ordering::SeqCst);
        ([("Content-Type","text/event-stream")],"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Local fixture complete\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n").into_response()
    }}}));
    let provider = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    {
        let mut config = f.server.config.write();
        config.model.base_url = format!("http://{address}");
        config.model.api_key = "local-fixture-only".into();
        config.model.api_key_env.clear();
        config.model.invoke_timeout_ms = 2000;
    }
    let stream = match crate::api::approvals::approve_handler(
        State(f.server.clone()),
        Path(a.approval_id.clone()),
        Json(crate::api::approvals::ApprovalDecisionRequest {
            conversation_id: Some(f.conversation.clone()),
            attestation_id: None,
        }),
    )
    .await
    {
        Ok(stream) => stream,
        Err((status, message)) => panic!("approval unexpectedly refused {status}: {message}"),
    };
    // Current-thread executor: preparation hasn't polled, but its token exists.
    if stop_before_poll {
        let token = f
            .server
            .active_tasks
            .lock()
            .get(&f.conversation)
            .unwrap()
            .clone();
        assert!(stop(f.server.clone(), &f.conversation).await.unwrap());
        assert!(token.is_cancelled());
    }
    let guard = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        crate::api::chat::lifecycle::acquire_turn(&f.server, &f.conversation),
    )
    .await
    .unwrap();
    if stop_before_poll {
        assert_eq!(host.0.load(Ordering::SeqCst), 0);
        assert_eq!(model_calls.load(Ordering::SeqCst), 0);
        assert!(cancelled(&f.server.db, &f.conversation).unwrap());
        let history = f.server.db.load_chat_messages(&f.conversation).unwrap();
        crate::agent::state::tool_protocol::validate(&history)
            .expect("cancel before dispatch must close the original assistant/tool pair");
        let results: Vec<_> = history
            .iter()
            .filter(|message| {
                message.role == "tool"
                    && message.tool_call_id.as_deref() == Some("controlled-fill-call")
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(results[0].content.as_deref().unwrap())
                .unwrap()["state"],
            "not_executed"
        );
    } else {
        assert_eq!(
            host.0.load(Ordering::SeqCst),
            1,
            "positive control must execute the approved Fill through the Gateway"
        );
        assert_eq!(
            model_calls.load(Ordering::SeqCst),
            1,
            "positive control must reach the controlled model continuation; persisted error: {:?}",
            f.server
                .db
                .get_conversation(&f.conversation)
                .unwrap()
                .run_error
        );
    }
    drop(guard);
    drop(stream);
    if stop_before_poll {
        f.late_approve(&a).await;
        // A new user turn in this same conversation must remain usable after stop.
        use tower::ServiceExt;
        let response = crate::api::build_router(f.server.clone()).oneshot(
            axum::http::Request::builder().method("POST").uri("/api/chat")
                .header("X-Yilian-Control-Session", "c".repeat(64))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(json!({"conversation_id":f.conversation,"message":"Continue after stopping the unexecuted action"}).to_string())).unwrap()
        ).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            axum::body::to_bytes(response.into_body(), 1024 * 1024),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            host.0.load(Ordering::SeqCst),
            0,
            "new chat must not replay the cancelled Fill"
        );
        assert_eq!(
            model_calls.load(Ordering::SeqCst),
            1,
            "same conversation must resume ordinary chat after cancellation"
        );
        assert_eq!(
            f.server
                .db
                .get_conversation(&f.conversation)
                .unwrap()
                .run_status,
            crate::db::ConversationRunStatus::Completed
        );
    }
    provider.abort();
}
#[tokio::test]
async fn approved_continuation_stop_prevents_valid_host_action_and_model() {
    run_continuation(true).await;
}
#[tokio::test]
async fn approved_continuation_positive_control_executes_valid_host_action_and_model() {
    run_continuation(false).await;
}

#[test]
fn goal_registry_cancellation_reaches_early_resume_and_actual_execution_token() {
    use crate::modules::task::{proofloop::*, TaskGraphId};
    let f = Fixture::new();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/proofloop/declared-two-node.json"
    ))
    .unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.budget.max_total_tokens = 1_000_000;
    let snapshot: CapabilitySnapshot = serde_json::from_value(fixture["snapshot"].clone()).unwrap();
    let candidate: CandidatePlan = serde_json::from_value(fixture["candidate"].clone()).unwrap();
    let graph = TaskGraphId::generate();
    let now = chrono::Utc::now().timestamp_millis();
    let draft = ProofDraft {
        source: None,
        planning_review: None,
        draft_id: "resume-token-association".into(),
        revision: 1,
        graph_id: graph.to_string(),
        contract: contract.clone(),
        snapshot: snapshot.clone(),
        candidate: Some(candidate.clone()),
        binding_confirmations: vec![],
        compilation: compile_candidate(&contract, &candidate, &snapshot, &graph, &[]),
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage {
            model_calls: 0,
            total_tokens: Some(0),
            started_at_ms: now,
            elapsed_ms: Some(0),
        },
    };
    f.server.db.save_proof_draft(&draft, now).unwrap();
    DraftService::new(f.server.db.clone())
        .confirm_draft(&f.server.task_world, &draft.draft_id, 1, &snapshot, now)
        .unwrap();
    let resume_cancel = CancellationToken::new();
    let goal_cancel = resume_cancel.clone();
    let run = match crate::api::chat::resolve_goal_chat_run(
        &f.server,
        &f.server.db,
        &crate::agent::delegation::GoalRef {
            goal_id: contract.goal_id,
            goal_version: contract.version,
        },
        goal_cancel,
    ) {
        Ok(run) => run,
        Err(_) => panic!("controlled confirmed goal must resolve"),
    };
    let actual_host_cancel = run.scope.cancel.child_token();
    assert!(!resume_cancel.is_cancelled());
    assert!(!actual_host_cancel.is_cancelled());
    f.server.task_world.cancel_task(&graph, now + 1).unwrap();
    assert!(
        resume_cancel.is_cancelled(),
        "graph cancellation must reach preparation's registered token"
    );
    assert!(run.scope.cancel.is_cancelled());
    assert!(actual_host_cancel.is_cancelled());
    let _ = run.finish();
}
