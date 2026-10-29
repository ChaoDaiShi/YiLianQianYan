use super::*;
#[path = "fork_tests.rs"]
mod fork_tests;
use crate::{
    db::MessageRow,
    safety::{ControlSession, CONTROL_SESSION_HEADER},
};
use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Request},
    routing::post,
};
use tokio::{net::TcpListener, sync::Mutex};
use tower::ServiceExt;

type Requests = Arc<Mutex<Vec<Value>>>;

async fn provider(State(requests): State<Requests>, Json(body): Json<Value>) -> impl IntoResponse {
    requests.lock().await.push(body);
    ([(CONTENT_TYPE,"text/event-stream")],concat!(
        "data: {\"id\":\"branch\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"provider answer\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    ))
}

fn append(server: &AppServer, conversation: &str, id: &str, role: &str, content: &str, time: i64) {
    server
        .db
        .add_message(&MessageRow {
            id: id.into(),
            conversation_id: conversation.into(),
            role: role.into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: time,
        })
        .unwrap();
}

async fn send(
    server: Arc<AppServer>,
    token: &str,
    path: &str,
    body: Value,
) -> (StatusCode, String) {
    let response = crate::api::build_router(server)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(CONTROL_SESSION_HEADER, token)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn overview_branches_execute_without_canvas_or_agent_binding() {
    let captured = Requests::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let provider_app = Router::new()
        .route("/chat/completions", post(provider))
        .with_state(captured.clone());
    let mock = tokio::spawn(async move {
        axum::serve(listener, provider_app).await.unwrap();
    });
    let token = "d".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    {
        let mut config = server.config.write();
        config.model.base_url = url;
        config.model.api_key = "controlled-test-key".into();
        config.model.api_key_env.clear();
        config.model.invoke_timeout_ms = 5000;
    }
    let source = server
        .db
        .create_conversation("Homepage history")
        .unwrap()
        .id;
    append(
        &server,
        &source,
        "overview-api-user",
        "user",
        "SOURCE ANCESTOR QUESTION",
        1,
    );
    append(
        &server,
        &source,
        "overview-api-answer",
        "assistant",
        "SOURCE ANCESTOR ANSWER",
        2,
    );
    append(
        &server,
        &source,
        "overview-api-sibling",
        "user",
        "LATER SIBLING EXCLUDED",
        3,
    );
    let (status, stream) = send(server.clone(), &token, "/api/message-canvas/branches", json!({"conversation_id":source,"parent_node_id":"overview-api-answer","message":"NEW FORK QUESTION"})).await;
    assert_eq!(status, StatusCode::OK, "{stream}");
    assert!(stream.contains("event: connected"));
    assert!(stream.contains("provider answer"));
    let overview = server.db.message_canvas_overview().unwrap();
    assert_eq!(overview.conversations.len(), 2);
    let fork = overview
        .conversations
        .iter()
        .find(|conversation| conversation.id != source)
        .unwrap();
    assert_eq!(fork.nodes.len(), 2);
    assert_eq!(
        fork.nodes[0].parent_id.as_deref(),
        Some("overview-api-answer")
    );
    assert!(server
        .db
        .connector_agent_for_conversation(&source)
        .unwrap()
        .is_none());
    assert!(server
        .db
        .connector_agent_for_conversation(&fork.id)
        .unwrap()
        .is_none());
    let requests = captured.lock().await;
    assert_eq!(requests.len(), 1);
    let sent = requests[0]["messages"].to_string();
    assert!(sent.contains("SOURCE ANCESTOR QUESTION"));
    assert!(sent.contains("SOURCE ANCESTOR ANSWER"));
    assert!(sent.contains("NEW FORK QUESTION"));
    assert!(!sent.contains("LATER SIBLING EXCLUDED"));
    assert_eq!(
        server.db.get_conversation(&source).unwrap().messages.len(),
        3
    );
    mock.abort();
}

#[tokio::test]
async fn connector_message_turns_use_actual_chat_sse_with_ancestor_only_context() {
    let captured = Requests::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let provider_app = Router::new()
        .route("/chat/completions", post(provider))
        .with_state(captured.clone());
    let mock = tokio::spawn(async move {
        axum::serve(listener, provider_app).await.unwrap();
    });
    let token = "b".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    {
        let mut config = server.config.write();
        config.model.base_url = url;
        config.model.api_key = "controlled-test-key".into();
        config.model.api_key_env.clear();
        config.model.invoke_timeout_ms = 5000;
    }
    server.db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('message-agent','Message agent','','BRANCH SCOPED INSTRUCTION','[]','[]',10,1,'database',1,1)",[]).unwrap();
    let canvas = server
        .db
        .create_connector_canvas("Canvas", "message-agent")
        .unwrap();
    let root = server.db.create_conversation("Root").unwrap().id;
    append(
        &server,
        &root,
        "initial-user",
        "user",
        "ANCESTOR QUESTION",
        1,
    );
    append(
        &server,
        &root,
        "initial-answer",
        "assistant",
        "ANCESTOR ANSWER",
        2,
    );
    append(
        &server,
        &root,
        "source-next-user",
        "user",
        "SOURCE SIBLING MUST BE ABSENT",
        3,
    );
    append(
        &server,
        &root,
        "source-next-answer",
        "assistant",
        "SOURCE SIBLING ANSWER MUST BE ABSENT",
        4,
    );
    let graph_path = format!("/api/connectors/{canvas}/messages/{root}");
    let (status, _) = send(server.clone(), &token, &graph_path, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let turn_path = format!("{graph_path}/turns");
    for question in ["FIRST BRANCH QUESTION", "SECOND BRANCH QUESTION"] {
        let (status, stream) = send(
            server.clone(),
            &token,
            &turn_path,
            json!({"parent_node_id":"initial-answer","message":question}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{stream}");
        assert!(stream.contains("event: connected"));
        assert!(stream.contains("provider answer"));
    }
    let graph = server.db.connector_message_graph(&canvas, &root).unwrap();
    let first_answer = graph
        .nodes
        .iter()
        .find(|node| node.role == "assistant" && node.conversation_id != root)
        .unwrap();
    let (status, stream) = send(
        server.clone(),
        &token,
        &turn_path,
        json!({"parent_node_id":first_answer.id,"message":"NESTED QUESTION"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stream}");
    let requests = captured.lock().await;
    assert_eq!(requests.len(), 3);
    for (index, request) in requests.iter().enumerate() {
        let serialized = request["messages"].to_string();
        assert!(serialized.contains("BRANCH SCOPED INSTRUCTION"));
        assert!(serialized.contains("ANCESTOR QUESTION"));
        assert!(serialized.contains("ANCESTOR ANSWER"));
        assert!(!serialized.contains("SOURCE SIBLING"));
        if index == 0 {
            assert!(!serialized.contains("SECOND BRANCH QUESTION"));
        }
        if index == 1 {
            assert!(!serialized.contains("FIRST BRANCH QUESTION"));
        }
        if index == 2 {
            assert!(serialized.contains("FIRST BRANCH QUESTION"));
            assert!(!serialized.contains("SECOND BRANCH QUESTION"));
        }
    }
    assert_eq!(server.db.get_conversation(&root).unwrap().messages.len(), 4);
    assert_eq!(graph.nodes.len(), 8);
    assert!(graph
        .runs
        .iter()
        .skip(1)
        .all(|run| run.status == "completed"));
    mock.abort();
}

#[tokio::test]
async fn connector_message_stop_retains_user_and_persists_cancelled_after_stream_closes() {
    async fn stalled_provider() -> StatusCode {
        std::future::pending().await
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route("/chat/completions", post(stalled_provider));
    let mock = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let token = "c".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    {
        let mut config = server.config.write();
        config.model.base_url = url;
        config.model.api_key = "controlled-test-key".into();
        config.model.api_key_env.clear();
        config.model.invoke_timeout_ms = 5000;
    }
    server.db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('stop-agent','Agent','','','[]','[]',10,1,'database',1,1)",[]).unwrap();
    let canvas = server
        .db
        .create_connector_canvas("Stop", "stop-agent")
        .unwrap();
    let root = server.db.create_conversation("Empty").unwrap().id;
    server
        .db
        .create_connector_message_graph(&canvas, &root)
        .unwrap();
    let response = crate::api::build_router(server.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/connectors/{canvas}/messages/{root}/turns"))
                .header(CONTROL_SESSION_HEADER, &token)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"parent_node_id":null,"message":"accepted before stop"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let graph = server.db.connector_message_graph(&canvas, &root).unwrap();
    let fork = graph
        .runs
        .iter()
        .find(|run| run.conversation_id != root)
        .unwrap()
        .conversation_id
        .clone();
    let (status, stopped) = send(
        server.clone(),
        &token,
        "/api/chat/stop",
        json!({"conversation_id":fork}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(stopped.contains("cancelled"));
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        to_bytes(response.into_body(), 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    let graph = server.db.connector_message_graph(&canvas, &root).unwrap();
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].content, "accepted before stop");
    assert_eq!(
        graph
            .runs
            .iter()
            .find(|run| run.conversation_id == fork)
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(server.db.get_conversation(&root).unwrap().messages.len(), 0);
    mock.abort();
}
