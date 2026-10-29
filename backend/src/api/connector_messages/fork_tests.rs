use super::*;

#[tokio::test]
async fn explicit_branch_creation_retains_history_without_starting_model_execution() {
    let token = "f".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    let root = server.db.create_conversation("Branch API").unwrap().id;
    append(&server, &root, "fork-u", "user", "First question", 1);
    append(&server, &root, "fork-a", "assistant", "First answer", 2);
    append(&server, &root, "later-u", "user", "Later question", 3);
    append(&server, &root, "later-a", "assistant", "Later answer", 4);
    let (status, body) = send(
        server.clone(),
        &token,
        &format!("/api/conversations/{root}/branches"),
        json!({"parent_node_id":"fork-a"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let value: Value = serde_json::from_str(&body).unwrap();
    let branch = value["id"].as_str().unwrap();
    let detail = server.db.get_conversation(branch).unwrap();
    assert_eq!(detail.messages.len(), 2);
    assert_eq!(detail.messages[1].content, "First answer");
    assert_eq!(server.db.get_conversation(&root).unwrap().messages.len(), 4);
    let overview = server.db.message_canvas_overview().unwrap();
    let projected = overview
        .conversations
        .iter()
        .find(|entry| entry.id == branch)
        .unwrap();
    assert!(projected.nodes.is_empty());
    assert_eq!(projected.branch.as_ref().unwrap().parent_node_id, "fork-a");
    assert_eq!(projected.status, "idle");
    let (status, _) = send(
        server.clone(),
        &token,
        &format!("/api/conversations/{root}/branches"),
        json!({"parent_node_id":"fork-u"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(server.db.list_conversations().unwrap().len(), 2);
}
