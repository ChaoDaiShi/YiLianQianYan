#[path = "support/shared_goal_activity.rs"]
mod support;
use serde_json::json;
use support::*;
use yilian_backend::{
    agent::{engine::AgentEvent, user_input::UserInputRequest},
    db::ConversationRunStatus,
    safety::approval::ApprovalStatus,
    tools::trait_def::RiskLevel,
};

#[tokio::test]
async fn goal_cancel_resolves_waiting_questions_and_approvals_without_executing_them() {
    let (server, _, graph) = setup();
    let input_conversation = bound_conversation(&server, &graph).await;
    start_turn(&server, &input_conversation);
    let input = server
        .db
        .create_conversation_input(
            &input_conversation,
            "ask",
            &UserInputRequest {
                title: "Continue?".into(),
                kind: "text".into(),
                options: vec![],
                allow_text: true,
            },
        )
        .unwrap();
    let approval_conversation = bound_conversation(&server, &graph).await;
    start_turn(&server, &approval_conversation);
    let approval = server.approval_store.create(
        approval_conversation.clone(),
        "approval-call".into(),
        "process".into(),
        json!({"command":"must not run"}),
        RiskLevel::High,
        "Explicit approval required".into(),
        "local-user".into(),
    );
    let event = AgentEvent {
        event_type: "approval_required".into(),
        conversation_id: approval_conversation.clone(),
        tool_call_id: Some(approval.tool_call_id.clone()),
        tool_name: Some(approval.tool_name.clone()),
        approval_id: Some(approval.approval_id.clone()),
        args: Some(approval.arguments.clone()),
        token: None,
        result: None,
        status: None,
        error: None,
        message_id: None,
        risk_level: None,
        reason: None,
        verification_success: None,
        verification_reason: None,
        configuration_source: None,
        should_replan: None,
    };
    server.db.apply_execution_event(&event).unwrap();
    server
        .db
        .set_conversation_run_status(
            &approval_conversation,
            ConversationRunStatus::WaitingApproval,
            None,
        )
        .unwrap();
    assert_eq!(
        call(
            &server,
            "POST",
            &format!("/api/approvals/{}/approve", approval.approval_id),
            json!({"conversation_id":"different-conversation"})
        )
        .await
        .0,
        409
    );
    assert_eq!(
        server
            .approval_store
            .get(&approval.approval_id)
            .unwrap()
            .status,
        ApprovalStatus::Pending
    );
    let unrelated = server.approval_store.create(
        "unrelated".into(),
        "other-call".into(),
        "process".into(),
        json!({}),
        RiskLevel::High,
        "Other goal".into(),
        "local-user".into(),
    );
    server.task_world.cancel_task(&graph, now()).unwrap();
    assert!(server
        .db
        .pending_conversation_inputs(&input_conversation)
        .unwrap()
        .is_empty());
    assert_eq!(
        server
            .db
            .conversation_input(&input.question_id)
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        server
            .approval_store
            .get(&approval.approval_id)
            .unwrap()
            .status,
        ApprovalStatus::Cancelled
    );
    assert_eq!(
        server
            .approval_store
            .get(&unrelated.approval_id)
            .unwrap()
            .status,
        ApprovalStatus::Pending
    );
    for conversation in [&input_conversation, &approval_conversation] {
        assert_eq!(
            server.db.get_conversation(conversation).unwrap().run_status,
            ConversationRunStatus::Cancelled
        );
    }
    assert_eq!(
        server
            .db
            .get_execution_history(&approval_conversation)
            .unwrap()[0]
            .approval_status,
        "cancelled"
    );
    assert_eq!(
        call(
            &server,
            "POST",
            &format!("/api/approvals/{}/approve", approval.approval_id),
            json!({"conversation_id":approval_conversation})
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            &server,
            "POST",
            &format!("/api/conversation-inputs/{}/answer", input.question_id),
            json!({"revision":1,"text":"continue"})
        )
        .await
        .0,
        409
    );
    server.task_world.cancel_task(&graph, now()).unwrap();
    assert_eq!(
        server
            .db
            .get_conversation(&input_conversation)
            .unwrap()
            .messages
            .iter()
            .filter(|message| message.tool_call_id.as_deref() == Some("ask"))
            .count(),
        1
    );
}

