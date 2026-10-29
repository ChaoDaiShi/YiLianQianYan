use super::*;
use crate::modules::capability::import_store::{self, Package};
use crate::safety::{ControlSession, CONTROL_SESSION_HEADER};
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request},
};
use tower::ServiceExt;

async fn request(
    server: Arc<AppServer>,
    method: Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = crate::api::build_router(server)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(CONTROL_SESSION_HEADER, "a".repeat(64))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap(),
    )
}

#[tokio::test]
async fn plugin_settings_follow_enable_state_and_persist_typed_values() {
    let server = Arc::new(
        AppServer::new_with_control_session(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new("a".repeat(64)).unwrap(),
        )
        .unwrap(),
    );
    let manifest = json!({"schema_version":1,"id":"demo.settings","name":"演示插件","version":"1","contributes":{"settings":[{
    "id":"display","title":"显示偏好","description":"插件自己的设置","fields":[
        {"key":"enabled","label":"显示提示","kind":"toggle","default":true},
        {"key":"mode","label":"布局","kind":"select","default":"compact","options":["compact","normal"]}
    ]}]}});
    let mut package = Package {
        id: "demo.settings".into(),
        name: "演示插件".into(),
        version: "1".into(),
        kind: "plugin".into(),
        source: "test".into(),
        permissions: vec![],
        content_hash: String::new(),
        file_encodings: BTreeMap::new(),
        files: BTreeMap::from([("plugin.json".into(), manifest.to_string())]),
    };
    import_store::validate_package(&mut package).unwrap();
    let preview = import_store::inspect(&server.db, package, 1).unwrap();
    let installed = import_store::confirm(&server.db, &preview.preview_id, 2).unwrap();
    let path = "/api/plugins/demo.settings/settings/display";
    assert_eq!(
        request(server.clone(), Method::GET, path, Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let enabled = crate::api::capabilities::change_import_owned(
        &server,
        "demo.settings",
        installed.revision,
        "enable",
    )
    .unwrap();
    let (status, loaded) = request(server.clone(), Method::GET, path, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(loaded["values"], json!({"enabled":true,"mode":"compact"}));
    let (_, canvas) = request(
        server.clone(),
        Method::GET,
        "/api/settings/canvas",
        Value::Null,
    )
    .await;
    assert!(canvas["presets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "plugin:demo.settings:display"));
    assert_eq!(
        canvas["state"]["connected_node_ids"]
            .as_array()
            .unwrap()
            .len(),
        15
    );
    let values = json!({"values":{"enabled":false,"mode":"normal"}});
    assert_eq!(
        request(server.clone(), Method::PUT, path, values.clone())
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        request(server.clone(), Method::GET, path, Value::Null)
            .await
            .1["values"],
        values["values"]
    );
    assert_eq!(
        request(
            server.clone(),
            Method::PUT,
            path,
            json!({"values":{"enabled":"yes","mode":"normal"}})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let disabled = crate::api::capabilities::change_import_owned(
        &server,
        "demo.settings",
        enabled.revision,
        "disable",
    )
    .unwrap();
    assert!(entries(&server.db).unwrap().is_empty());
    assert_eq!(
        request(server.clone(), Method::GET, path, Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    crate::api::capabilities::change_import_owned(
        &server,
        "demo.settings",
        disabled.revision,
        "enable",
    )
    .unwrap();
    assert_eq!(
        request(server, Method::GET, path, Value::Null).await.1["values"],
        values["values"]
    );
}
