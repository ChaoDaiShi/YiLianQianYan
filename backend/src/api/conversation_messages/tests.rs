use super::*;
use crate::{
    db::{ConversationRunStatus, MessageRow, ResourceTarget},
    integrations::secret::InMemorySecretStore,
    safety::ControlSession,
};
use axum::{body::Body, http::Request};
use tower::ServiceExt;

fn server() -> Arc<AppServer> {
    let root = std::env::temp_dir().join(format!("message-edit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    Arc::new(
        AppServer::new_with_control_session_and_store(
            &root.join("app.db"),
            root.to_str().unwrap(),
            ControlSession::generate(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    )
}
fn user(id: &str, conversation: &str, time: i64) -> MessageRow {
    MessageRow {
        id: id.into(),
        conversation_id: conversation.into(),
        role: "user".into(),
        content: "fixture question".into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: time,
    }
}

#[tokio::test]
async fn corrupt_history_is_rejected_before_a_new_user_row_or_model_request() {
    let server = server();
    let conversation = server.db.create_conversation("fixture").unwrap().id;
    server
        .db
        .accept_conversation_turn(&user("u1", &conversation, 1))
        .unwrap();
    let mut call = user("pending-call", &conversation, 2);
    call.role = "assistant".into();
    call.content.clear();
    call.tool_calls = Some(
        json!([{"id":"call","type":"function","function":{"name":"read_file","arguments":"{}"}}])
            .to_string(),
    );
    server.db.add_message(&call).unwrap();
    server
        .db
        .set_conversation_run_status(
            &conversation,
            ConversationRunStatus::Failed,
            Some("fixture failure"),
        )
        .unwrap();
    let response = crate::api::chat::chat_handler(
        State(server.clone()),
        Json(crate::api::chat::ChatRequest {
            options: Default::default(),
            conversation_id: Some(conversation.clone()),
            message: "next attempt".into(),
            resource_ids: Vec::new(),
            workflow_id: None,
            goal_ref: None,
        }),
    )
    .await;
    let (status, body) = match response {
        Err(error) => error,
        Ok(_) => panic!("corrupt history should be rejected before model setup"),
    };
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body.0["affected_user_message_id"], "u1");
    assert_eq!(
        server
            .db
            .get_conversation(&conversation)
            .unwrap()
            .messages
            .len(),
        2
    );
}

#[tokio::test]
async fn rollback_cancels_old_approval_restores_draft_and_preserves_real_resources() {
    let server = server();
    let conversation = server.db.create_conversation("fixture").unwrap().id;
    server
        .db
        .accept_conversation_turn(&user("u1", &conversation, 1))
        .unwrap();
    let mut call = user("pending-call", &conversation, 2);
    call.role = "assistant".into();
    call.content.clear();
    call.tool_calls = Some(
        json!([{"id":"call","type":"function","function":{"name":"read_file","arguments":"{}"}}])
            .to_string(),
    );
    server.db.add_message(&call).unwrap();
    server
        .db
        .set_conversation_run_status(&conversation, ConversationRunStatus::WaitingApproval, None)
        .unwrap();
    let (approval, _) = server.approval_store.create_or_get_pending(
        conversation.clone(),
        "call".into(),
        "read_file".into(),
        json!({}),
        crate::tools::RiskLevel::High,
        "fixture approval".into(),
        "local-user".into(),
    );
    let resource = server
        .resource_service
        .ingest(
            "fixture.txt",
            "text/plain",
            b"preserved attachment",
            json!({}),
        )
        .unwrap();
    server
        .db
        .bind_resources(
            &ResourceTarget::Conversation {
                conversation_id: conversation.clone(),
            },
            &[resource.id.clone()],
        )
        .unwrap();
    server
        .db
        .conn()
        .execute(
            "INSERT INTO message_attachments(message_id,resource_id,position) VALUES('u1',?1,0)",
            [&resource.id],
        )
        .unwrap();
    let response = router()
        .with_state(server.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/conversations/{conversation}/messages/u1/rollback"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["draft"]["text"], "fixture question");
    assert_eq!(body["draft"]["resource_scope"], "conversation");
    assert_eq!(body["draft"]["resources"][0]["id"], resource.id);
    assert_eq!(
        server.resource_service.read_bytes(&resource.id).unwrap(),
        b"preserved attachment"
    );
    assert_eq!(
        server
            .db
            .resource_bindings(&ResourceTarget::Conversation {
                conversation_id: conversation
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        server
            .approval_store
            .get(&approval.approval_id)
            .unwrap()
            .status
            .to_string(),
        "cancelled"
    );
}
#[tokio::test]
async fn rollback_attachment_only_user_restores_the_original_resource() {
    let server = server();
    let conversation = server.db.create_conversation("attachment only").unwrap().id;
    let mut message = user("attachment-user", &conversation, 1);
    message.content.clear();
    server.db.accept_conversation_turn(&message).unwrap();
    let resource = server
        .resource_service
        .ingest("fixture.txt", "text/plain", b"fixture bytes", json!({}))
        .unwrap();
    server
        .db
        .bind_resources(
            &ResourceTarget::Conversation {
                conversation_id: conversation.clone(),
            },
            &[resource.id.clone()],
        )
        .unwrap();
    server.db.conn().execute("INSERT INTO message_attachments(message_id,resource_id,position) VALUES('attachment-user',?1,0)", [&resource.id]).unwrap();
    server
        .db
        .set_conversation_run_status(&conversation, ConversationRunStatus::Failed, Some("failed"))
        .unwrap();
    let response = router()
        .with_state(server.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/conversations/{conversation}/messages/attachment-user/rollback"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["draft"]["text"], "");
    assert_eq!(body["draft"]["resources"][0]["id"], resource.id);
    assert_eq!(
        server.resource_service.read_bytes(&resource.id).unwrap(),
        b"fixture bytes"
    );
}
