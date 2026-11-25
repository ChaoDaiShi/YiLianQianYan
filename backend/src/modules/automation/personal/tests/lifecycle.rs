use super::*;

#[test]
fn personal_assistant_empty_current_turn_does_not_borrow_previous_report() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    f.server
        .db
        .persist_chat_message(
            conversation(&a),
            &crate::integrations::llm::types::ChatMessage {
                role: "assistant".into(),
                content: Some("Old report from another round".into()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            1,
        )
        .unwrap();
    let item = claim(&f, &a);
    let user = crate::db::MessageRow {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conversation(&a).into(),
        role: "user".into(),
        content: "new round".into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    f.server.db.accept_conversation_turn(&user).unwrap();
    mutate(&f.server, &item.id, |v| v.assistant_turn_id = Some(user.id)).unwrap();
    finish_turn(
        &f.server,
        conversation(&a),
        ConversationRunStatus::Completed,
        None,
    )
    .unwrap();
    let latest = attempts(&f.server, &a.id).unwrap().remove(0);
    assert_eq!(latest.status, "completed");
    assert!(latest.report.is_none());
}

#[tokio::test]
async fn personal_assistant_stop_during_search_closes_uncertain_protocol_before_explicit_new_turn()
{
    use crate::safety::grant::GrantEffect;
    let f = Fixture::new();
    let a = discovery_delegation(&f);
    std::fs::create_dir_all(f.root.join("scan")).unwrap();
    for index in 0..600 {
        std::fs::write(
            f.root.join(format!("scan/{index:04}.txt")),
            "controlled notes",
        )
        .unwrap();
    }
    read_grant(&f, &f.root, true, GrantEffect::Allow);
    let (service, calls) = model(&f, 7).await;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if f.server
                .db
                .get_execution_history(conversation(&a))
                .unwrap()
                .iter()
                .any(|r| r.name == "grep" && r.execution_status == "running")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    stop(&f.server, &a.id).await.unwrap();
    assert!(f
        .server
        .db
        .conversation_protocol_issue(conversation(&a))
        .unwrap()
        .is_none());
    let stored = f.server.db.get_conversation(conversation(&a)).unwrap();
    assert!(stored.messages.iter().any(|m| m.role == "tool"));
    assert!(stored
        .execution_history
        .iter()
        .any(|r| r.name == "grep" && r.execution_status == "failed"));
    let original = attempts(&f.server, &a.id).unwrap()[0].clone();
    assert_eq!(original.status, "cancelled");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    service.abort();
    let (service, calls) = model(&f, 5).await;
    let paused = f.server.db.get_automation(&a.id).unwrap().unwrap();
    let mut request = management_request(&paused);
    request.acknowledged = true;
    request.reviewed_occurrence_id = Some(original.id);
    super::super::control::start(&f.server, &a.id, request)
        .await
        .unwrap();
    wait_status(&f, &a, "completed").await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    service.abort();
}

#[tokio::test]
async fn personal_assistant_explicit_stop_repairs_persisted_interrupted_tool_without_replaying() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let item = claim(&f, &a);
    f.server
        .db
        .persist_chat_message(
            conversation(&a),
            &crate::integrations::llm::types::ChatMessage {
                role: "assistant".into(),
                content: None,
                tool_calls: Some(vec![crate::integrations::llm::types::ToolCall {
                    id: "interrupted-read".into(),
                    call_type: "function".into(),
                    function: crate::integrations::llm::types::ToolCallFunction {
                        name: "read_file".into(),
                        arguments: json!({"path":f.root.join("unknown.txt")}).to_string(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    mutate(&f.server, &item.id, |v| v.status = "interrupted".into()).unwrap();
    f.server
        .db
        .set_conversation_run_status(conversation(&a), ConversationRunStatus::Interrupted, None)
        .unwrap();
    assert!(f
        .server
        .db
        .conversation_protocol_issue(conversation(&a))
        .unwrap()
        .is_some());
    let before = super::super::management::detail(&f.server, &a.id).unwrap();
    assert_eq!(before["actions"]["can_start"], false);
    assert_eq!(before["actions"]["can_stop"], true);
    stop(&f.server, &a.id).await.unwrap();
    assert!(f
        .server
        .db
        .conversation_protocol_issue(conversation(&a))
        .unwrap()
        .is_none());
    let stored = f.server.db.get_conversation(conversation(&a)).unwrap();
    assert!(stored
        .messages
        .iter()
        .any(|m| m.role == "tool" && m.content.contains("结果不确定")));
    assert!(stored
        .execution_history
        .iter()
        .any(|r| r.tool_call_id == "interrupted-read" && r.verification_status == "failed"));
    assert!(!f.root.join("unknown.txt").exists());
    let after = super::super::management::detail(&f.server, &a.id).unwrap();
    assert_eq!(after["actions"]["requires_review"], true);
}

#[tokio::test]
async fn personal_assistant_waits_for_answer_and_stop_cancels_pending_input() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let (service, _) = model(&f, 1).await;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    wait_status(&f, &a, "waiting_input").await;
    assert_eq!(
        super::super::super::run_now(&f.server, &a.id)
            .await
            .unwrap()
            .status,
        "skipped"
    );
    assert_eq!(
        f.server
            .db
            .pending_conversation_inputs(conversation(&a))
            .unwrap()
            .len(),
        1
    );
    stop(&f.server, &a.id).await.unwrap();
    assert!(f
        .server
        .db
        .pending_conversation_inputs(conversation(&a))
        .unwrap()
        .is_empty());
    assert_eq!(
        f.server
            .db
            .get_conversation(conversation(&a))
            .unwrap()
            .run_status,
        ConversationRunStatus::Cancelled
    );
    service.abort();
}

#[tokio::test]
async fn personal_assistant_failure_requires_manual_followup_and_recovery_never_replays() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let (service, _) = model(&f, 3).await;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    wait_status(&f, &a, "failed").await;
    let mut item = occurrence(&a, 2, false, false);
    assert!(f
        .server
        .db
        .claim_automation_occurrence(&a, a.next_run_at, &mut item, true)
        .unwrap());
    dispatch(&f.server, &a, &mut item).await.unwrap();
    assert_eq!(item.status, "blocked");
    let mut interrupted = claim(&f, &a);
    recover(&f.server, &mut interrupted).unwrap();
    assert_eq!(interrupted.status, "interrupted");
    let reopened = crate::db::Database::new(&f.root.join("state.db")).unwrap();
    assert!(reopened
        .automation_history(&a.id)
        .unwrap()
        .iter()
        .any(|v| v.status == "interrupted"));
    service.abort();
}

#[tokio::test]
async fn personal_assistant_stop_during_model_prevents_further_work() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let (service, calls) = model(&f, 2).await;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    stop(&f.server, &a.id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while f.server.active_tasks.lock().contains_key(conversation(&a)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(attempts(&f.server, &a.id).unwrap()[0].status, "cancelled");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    service.abort();
}

#[tokio::test]
async fn personal_assistant_stop_cancels_persisted_approval_before_any_file_write() {
    let f = Fixture::new();
    let a = f.save(true, 5);
    let (service, _) = model(&f, 4).await;
    f.server.config.write().permissions.mode = crate::config::types::PermissionMode::FullApproval;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    wait_status(&f, &a, "waiting_approval").await;
    assert!(!f.root.join("pending.md").exists());
    let pending = f
        .server
        .approval_store
        .pending_for(conversation(&a))
        .unwrap();
    stop(&f.server, &a.id).await.unwrap();
    assert!(f
        .server
        .approval_store
        .pending_for(conversation(&a))
        .is_none());
    let stored = f.server.db.get_conversation(conversation(&a)).unwrap();
    assert!(stored
        .execution_history
        .iter()
        .all(|record| record.approval_status != "pending"));
    assert_eq!(
        f.server
            .approval_store
            .get(&pending.approval_id)
            .unwrap()
            .status,
        crate::safety::ApprovalStatus::Cancelled
    );
    assert!(!f.root.join("pending.md").exists());
    service.abort();
}

#[tokio::test]
async fn personal_assistant_answer_resumes_same_occurrence_with_restricted_runtime() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let (service, _) = model(&f, 1).await;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    let initial = wait_status(&f, &a, "waiting_input").await;
    let mut restored = initial.clone();
    recover(&f.server, &mut restored).unwrap();
    assert_eq!(
        restored.status, "waiting_input",
        "restoring a persisted question must wait for an explicit answer"
    );
    let question = f
        .server
        .db
        .pending_conversation_inputs(conversation(&a))
        .unwrap()
        .remove(0);
    let response = crate::api::conversation_inputs::answer_handler(
        State(f.server.clone()),
        axum::extract::Path(question.question_id),
        Json(crate::db::conversation_inputs::AnswerInputRequest {
            revision: question.revision,
            option_id: None,
            text: Some("For the project owner".into()),
        }),
    )
    .await
    .unwrap()
    .into_response();
    to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let finished = wait_status(&f, &a, "completed").await;
    assert_eq!(initial.id, finished.id);
    assert_eq!(activity(&f.server, &a.id).unwrap()["used_runs"], 1);
    service.abort();
}

#[tokio::test]
async fn personal_assistant_restart_stop_reconciles_approval_without_process_local_store() {
    let f = Fixture::new();
    let a = f.save(true, 5);
    let (service, _) = model(&f, 4).await;
    f.server.config.write().permissions.mode = crate::config::types::PermissionMode::FullApproval;
    super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    wait_status(&f, &a, "waiting_approval").await;
    let reopened = Arc::new(
        AppServer::new_with_control_session_and_store(
            &f.root.join("state.db"),
            f.root.to_str().unwrap(),
            ControlSession::new("personal-assistant-controlled-reopened-session-000000").unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    );
    assert!(reopened.approval_store.list_pending().is_empty());
    stop(&reopened, &a.id).await.unwrap();
    let state = reopened.db.get_conversation(conversation(&a)).unwrap();
    assert!(state
        .execution_history
        .iter()
        .all(|record| record.approval_status != "pending"));
    crate::agent::state::tool_protocol::validate(
        &reopened.db.load_chat_messages(conversation(&a)).unwrap(),
    )
    .unwrap();
    assert!(!f.root.join("pending.md").exists());
    service.abort();
}

#[tokio::test]
async fn personal_assistant_stale_claim_cannot_start_after_stop_changes_revision() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let mut queued = claim(&f, &a);
    stop(&f.server, &a.id).await.unwrap();
    dispatch(&f.server, &a, &mut queued).await.unwrap();
    assert_eq!(queued.status, "cancelled");
    let runtime = crate::modules::automation::runtime_view(&f.server, &a.id).unwrap();
    assert_eq!(
        runtime.phase,
        crate::modules::automation::model::AutomationPhase::Suspended
    );
    assert_eq!(runtime.latest_status.as_deref(), Some("cancelled"));
    assert!(f
        .server
        .db
        .get_conversation(conversation(&a))
        .unwrap()
        .messages
        .is_empty());
    assert!(f.server.active_tasks.lock().is_empty());
}
