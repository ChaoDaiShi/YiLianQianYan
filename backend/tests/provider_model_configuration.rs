use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use yilian_backend::{
    app::router::build_router, integrations::secret::InMemorySecretStore, safety::ControlSession,
    AppServer,
};

const TOKEN: &str = "provider-catalog-configuration-fixture-0000000000000000";
const KEY: &str = "catalog-shared-fixture-key";

async fn call(app: &Router, method: &str, path: &str, body: Value) -> Value {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("X-Yilian-Control-Session", TOKEN)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let raw = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let raw = String::from_utf8(raw.to_vec()).unwrap();
    assert!(status.is_success(), "{method} {path}: {status} {raw}");
    assert!(!raw.contains(KEY));
    if raw.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&raw).unwrap()
    }
}

#[tokio::test]
async fn one_catalog_account_adds_models_and_routes_the_explicit_speech_default() {
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            Path::new(":memory:"),
            ".",
            ControlSession::new(TOKEN).unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    );
    let app = build_router(server.clone());
    let fixture = Router::new()
        .route("/v1/models", get(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], format!("Bearer {KEY}"));
            Json(json!({"data":[{"id":"fixture-multimodal","architecture":{"input_modalities":["text","image","audio"],"output_modalities":["text"]}}]}))
        }))
        .route("/v1/chat/completions", post(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], format!("Bearer {KEY}"));
            Json(json!({"id":"verify-chat","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}))
        }))
        .route("/v1/audio/transcriptions", post(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], format!("Bearer {KEY}"));
            (StatusCode::OK, Json(json!({"text":"忆涟千言"})))
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let provider = tokio::spawn(async move { axum::serve(listener, fixture).await.unwrap() });
    let original = call(&app, "GET", "/api/settings", Value::Null).await;
    let catalog = call(
        &app,
        "POST",
        "/api/llm/models/discover",
        json!({"provider":"custom","base_url":base,"api_key":KEY}),
    )
    .await;
    assert_eq!(
        catalog["models"][0]["input_modalities"],
        json!(["text", "image", "audio"])
    );
    assert_eq!(catalog["models"][0]["output_modalities"], json!(["text"]));

    let account = call(&app, "POST", "/api/provider-accounts", json!({"id":"","label":"测试账户","provider_id":"custom","base_url":base,"profile_ids":[],"credential_action":"replace","credential_fields":{"api_key":KEY}})).await;
    let account_id = account["id"].as_str().unwrap();
    let chat_payload = |model| json!({"provider_account_id":account_id,"label":model,"model":model,"provider":"custom","base_url":base});
    let chat = call(
        &app,
        "POST",
        "/api/llm/models",
        chat_payload("fixture-multimodal"),
    )
    .await;
    let chat_id = chat["id"].as_str().unwrap();
    let verified = call(
        &app,
        "POST",
        &format!("/api/llm/models/{chat_id}/verify"),
        Value::Null,
    )
    .await;
    let second = call(
        &app,
        "POST",
        "/api/llm/models",
        chat_payload("fixture-second"),
    )
    .await;
    let models = call(&app, "GET", "/api/llm/models", Value::Null).await;
    let saved = models
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["id"] == chat_id)
        .unwrap();
    assert!(verified["model"]["verified_at"].is_number());
    assert_eq!(saved["verified_at"], verified["model"]["verified_at"]);
    assert_eq!(saved["active"], false);
    assert_eq!(second["api_key_configured"], true);

    let service = |kind| json!({"label":"fixture-multimodal","model":"fixture-multimodal","provider_account_id":account_id,"service_kind":kind,"adapter":"openai"});
    call(&app, "POST", "/api/service-profiles", service("vision")).await;
    let speech = call(&app, "POST", "/api/service-profiles", service("stt")).await;
    let speech_id = speech["id"].as_str().unwrap();
    let chat_repeat = call(
        &app,
        "POST",
        "/api/llm/models",
        chat_payload("fixture-multimodal"),
    )
    .await;
    assert_eq!(chat_repeat["id"], chat_id);
    let speech_repeat = call(&app, "POST", "/api/service-profiles", service("stt")).await;
    assert_eq!(speech_repeat["id"], speech_id);
    let model_entities = call(&app, "GET", "/api/llm/provider-models", Value::Null).await;
    let rows = model_entities.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let multimodal = rows
        .iter()
        .find(|row| row["model"] == "fixture-multimodal")
        .unwrap();
    assert_eq!(multimodal["provider_account_id"], account_id);
    assert_eq!(multimodal["profiles"]["chat"], json!([chat_id]));
    assert_eq!(multimodal["profiles"]["stt"], json!([speech_id]));
    assert_eq!(
        multimodal["profiles"]["vision"].as_array().unwrap().len(),
        1
    );
    assert!(multimodal["profiles"]["image"].is_null());
    let before = call(&app, "GET", "/api/settings", Value::Null).await;
    assert_eq!(before["voice"]["stt"], original["voice"]["stt"]);
    assert!(call(&app, "GET", "/api/service-defaults/vision", Value::Null).await["id"].is_null());

    call(
        &app,
        "PUT",
        "/api/service-defaults/stt",
        json!({"source":"profile","id":speech_id}),
    )
    .await;
    assert_eq!(
        call(&app, "GET", "/api/service-defaults/stt", Value::Null).await["id"],
        speech_id
    );
    assert_eq!(
        call(&app, "POST", "/api/providers/stt/verify", Value::Null).await["status"],
        "ok"
    );
    let mut updated = call(&app, "GET", "/api/settings", Value::Null).await;
    updated["voice"]["stt"]["clear_api_key"] = json!(true);
    assert_eq!(
        call(&app, "PUT", "/api/settings", updated).await["status"],
        "saved"
    );
    assert!(call(&app, "GET", "/api/service-defaults/stt", Value::Null).await["id"].is_null());
    let shared_catalog = call(
        &app,
        "POST",
        "/api/llm/models/discover",
        json!({"provider":"custom","base_url":base,"provider_account_id":account_id}),
    )
    .await;
    assert_eq!(shared_catalog["models"][0]["id"], "fixture-multimodal");
    call(
        &app,
        "DELETE",
        &format!("/api/llm/models/{}", second["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    let accounts = call(&app, "GET", "/api/provider-accounts", Value::Null).await;
    assert_eq!(accounts[0]["profile_ids"], json!([chat_id]));
    assert_eq!(accounts[0]["credential_configured"], true);
    provider.abort();
}

#[tokio::test]
async fn model_post_redirects_preserve_origin_for_chat_embedding_and_image() {
    use yilian_backend::db::basic_services::BasicServiceProfile;
    use yilian_backend::{
        config::types::ModelConfig,
        db::{model_pools::ProviderAccount, Database},
        integrations::{
            llm::{client::LlmClient, types::ChatMessage},
            secret::{SecretRef, SecretResolver, SecretStore},
        },
        modules::settings::application::basic_services::{invoke_profile, BasicServiceRequest},
    };

    async fn listen(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (base, task)
    }

    let received = Arc::new(AtomicUsize::new(0));
    let counter = received.clone();
    let (other, other_task) = listen(Router::new().fallback(move |_: Body| {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Json(json!({"unexpected": true}))
        }
    }))
    .await;
    let allowed = Router::new()
        .route(
            "/same/chat/completions",
            post(|| async { (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, "/real-chat")]) }),
        )
        .route(
            "/real-chat",
            post(|Json(body): Json<Value>| async move {
                assert_eq!(body["messages"][0]["content"], "fixture-private-input");
                Json(json!({"id":"redirect-verified","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}))
            }),
        )
        .route(
            "/same/embeddings",
            post(|| async { (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, "/real-embedding")]) }),
        )
        .route(
            "/real-embedding",
            post(|headers: axum::http::HeaderMap, Json(body): Json<Value>| async move {
                assert_eq!(headers[header::AUTHORIZATION], format!("Bearer {KEY}"));
                assert_eq!(body["model"], "fixture-embedding");
                assert_eq!(body["input"], "fixture-private-input");
                Json(json!({"data":[{"embedding":[0.25,0.75]}]}))
            }),
        )
        .fallback(move || {
            let location = other.clone() + "/unexpected";
            async move {
                (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, location)]).into_response()
            }
        });
    let (base, allowed_task) = listen(allowed).await;
    let store = Arc::new(InMemorySecretStore::new());
    let secret = SecretRef::new("llm.redirect-fixture");
    store
        .put(&secret, secrecy::SecretString::from(KEY))
        .await
        .unwrap();
    let resolver = Arc::new(SecretResolver::new(store));
    let db = Database::new(Path::new(":memory:")).unwrap();
    db.save_provider_account(&ProviderAccount {
        id: "redirect-account".into(),
        label: "Redirect fixture".into(),
        provider_id: "custom".into(),
        base_url: base.clone(),
        credential_id: Some(secret.key.clone()),
        profile_ids: vec![],
    })
    .unwrap();
    let config = ModelConfig {
        base_url: base.clone(),
        api_key: KEY.into(),
        api_key_env: String::new(),
        embedding_model: "fixture-embedding".into(),
        embedding_base_url: base.clone(),
        embedding_api_key: KEY.into(),
        ..ModelConfig::default()
    };
    let message = [ChatMessage::user("fixture-private-input")];
    let client = LlmClient::new(&config, resolver.clone());
    let chat = client.invoke(&message, &[]).await.unwrap_err();
    let stream = client.invoke_stream(&message, &[]).await.err().unwrap();
    let embedding = client.embed("fixture-private-input").await.unwrap_err();
    let service = invoke_profile(
        &db,
        &resolver,
        &BasicServiceProfile {
            id: "redirect-image".into(),
            label: "Redirect image".into(),
            provider_account_id: "redirect-account".into(),
            service_kind: "image".into(),
            model: "fixture-image".into(),
            adapter: "openai".into(),
            parameters: json!({}),
            revision: 1,
            test: None,
        },
        BasicServiceRequest {
            prompt: "fixture-private-input".into(),
            image_url: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(received.load(Ordering::SeqCst), 0);
    for error in [
        chat.to_string(),
        stream.to_string(),
        embedding.to_string(),
        service.to_string(),
    ] {
        assert!(error.contains("重定向"), "{error}");
        assert!(!error.contains("fixture-private-input"), "{error}");
    }
    let same_origin = LlmClient::new(
        &ModelConfig {
            base_url: format!("{base}/same"),
            embedding_base_url: format!("{base}/same"),
            ..config
        },
        resolver,
    );
    assert!(same_origin.invoke(&message, &[]).await.is_ok());
    assert_eq!(
        same_origin.embed("fixture-private-input").await.unwrap(),
        vec![0.25, 0.75]
    );
    other_task.abort();
    allowed_task.abort();
}

#[test]
fn chat_model_pool_rejects_incompatible_profile_at_save_and_runtime_load() {
    use yilian_backend::{
        db::{
            model_pools::{ModelPoolInput, ModelPoolTier},
            Database, LlmModelInput,
        },
        integrations::{llm::model_pool::load_model_pool_runtime, secret::SecretResolver},
    };
    let db = Database::new(Path::new(":memory:")).unwrap();
    let mut model = LlmModelInput {
        id: "legacy-candidate".into(),
        provider: "fixture".into(),
        label: "fixture".into(),
        model: "fixture".into(),
        base_url: "http://127.0.0.1:1/v1".into(),
        api_format: "openai".into(),
        api_key_ref: String::new(),
        api_key_env: String::new(),
        temperature: 0.0,
        max_tokens: 100,
        invoke_timeout_ms: 1000,
    };
    db.create_llm_model(&model).unwrap();
    let input = ModelPoolInput {
        label: "fixture".into(),
        service_kind: "chat".into(),
        enabled: true,
        attempt_timeout_ms: 1000,
        total_timeout_ms: 3000,
        tiers: vec![ModelPoolTier {
            id: "one".into(),
            label: "one".into(),
            profile_ids: vec![model.id.clone()],
        }],
        expected_revision: None,
    };
    db.save_model_pool("configured-before-model-change", input.clone())
        .unwrap();
    model.api_format = "legacy-protocol".into();
    db.update_llm_model(&model).unwrap();

    let message = "候选模型不支持 OpenAI 兼容对话协议";
    assert_eq!(
        db.save_model_pool("reject-incompatible", input)
            .unwrap_err(),
        message
    );
    let result = load_model_pool_runtime(
        &db,
        Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new()))),
        "configured-before-model-change",
        "fixture-conversation",
        Arc::new(|_| {}),
    );
    assert_eq!(result.err().as_deref(), Some(message));
}
