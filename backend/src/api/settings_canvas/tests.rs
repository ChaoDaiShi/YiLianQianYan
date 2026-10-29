use super::*;
use crate::safety::{ControlSession, CONTROL_SESSION_HEADER};
use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request},
};
use tower::ServiceExt;

async fn send(
    server: Arc<AppServer>,
    token: &str,
    method: Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = crate::api::build_router(server)
        .oneshot(
            Request::builder()
                .method(method)
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
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn settings_canvas_routes_persist_ui_only_and_reject_unknown_scopes() {
    let token = "c".repeat(64);
    let server = Arc::new(
        AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(token.clone()).unwrap(),
        )
        .unwrap(),
    );
    server.db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('settings-agent','Settings agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    let canvas = server
        .db
        .create_connector_canvas("Settings", "settings-agent")
        .unwrap();
    let path = format!("/api/settings/canvas?canvas_id={canvas}");
    let before = serde_json::to_value(&*server.config.read()).unwrap();
    let (status, initial) = send(server.clone(), &token, Method::GET, &path, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial["presets"].as_array().unwrap().len(), 15);
    assert_eq!(
        initial["state"]["connected_node_ids"],
        serde_json::to_value(SettingsCanvasState::default().connected_node_ids).unwrap()
    );
    let state = json!({
        "connected_node_ids": [],
        "expanded_group_ids": ["models"],
        "nodes": [{"id":"group:models","x":-12.0,"y":42.0},{"id":"model","x":280.0,"y":42.0}],
        "viewport": {"x":22.0,"y":18.0,"zoom":1.5}
    });
    let (status, saved) = send(server.clone(), &token, Method::PUT, &path, state.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["state"], state);
    let (status, reloaded) = send(server.clone(), &token, Method::GET, &path, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reloaded, saved);
    let (_, global) = send(
        server.clone(),
        &token,
        Method::GET,
        "/api/settings/canvas",
        Value::Null,
    )
    .await;
    assert_eq!(global["state"], initial["state"]);
    assert_eq!(initial["state"]["expanded_group_ids"], json!([]));
    assert_eq!(
        serde_json::to_value(&*server.config.read()).unwrap(),
        before
    );
    assert!(server
        .db
        .resolve_connector_bindings("settings-agent")
        .unwrap()
        .is_empty());

    let mut invalid = state.clone();
    invalid["connected_node_ids"] = json!(["model", "model"]);
    assert_eq!(
        send(server.clone(), &token, Method::PUT, &path, invalid)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut invalid = state.clone();
    invalid["expanded_group_ids"] = json!(["models", "unknown"]);
    assert_eq!(
        send(server.clone(), &token, Method::PUT, &path, invalid)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let missing = "/api/settings/canvas?canvas_id=missing";
    assert_eq!(
        send(server.clone(), &token, Method::GET, missing, Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(server.clone(), &token, Method::PUT, missing, state)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    server.db.delete_connector_canvas(&canvas).unwrap();
    assert_eq!(
        send(server, &token, Method::GET, &path, Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
