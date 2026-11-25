use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use yilian_backend::{
    api::build_router,
    app::state::AppServer,
    safety::{ControlSession, CONTROL_SESSION_HEADER},
};

fn server() -> (Arc<AppServer>, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("yilian-rich-pages-{}", uuid::Uuid::new_v4()));
    (reopen(&root), root)
}

fn reopen(root: &std::path::Path) -> Arc<AppServer> {
    Arc::new(
        AppServer::new_with_control_session(
            &root.join("app.db"),
            &root.to_string_lossy(),
            ControlSession::new("p".repeat(64)).unwrap(),
        )
        .unwrap(),
    )
}

async fn call(
    server: &Arc<AppServer>,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = build_router(server.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(CONTROL_SESSION_HEADER, "p".repeat(64))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
    )
}

#[tokio::test]
async fn nested_pages_detect_cycles_conflicts_and_restore_history() {
    let (server, _) = server();
    let (status, workspace) = call(
        &server,
        "POST",
        "/api/workspaces",
        json!({"name":"验收空间"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    let workspace = workspace["id"].as_str().unwrap();
    let (status, root) = call(
        &server,
        "POST",
        "/api/v1/pages",
        json!({"workspace_id":workspace,"title":"根页面"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{root}");
    let root_id = root["id"].as_str().unwrap();
    let (_, child) = call(
        &server,
        "POST",
        "/api/v1/pages",
        json!({"workspace_id":workspace,"title":"子页面","parent_page_id":root_id}),
    )
    .await;
    let child_id = child["id"].as_str().unwrap();
    let path = format!("/api/v1/pages/{root_id}");
    let (status, _) = call(
        &server,
        "PATCH",
        &path,
        json!({"revision":1,"parent_page_id":child_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let block = json!({"id":"intro","type":"markdown","text":"# 标题\n\n- 段落"});
    let (status, saved) = call(
        &server,
        "POST",
        &format!("{path}/blocks/patch"),
        json!({"revision":1,"operations":[{"op":"insert","block":block}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["revision"], 2);
    let (status, _) = call(
        &server,
        "PATCH",
        &path,
        json!({"revision":1,"title":"过期"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, restored) = call(
        &server,
        "POST",
        &format!("{path}/revisions/1/restore"),
        json!({"revision":2}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["blocks"], json!([]));
    assert_eq!(restored["revision"], 3);
    let (status, _) = call(&server, "DELETE", &path, json!({"revision":3})).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let child_path = format!("/api/v1/pages/{child_id}");
    assert_eq!(
        call(&server, "GET", &child_path, Value::Null).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _) = call(
        &server,
        "POST",
        &format!("{path}/restore"),
        json!({"revision":4}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&server, "GET", &child_path, Value::Null).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn rich_actions_validate_and_restore_against_the_assistant_message() {
    let (server, root) = server();
    let conversation = server.db.create_conversation("富交互").unwrap();
    server.db.add_message(&yilian_backend::db::MessageRow {
        id:"rich-test".into(), conversation_id: conversation.id.clone(), role:"assistant".into(),
        content: r#"统计
```yilian-rich
{"schema_version":1,"blocks":[{"type":"chart","id":"sales","title":"销量","kind":"bar","labels":["一月","二月"],"values":[8,12]},{"type":"form","id":"form","title":"答复","fields":[{"name":"note","label":"备注","required":true}]}]}
```
```ag-ui
{"type":"CUSTOM","name":"yilian.rich_content","value":{"schema_version":1,"blocks":[{"type":"text","id":"event","text":"外部事件"}]}}
```
```a2ui
{"version":"1.0","createSurface":{"surfaceId":"incoming","components":[{"id":"root","component":"Text","text":"外部组件"}]}}
```
```a2ui
{"version":"unsupported","createSurface":{"surfaceId":"future","components":[{"id":"root","component":"Text","text":"未知版本"}]}}
```"#.into(),
        tool_calls:None, tool_call_id:None, tool_name:None, tool_result:None, created_at:1,
    }).unwrap();
    let path = format!(
        "/api/v1/conversations/{}/messages/rich-test/rich-content",
        conversation.id
    );
    let (status, payload) = call(&server, "GET", &path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{payload}");
    assert_eq!(payload["parts"][1]["blocks"][0]["id"], "sales");
    assert_eq!(payload["parts"][2]["blocks"][0]["text"], "外部事件");
    assert_eq!(payload["parts"][3]["blocks"][0]["text"], "外部组件");
    assert_eq!(payload["parts"][4]["type"], "markdown");
    let form = format!("{path}/form/actions");
    let (status, _) = call(
        &server,
        "POST",
        &form,
        json!({"state_version":0,"action":"execute","values":{"command":"invalid"}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &server,
        "POST",
        &form,
        json!({"state_version":0,"action":"submit","values":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &server,
        "POST",
        &form,
        json!({"state_version":0,"action":"submit","values":{"note":"已填写"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &server,
        "POST",
        &form,
        json!({"state_version":0,"action":"submit","values":{"note":"过期写入"}}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, workspace) = call(
        &server,
        "POST",
        "/api/workspaces",
        json!({"name":"聊天成果空间"}),
    )
    .await;
    let (status, document) = call(
        &server,
        "POST",
        &format!(
            "/api/v1/conversations/{}/messages/rich-test/save-to-page",
            conversation.id
        ),
        json!({"workspace_id":workspace["id"],"title":"已保存交互"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{document}");
    let document_path = format!("/api/v1/pages/{}", document["id"].as_str().unwrap());
    drop(server);
    let server = reopen(&root);
    let (_, saved) = call(&server, "GET", &path, Value::Null).await;
    assert_eq!(saved["states"]["form"]["state"]["note"], "已填写");
    let (status, doc) = call(&server, "GET", &document_path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{doc}");
    assert_eq!(doc["blocks"][0]["conversation_id"], conversation.id);
    assert_eq!(doc["blocks"][0]["message_id"], "rich-test");
    assert_eq!(
        doc["blocks"][0]["snapshot"]["states"]["form"]["state"]["note"],
        "已填写"
    );
}

#[tokio::test]
async fn markdown_round_trips_and_search_pages_without_loading_descendants() {
    let (server, _) = server();
    let (_, workspace) = call(
        &server,
        "POST",
        "/api/workspaces",
        json!({"name":"Markdown 工作空间"}),
    )
    .await;
    let workspace = workspace["id"].as_str().unwrap();
    let markdown = "# 测试标题\n\n1. 编号项目\n\n> 引用\n\n~~~rust\nfn main() {}\n~~~\n";
    let (_, first) = call(
        &server,
        "POST",
        "/api/v1/pages",
        json!({"workspace_id":workspace,"title":"第一篇","markdown":markdown}),
    )
    .await;
    let id = first["id"].as_str().unwrap();
    let (_, second) = call(
        &server,
        "POST",
        "/api/v1/pages",
        json!({"workspace_id":workspace,"title":"第二篇"}),
    )
    .await;
    let (_, child) = call(
        &server,
        "POST",
        "/api/v1/pages",
        json!({
            "workspace_id":workspace,"title":"子页面","parent_page_id":id
        }),
    )
    .await;
    let list = format!("/api/v1/pages?workspace_id={workspace}");
    let (_, roots) = call(&server, "GET", &format!("{list}&limit=1"), Value::Null).await;
    assert_eq!(roots["pages"][0]["id"], first["id"]);
    assert_eq!(roots["pages"][0]["has_children"], true);
    assert_eq!(roots["next_offset"], 1);
    let (_, remainder) = call(
        &server,
        "GET",
        &format!("{list}&limit=1&offset=1"),
        Value::Null,
    )
    .await;
    assert_eq!(remainder["pages"][0]["id"], second["id"]);
    assert_eq!(remainder["next_offset"], Value::Null);
    let (_, children) = call(
        &server,
        "GET",
        &format!("{list}&parent_page_id={id}"),
        Value::Null,
    )
    .await;
    assert_eq!(children["pages"][0]["id"], child["id"]);
    let (_, result) = call(&server, "GET", &format!("{list}&q=fn%20main"), Value::Null).await;
    assert_eq!(result["pages"][0]["id"], first["id"]);
    let (status, exported) = call(
        &server,
        "GET",
        &format!("/api/v1/pages/{id}/markdown"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exported, markdown);
    assert_eq!(
        call(
            &server,
            "DELETE",
            &format!("/api/workspaces/{workspace}"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    let page = format!("/api/v1/pages/{id}");
    assert_eq!(
        call(&server, "GET", &page, Value::Null).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &server,
            "PATCH",
            &page,
            json!({"revision":1,"title":"归档写入"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
