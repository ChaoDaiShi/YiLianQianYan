use super::*;
use crate::{
    db::model_pools::ProviderAccount,
    integrations::secret::{InMemorySecretStore, SecretStore},
    safety::ControlSession,
};
use axum::{
    body::{to_bytes, Body},
    http::Request,
    routing::get,
    Router,
};
use std::sync::Mutex;
use tower::ServiceExt;

const TOKEN: &str = "catalog-test-control-session-00000000000000000000";

#[tokio::test]
async fn catalog_reads_upstream_and_saved_credentials_without_saving_or_echoing_keys() {
    let store = Arc::new(InMemorySecretStore::new());
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new(TOKEN).unwrap(),
            store.clone(),
        )
        .unwrap(),
    );
    let calls = Arc::new(Mutex::new(Vec::new()));
    let capture = calls.clone();
    let mock = Router::new().route("/v1/models", get(move |headers: axum::http::HeaderMap| {
        let capture = capture.clone();
        async move {
            capture.lock().unwrap().push(headers.get("authorization").unwrap().to_str().unwrap().to_owned());
            Json(json!({"data":[{"id":"model-b","name":"上游模型 B","architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"max_output_tokens":8192},{"id":"model-a"},{"id":"model-a"},{"id":""}]}))
        }
    })).route("/error/models", get(|| async { (StatusCode::UNAUTHORIZED, "fixture-key must not escape") }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let call = |body: Value, auth: bool| {
        let router = crate::app::router::build_router(server.clone());
        async move {
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/llm/models/discover")
                .header("content-type", "application/json");
            if auth {
                req = req.header("X-Yilian-Control-Session", TOKEN);
            }
            let response = router
                .oneshot(req.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            (status, String::from_utf8(bytes.to_vec()).unwrap())
        }
    };
    let body =
        json!({"provider":"custom","base_url":format!("{origin}/v1"),"api_key":"fixture-key"});
    assert_eq!(call(body.clone(), false).await.0, StatusCode::UNAUTHORIZED);
    let (status, raw) = call(body.clone(), true).await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(!raw.contains("fixture-key"));
    let data: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(data["models"].as_array().unwrap().len(), 2);
    assert_eq!(data["models"][0]["id"], "model-a");
    assert_eq!(data["models"][1]["name"], "上游模型 B");
    assert_eq!(data["models"][1]["max_output_tokens"], 8192);
    assert_eq!(
        data["models"][1]["input_modalities"],
        json!(["text", "image"])
    );
    assert_eq!(data["models"][1]["output_modalities"], json!(["text"]));
    assert!(server.db.list_llm_models().unwrap().is_empty());
    store
        .put(
            &SecretRef::new("catalog.account"),
            SecretString::from("stored-key".to_string()),
        )
        .await
        .unwrap();
    server
        .db
        .save_provider_account(&ProviderAccount {
            id: "account".into(),
            provider_id: "custom".into(),
            label: "测试".into(),
            base_url: format!("{origin}/v1"),
            credential_id: Some("catalog.account".into()),
            profile_ids: vec![],
        })
        .unwrap();
    let saved = json!({"provider":"custom","base_url":format!("{origin}/v1"),"provider_account_id":"account"});
    assert_eq!(call(saved.clone(), true).await.0, StatusCode::OK);
    let mut changed = saved;
    changed["base_url"] = json!(format!("{origin}/other"));
    assert_eq!(call(changed, true).await.0, StatusCode::BAD_REQUEST);
    let (status, raw) = call(
        json!({"provider":"custom","base_url":format!("{origin}/error"),"api_key":"fixture-key"}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!raw.contains("fixture-key"));
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["Bearer fixture-key", "Bearer stored-key"]
    );
    service.abort();
}
