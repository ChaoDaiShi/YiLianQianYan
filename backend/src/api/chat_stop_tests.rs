use super::*;
use crate::{
    browser::BrowserRuntime,
    integrations::secret::InMemorySecretStore,
    safety::{approval::ApprovalStatus, ControlSession},
    tools::trait_def::RiskLevel,
};
use axum::extract::{Path, State};
use axum::{http::StatusCode, Json};
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct Fixture {
    server: Arc<AppServer>,
    root: std::path::PathBuf,
    conversation: String,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("yilian-stop-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("bundle")).unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session_store_and_browser(
                std::path::Path::new(":memory:"),
                root.to_str().unwrap(),
                ControlSession::new("c".repeat(64)).unwrap(),
                Arc::new(InMemorySecretStore::new()),
                Arc::new(BrowserRuntime::for_local_fixture(root.join("bundle"))),
            )
            .unwrap(),
        );
        let conversation = server.db.create_conversation("stop fixture").unwrap().id;
        Self {
            server,
            root,
            conversation,
        }
    }
    fn turn(&self) -> String {
        self.server
            .db
            .accept_conversation_turn(&MessageRow {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: self.conversation.clone(),
                role: "user".into(),
                content: "local fixture".into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: chrono::Utc::now().timestamp_millis(),
            })
            .unwrap()
    }
    fn approval(&self) -> PendingApproval {
        self.server.approval_store.create(
            self.conversation.clone(),
            uuid::Uuid::new_v4().to_string(),
            "visible_browser_fill".into(),
            json!({"text":"must never execute"}),
            RiskLevel::High,
            "fixture".into(),
            "local_user".into(),
        )
    }
    async fn late_approve(&self, a: &PendingApproval) {
        let result = crate::api::approvals::approve_handler(
            State(self.server.clone()),
            Path(a.approval_id.clone()),
            Json(crate::api::approvals::ApprovalDecisionRequest {
                conversation_id: Some(self.conversation.clone()),
                attestation_id: None,
            }),
        )
        .await;
        match result {
            Err((status, _)) => assert_eq!(status, StatusCode::CONFLICT),
            Ok(_) => panic!("stopped approval resumed"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn active_stop_revokes_pending_approval_without_early_return_and_is_idempotent() {
    let f = Fixture::new();
    f.turn();
    let a = f.approval();
    let token = CancellationToken::new();
    f.server
        .active_tasks
        .lock()
        .insert(f.conversation.clone(), token.clone());
    let guard = crate::api::chat::lifecycle::acquire_turn(&f.server, &f.conversation).await;
    assert!(stop(f.server.clone(), &f.conversation).await.unwrap());
    assert!(token.is_cancelled());
    assert_eq!(
        f.server.approval_store.get(&a.approval_id).unwrap().status,
        ApprovalStatus::Cancelled
    );
    assert!(cancelled(&f.server.db, &f.conversation).unwrap());
    drop(guard);
    f.late_approve(&a).await;
    let _ = stop(f.server.clone(), &f.conversation).await.unwrap();
    let count: i64 = f
        .server
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM messages WHERE role='tool' AND tool_call_id=?1",
            [&a.tool_call_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
#[tokio::test]
async fn paused_stop_only_revokes_conversation_owner_not_task_or_workflow() {
    let f = Fixture::new();
    f.turn();
    let a = f.approval();
    f.server
        .db
        .set_conversation_run_status(
            &f.conversation,
            crate::db::ConversationRunStatus::WaitingApproval,
            None,
        )
        .unwrap();
    let task = f.server.approval_store.create_task_agent(
        "task".into(),
        f.conversation.clone(),
        "agent".into(),
        "task-call".into(),
        "read_file".into(),
        json!({}),
        RiskLevel::High,
        "fixture".into(),
        "local_user".into(),
    );
    let workflow = f.server.approval_store.create_workflow(
        f.conversation.clone(),
        "workflow".into(),
        "node".into(),
        "workflow-call".into(),
        "read_file".into(),
        json!({}),
        RiskLevel::High,
        "fixture".into(),
        "local_user".into(),
    );
    assert!(stop(f.server.clone(), &f.conversation).await.unwrap());
    assert_eq!(
        f.server.approval_store.get(&a.approval_id).unwrap().status,
        ApprovalStatus::Cancelled
    );
    for other in [task, workflow] {
        assert_eq!(
            f.server
                .approval_store
                .get(&other.approval_id)
                .unwrap()
                .status,
            ApprovalStatus::Pending
        );
    }
    f.late_approve(&a).await;
}
#[tokio::test]
async fn stop_during_unregistered_continuation_gate_revokes_late_approval() {
    let f = Fixture::new();
    f.turn();
    let guard = crate::api::chat::lifecycle::acquire_turn(&f.server, &f.conversation).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        stop(f.server.clone(), &f.conversation),
    )
    .await
    .unwrap()
    .unwrap();
    let late = f.approval();
    drop(guard);
    // Whether cleanup or approve acquires the original gate first, no resumed tool.
    f.late_approve(&late).await;
    assert_eq!(
        f.server
            .approval_store
            .get(&late.approval_id)
            .unwrap()
            .status,
        ApprovalStatus::Cancelled
    );
}
#[tokio::test]
async fn preparation_stop_does_not_accept_or_cancel_a_completed_turn() {
    let f = Fixture::new();
    let id = f.turn();
    f.server
        .db
        .set_conversation_run_status(
            &f.conversation,
            crate::db::ConversationRunStatus::Completed,
            None,
        )
        .unwrap();
    let token = CancellationToken::new();
    let guard =
        crate::api::chat::lifecycle::acquire_registered_turn(&f.server, &f.conversation, &token)
            .await;
    assert!(stop(f.server.clone(), &f.conversation).await.unwrap());
    assert!(token.is_cancelled());
    assert_eq!(
        f.server.db.conversation_turns(&f.conversation).unwrap()[0].id,
        id
    );
    assert_eq!(
        f.server
            .db
            .get_conversation(&f.conversation)
            .unwrap()
            .run_status,
        crate::db::ConversationRunStatus::Completed
    );
    drop(guard);
}
#[tokio::test]
async fn captured_turn_cas_cannot_cancel_a_new_turn() {
    let f = Fixture::new();
    let old = f.turn();
    f.server
        .db
        .set_conversation_run_status(
            &f.conversation,
            crate::db::ConversationRunStatus::Completed,
            None,
        )
        .unwrap();
    let new = f.turn();
    assert!(!cancel_turn(&f.server.db, &f.conversation, &old).unwrap());
    let turns = f.server.db.conversation_turns(&f.conversation).unwrap();
    assert_eq!(turns[0].id, new);
    assert_eq!(turns[0].status, "running");
}
#[tokio::test]
async fn persistence_failure_never_restores_consumable_approval() {
    let f = Fixture::new();
    f.turn();
    let a = f.approval();
    f.server.db.conn().execute_batch("CREATE TRIGGER stop_message_failure BEFORE INSERT ON messages WHEN NEW.role='tool' BEGIN SELECT RAISE(FAIL,'injected stop persistence failure'); END;").unwrap();
    let error = stop(f.server.clone(), &f.conversation).await.unwrap_err();
    assert!(error.contains("injected stop persistence failure"));
    assert_eq!(
        f.server.approval_store.get(&a.approval_id).unwrap().status,
        ApprovalStatus::Cancelled
    );
    assert!(f
        .server
        .approval_store
        .consume_for_approval(&a.approval_id, &f.conversation)
        .is_err());
    assert!(cancelled(&f.server.db, &f.conversation).unwrap());
}

#[path = "chat_stop_resume_tests.rs"]
mod resume;

#[tokio::test]
async fn agent_cancel_does_not_change_already_approved_or_rejected_turn() {
    for approve in [true, false] {
        let f = Fixture::new();
        f.turn();
        let a = f.approval();
        if approve {
            f.server
                .approval_store
                .consume_for_approval(&a.approval_id, &f.conversation)
                .unwrap();
        } else {
            f.server
                .approval_store
                .consume_for_rejection(&a.approval_id, &f.conversation)
                .unwrap();
        }
        let Json(result) = crate::api::approvals::cancel_handler(
            State(f.server.clone()),
            Path(a.approval_id.clone()),
            Json(crate::api::approvals::ApprovalDecisionRequest {
                conversation_id: Some(f.conversation.clone()),
                attestation_id: None,
            }),
        )
        .await;
        assert_eq!(result["ok"], false);
        assert_eq!(
            f.server
                .db
                .get_conversation(&f.conversation)
                .unwrap()
                .run_status,
            crate::db::ConversationRunStatus::Running
        );
        assert_eq!(
            f.server.approval_store.get(&a.approval_id).unwrap().status,
            if approve {
                ApprovalStatus::Approved
            } else {
                ApprovalStatus::Rejected
            }
        );
    }
}
#[tokio::test]
async fn repeated_agent_cancel_is_idempotent_and_does_not_cancel_new_turn() {
    let f = Fixture::new();
    f.turn();
    let a = f.approval();
    for first in [true, false] {
        if !first {
            f.turn();
        }
        let Json(result) = crate::api::approvals::cancel_handler(
            State(f.server.clone()),
            Path(a.approval_id.clone()),
            Json(crate::api::approvals::ApprovalDecisionRequest {
                conversation_id: Some(f.conversation.clone()),
                attestation_id: None,
            }),
        )
        .await;
        assert_eq!(result["ok"], true);
        assert_eq!(
            f.server
                .db
                .get_conversation(&f.conversation)
                .unwrap()
                .run_status,
            if first {
                crate::db::ConversationRunStatus::Cancelled
            } else {
                crate::db::ConversationRunStatus::Running
            }
        );
    }
    let count: i64 = f
        .server
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM messages WHERE role='tool' AND tool_call_id=?1",
            [&a.tool_call_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn turn_lookup_failure_still_cancels_active_token_and_pending_authority() {
    let f = Fixture::new();
    f.turn();
    let a = f.approval();
    let token = CancellationToken::new();
    f.server
        .active_tasks
        .lock()
        .insert(f.conversation.clone(), token.clone());
    f.server
        .db
        .conn()
        .execute_batch("ALTER TABLE conversation_turns RENAME TO unavailable_conversation_turns;")
        .unwrap();
    let error = stop(f.server.clone(), &f.conversation).await.unwrap_err();
    assert!(error.contains("conversation_turns"));
    assert!(token.is_cancelled());
    assert_eq!(
        f.server.approval_store.get(&a.approval_id).unwrap().status,
        ApprovalStatus::Cancelled
    );
    f.late_approve(&a).await;
}

#[tokio::test]
async fn not_executed_receipt_is_idempotent_and_never_overwrites_unknown() {
    let f = Fixture::new();
    f.turn();
    let a = f.approval();
    record_not_executed(&f.server.db, &a).unwrap();
    record_not_executed(&f.server.db, &a).unwrap();
    let count:i64=f.server.db.conn().query_row("SELECT count(*) FROM messages WHERE role='tool' AND conversation_id=?1 AND tool_call_id=?2",params![f.conversation,a.tool_call_id],|r|r.get(0)).unwrap();
    assert_eq!(count, 1);
    let unknown = f.approval();
    let content =
        json!({"state":"result_unknown","reason":"action may already have affected page"})
            .to_string();
    f.server
        .db
        .add_message(&MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: f.conversation.clone(),
            role: "tool".into(),
            content: content.clone(),
            tool_calls: None,
            tool_call_id: Some(unknown.tool_call_id.clone()),
            tool_name: Some(unknown.tool_name.clone()),
            tool_result: Some(content.clone()),
            created_at: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();
    record_not_executed(&f.server.db, &unknown).unwrap();
    let saved:String=f.server.db.conn().query_row("SELECT content FROM messages WHERE role='tool' AND conversation_id=?1 AND tool_call_id=?2",params![f.conversation,unknown.tool_call_id],|r|r.get(0)).unwrap();
    assert_eq!(
        saved, content,
        "unknown result is immutable; cancellation cannot claim it did not run"
    );
}
