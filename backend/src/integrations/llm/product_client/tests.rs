use super::*;
use crate::{
    db::{
        model_pools::{ModelPoolInput, ModelPoolTier},
        LlmModelInput,
    },
    integrations::{
        llm::types::ChatMessage,
        secret::{InMemorySecretStore, SecretRef, SecretStore},
    },
};

fn profile(db: &Database, base: &str) {
    db.create_llm_model(&LlmModelInput {
        id: "pool-profile".into(),
        provider: "openai".into(),
        label: "fixture".into(),
        model: "pool-only-model".into(),
        base_url: base.into(),
        api_format: "openai".into(),
        api_key_ref: "product.fixture".into(),
        api_key_env: String::new(),
        temperature: 0.0,
        max_tokens: 200,
        invoke_timeout_ms: 5000,
    })
    .unwrap();
}

fn pool_input() -> ModelPoolInput {
    ModelPoolInput {
        label: "default pool".into(),
        service_kind: "chat".into(),
        enabled: true,
        attempt_timeout_ms: 5000,
        total_timeout_ms: 5000,
        tiers: vec![ModelPoolTier {
            id: "primary".into(),
            label: "primary".into(),
            profile_ids: vec!["pool-profile".into()],
        }],
        expected_revision: None,
    }
}

#[tokio::test]
async fn both_product_clients_use_the_default_pool_without_an_active_model_and_record_usage_once() {
    use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
    let requests = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    async fn completion(
        State(requests): State<Arc<std::sync::Mutex<Vec<String>>>>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        assert_eq!(headers["authorization"], "Bearer local-product-fixture");
        requests
            .lock()
            .unwrap()
            .push(body["model"].as_str().unwrap().into());
        Json(
            serde_json::json!({"id":"fixture","choices":[{"index":0,"message":{"role":"assistant","content":"pool response"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}),
        )
    }
    let app = Router::new()
        .route("/chat/completions", post(completion))
        .with_state(Arc::clone(&requests));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    profile(&db, &base);
    db.save_model_pool("default-pool", pool_input()).unwrap();
    db.set_model_pool_binding("default", Some("default-pool"))
        .unwrap();
    assert!(db.get_active_llm_model().unwrap().is_none());
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    store
        .put(
            &SecretRef::new("product.fixture"),
            secrecy::SecretString::from("local-product-fixture".to_string()),
        )
        .await
        .unwrap();
    let resolver = Arc::new(SecretResolver::new(store));
    let legacy = ModelConfig {
        base_url: "http://127.0.0.1:1".into(),
        name: "must-not-be-used".into(),
        api_key_env: String::new(),
        ..Default::default()
    };
    for source in ["knowledge_build", "learning"] {
        let client = configured_product_client(
            &db,
            &legacy,
            Arc::clone(&resolver),
            source,
            "fixture-operation",
        )
        .unwrap();
        let response = client
            .invoke(&[ChatMessage::user("local fixture")], &[])
            .await
            .unwrap();
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("pool response")
        );
    }
    assert_eq!(
        *requests.lock().unwrap(),
        vec!["pool-only-model", "pool-only-model"]
    );
    assert_eq!(db.list_pool_attempts(None).unwrap().len(), 2);
    let usage = db
        .aggregate_llm_usage(
            Some("pool-profile"),
            0,
            chrono::Utc::now().timestamp_millis() + 1,
        )
        .unwrap();
    assert_eq!(usage.total_tokens, 14);
    server.abort();
}

#[test]
fn invalid_default_binding_is_an_error_even_when_an_active_model_exists() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    profile(&db, "http://127.0.0.1:1");
    db.activate_llm_model("pool-profile").unwrap();
    db.save_model_pool("default-pool", pool_input()).unwrap();
    db.set_model_pool_binding("default", Some("default-pool"))
        .unwrap();
    let resolver = Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new())));
    let mut disabled = pool_input();
    disabled.enabled = false;
    disabled.expected_revision = Some(1);
    db.save_model_pool("default-pool", disabled).unwrap();
    assert!(configured_product_client(
        &db,
        &ModelConfig::default(),
        Arc::clone(&resolver),
        "learning",
        "fixture"
    )
    .is_err());
    db.conn().execute("UPDATE model_pools SET payload=json_set(payload,'$.enabled',json('true'),'$.tiers[0].profile_ids[0]','missing-profile') WHERE id='default-pool'", []).unwrap();
    assert!(configured_product_client(
        &db,
        &ModelConfig::default(),
        resolver,
        "knowledge_build",
        "fixture"
    )
    .is_err());
}