#[tokio::test]
async fn cancelled_goal_reconciles_late_waiting_outcome_and_keeps_plain_chat_semantics() {
    let (server, _, graph) = setup();
    let conversation = bound_conversation(&server, &graph).await;
    start_turn(&server, &conversation);
    server.task_world.cancel_task(&graph, now()).unwrap();
    // Simulate the producer handing off an ask_user after cancellation won
    // the graph control ordering boundary.
    server
        .db
        .create_conversation_input(
            &conversation,
            "late-ask",
            &UserInputRequest {
                title: "Late question".into(),
                kind: "text".into(),
                options: vec![],
                allow_text: true,
            },
        )
        .unwrap();
    assert_eq!(
        server
            .task_world
            .commit_conversation_outcome(&conversation, ConversationRunStatus::WaitingInput, None)
            .unwrap(),
        ConversationRunStatus::Cancelled
    );
    assert!(server
        .db
        .pending_conversation_inputs(&conversation)
        .unwrap()
        .is_empty());
    let plain = server.db.create_conversation("Ordinary chat").unwrap().id;
    start_turn(&server, &plain);
    assert_eq!(
        server
            .task_world
            .commit_conversation_outcome(&plain, ConversationRunStatus::WaitingInput, None)
            .unwrap(),
        ConversationRunStatus::WaitingInput
    );
}

#[tokio::test]
async fn reopening_a_cancelled_goal_cleans_waiters_left_between_control_commit_and_handoff() {
    let (server, _, graph) = setup();
    let conversation = bound_conversation(&server, &graph).await;
    start_turn(&server, &conversation);
    server.task_world.cancel_task(&graph, now()).unwrap();
    server
        .db
        .create_conversation_input(
            &conversation,
            "interrupted-ask",
            &UserInputRequest {
                title: "Interrupted question".into(),
                kind: "text".into(),
                options: vec![],
                allow_text: true,
            },
        )
        .unwrap();
    let _reopened =
        yilian_backend::modules::task::TaskWorldRuntime::new(&server.db, server.event_hub.clone())
            .unwrap();
    assert!(server
        .db
        .pending_conversation_inputs(&conversation)
        .unwrap()
        .is_empty());
    assert_eq!(
        server
            .db
            .get_conversation(&conversation)
            .unwrap()
            .run_status,
        ConversationRunStatus::Cancelled
    );
}

#[test]
fn explicit_approval_racing_goal_cancel_never_leaves_a_pending_request() {
    use std::sync::{Arc, Barrier};
    use yilian_backend::safety::approval::ApprovalStore;
    for _ in 0..16 {
        let store = Arc::new(ApprovalStore::new());
        let approval = store.create(
            "bound-conversation".into(),
            "call".into(),
            "process".into(),
            json!({}),
            RiskLevel::High,
            "User decision".into(),
            "local-user".into(),
        );
        let gate = Arc::new(Barrier::new(2));
        let (confirm_store, confirm_gate, approval_id) =
            (store.clone(), gate.clone(), approval.approval_id.clone());
        let confirmed = std::thread::spawn(move || {
            confirm_gate.wait();
            confirm_store.consume_for_approval(&approval_id, "bound-conversation")
        });
        gate.wait();
        let cancelled = store.cancel_pending_for_goal("graph", &["bound-conversation".into()]);
        let confirmed = confirmed.join().unwrap();
        let current = store.get(&approval.approval_id).unwrap();
        assert!(matches!(
            current.status,
            ApprovalStatus::Approved | ApprovalStatus::Cancelled
        ));
        assert_eq!(
            confirmed.is_ok(),
            current.status == ApprovalStatus::Approved
        );
        assert_eq!(
            !cancelled.is_empty(),
            current.status == ApprovalStatus::Cancelled
        );
    }
}
