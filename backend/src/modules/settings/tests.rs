//! Settings module tests — moved from `api/settings.rs`.
//!
//! Handlers and application functions keep their original names so the
//! assertions are unchanged from the pre-refactor file.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use secrecy::SecretString;
use std::sync::Arc;

use crate::modules::voice::VoiceProviderError;
use crate::safety::ControlSession;
use crate::secret::{SecretRef, CHAT_KEY_REF, STT_KEY_REF};
use crate::server::AppServer;

use super::api::routes::{update_handler, verify_provider_handler};
use super::application::provider_test::normalize_voice_connection_error;
use super::application::service::build_redacted_config;

use crate::secret::{InMemorySecretStore, SecretStore, SecretStoreError, SecretStoreStatus};

struct DeleteFailingStore {
    inner: InMemorySecretStore,
}

impl DeleteFailingStore {
    fn new() -> Self {
        Self {
            inner: InMemorySecretStore::new(),
        }
    }
}

#[async_trait::async_trait]
impl SecretStore for DeleteFailingStore {
    async fn put(
        &self,
        secret_ref: &SecretRef,
        value: SecretString,
    ) -> Result<(), SecretStoreError> {
        self.inner.put(secret_ref, value).await
    }

    async fn get(&self, secret_ref: &SecretRef) -> Result<Option<SecretString>, SecretStoreError> {
        self.inner.get(secret_ref).await
    }

    async fn delete(&self, _secret_ref: &SecretRef) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Backend)
    }

    async fn status(&self) -> SecretStoreStatus {
        SecretStoreStatus::Available
    }
}

#[tokio::test]
async fn settings_get_never_returns_secret_value() {
    let path = std::env::temp_dir().join(format!(
        "yilian-settings-redact-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = AppServer::new_with_control_session_and_store(
        &path,
        ".",
        ControlSession::generate(),
        Arc::clone(&store),
    )
    .unwrap();
    store
        .put(
            &SecretRef::new(CHAT_KEY_REF),
            SecretString::from("SUPER_SECRET".to_string()),
        )
        .await
        .unwrap();
    {
        let mut cfg = server.config.write();
        cfg.model.api_key_ref = Some(SecretRef::new(CHAT_KEY_REF));
    }

    let json = build_redacted_config(&server).await;
    let text = json.to_string();
    assert!(!text.contains("SUPER_SECRET"));
    assert_eq!(json["model"]["api_key"], "");
    assert_eq!(json["model"]["api_key_configured"], true);
    assert_eq!(json["model"]["api_key_source"], "secret_store");

    drop(server);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn legacy_flat_voice_settings_migrate_without_guessing_models() {
    let migrated: crate::config::types::VoiceConfig = serde_json::from_value(serde_json::json!({
        "provider": "openai-compatible",
        "base_url": "https://voice.example/v1",
        "stt_model": "stt-real",
        "tts_model": "tts-real",
        "voice": "legacy-voice",
        "language": "zh",
        "api_key_env": "VOICE_KEY",
        "timeout_ms": 4321
    }))
    .unwrap();

    assert_eq!(migrated.stt.provider, "openai-compatible");
    assert_eq!(migrated.stt.model, "stt-real");
    assert_eq!(migrated.tts.provider, "openai-compatible");
    assert_eq!(migrated.tts.model, "tts-real");
    assert_eq!(migrated.tts.voice, "legacy-voice");
    let encoded = serde_json::to_value(&migrated).unwrap();
    assert!(encoded.get("stt").is_some());
    assert!(encoded.get("tts").is_some());
    assert!(encoded.get("provider").is_none());

    let missing_tts: crate::config::types::VoiceConfig =
        serde_json::from_value(serde_json::json!({
            "provider": "openai-compatible",
            "base_url": "https://voice.example/v1",
            "stt_model": "stt-real",
            "voice": "legacy-voice",
            "language": "zh",
            "api_key_env": "VOICE_KEY",
            "timeout_ms": 4321
        }))
        .unwrap();
    assert!(missing_tts.tts.model.is_empty());
    assert!(!missing_tts.tts.structurally_configured());
}

#[tokio::test]
async fn voice_settings_redact_stt_and_tts_secrets_independently() {
    let path = std::env::temp_dir().join(format!(
        "yilian-settings-voice-split-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = AppServer::new_with_control_session_and_store(
        &path,
        ".",
        ControlSession::generate(),
        Arc::clone(&store),
    )
    .unwrap();
    let stt_ref = SecretRef::new(crate::secret::STT_KEY_REF);
    let tts_ref = SecretRef::new(crate::secret::TTS_KEY_REF);
    store
        .put(&stt_ref, SecretString::from("STT_SECRET"))
        .await
        .unwrap();
    store
        .put(&tts_ref, SecretString::from("TTS_SECRET"))
        .await
        .unwrap();
    {
        let mut config = server.config.write();
        config.voice.stt.api_key_ref = Some(stt_ref);
        config.voice.tts.api_key_ref = Some(tts_ref);
    }

    let value = build_redacted_config(&server).await;
    let text = value.to_string();
    assert!(!text.contains("STT_SECRET"));
    assert!(!text.contains("TTS_SECRET"));
    assert_eq!(value["voice"]["stt"]["api_key"], "");
    assert_eq!(value["voice"]["stt"]["api_key_configured"], true);
    assert_eq!(value["voice"]["tts"]["api_key"], "");
    assert_eq!(value["voice"]["tts"]["api_key_configured"], true);

    drop(server);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn clear_delete_failure_preserves_each_persisted_secret_reference() {
    for kind in ["model", "embedding", "stt", "tts"] {
        let path = std::env::temp_dir().join(format!(
            "yilian-settings-clear-failure-{kind}-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store: Arc<dyn SecretStore> = Arc::new(DeleteFailingStore::new());
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &path,
                ".",
                ControlSession::generate(),
                Arc::clone(&store),
            )
            .unwrap(),
        );
        let secret_ref = SecretRef::new(format!("test.clear.failure.{kind}"));
        store
            .put(&secret_ref, SecretString::from("test-clear-key"))
            .await
            .unwrap();
        let mut persisted = server.config.read().clone();
        match kind {
            "model" => persisted.model.api_key_ref = Some(secret_ref.clone()),
            "embedding" => persisted.model.embedding_api_key_ref = Some(secret_ref.clone()),
            "stt" => {
                persisted.voice.stt.model = "test-stt".to_string();
                persisted.voice.stt.api_key_ref = Some(secret_ref.clone());
            }
            "tts" => persisted.voice.tts.api_key_ref = Some(secret_ref.clone()),
            _ => unreachable!(),
        }
        server.db.save_settings(&persisted).unwrap();
        *server.config.write() = persisted.clone();

        let mut incoming = persisted;
        match kind {
            "model" => incoming.model.clear_api_key = true,
            "embedding" => incoming.model.clear_embedding_api_key = true,
            "stt" => incoming.voice.stt.clear_api_key = true,
            "tts" => incoming.voice.tts.clear_api_key = true,
            _ => unreachable!(),
        }
        let response = update_handler(State(Arc::clone(&server)), Json(incoming))
            .await
            .0;
        assert!(
            response.get("error").is_some(),
            "{kind} clear must fail closed"
        );

        let current = server.config.read().clone();
        let retained = match kind {
            "model" => current.model.api_key_ref,
            "embedding" => current.model.embedding_api_key_ref,
            "stt" => current.voice.stt.api_key_ref,
            "tts" => current.voice.tts.api_key_ref,
            _ => unreachable!(),
        };
        assert_eq!(
            retained,
            Some(secret_ref.clone()),
            "{kind} ref must remain live"
        );
        assert!(matches!(store.get(&secret_ref).await, Ok(Some(_))));

        let saved = server.db.get_settings().unwrap();
        let persisted_ref = match kind {
            "model" => saved.model.api_key_ref,
            "embedding" => saved.model.embedding_api_key_ref,
            "stt" => saved.voice.stt.api_key_ref,
            "tts" => saved.voice.tts.api_key_ref,
            _ => unreachable!(),
        };
        assert_eq!(persisted_ref, Some(secret_ref));

        drop(server);
        let _ = std::fs::remove_file(&path);
    }
}

#[tokio::test]
async fn settings_get_projects_active_model_provider_readiness_without_secrets() {
    let path = std::env::temp_dir().join(format!(
        "yilian-settings-readiness-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = AppServer::new_with_control_session_and_store(
        &path,
        ".",
        ControlSession::generate(),
        Arc::clone(&store),
    )
    .unwrap();

    let active_ref = SecretRef::new("llm.readiness-active");
    store
        .put(&active_ref, SecretString::from("test-active-key"))
        .await
        .unwrap();
    server
        .db
        .create_llm_model(&crate::db::LlmModelInput {
            id: "readiness-active".into(),
            provider: "profile-provider".into(),
            label: "Active profile".into(),
            model: "profile-model".into(),
            base_url: "https://provider.example/v1".into(),
            api_format: "openai".into(),
            api_key_ref: active_ref.key.clone(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 128,
            invoke_timeout_ms: 5_000,
        })
        .unwrap();
    server.db.activate_llm_model("readiness-active").unwrap();

    let value = build_redacted_config(&server).await;
    let readiness = &value["provider_readiness"];
    assert_eq!(readiness["model"]["provider"], "profile-provider");
    assert_eq!(readiness["model"]["model"], "profile-model");
    assert_eq!(readiness["model"]["configured"], true);
    assert_eq!(readiness["model"]["available"], true);
    assert!(!value.to_string().contains("test-active-key"));

    drop(server);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn provider_readiness_fails_closed_for_unsupported_voice_provider() {
    let path = std::env::temp_dir().join(format!(
        "yilian-settings-unsupported-voice-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = AppServer::new_with_control_session_and_store(
        &path,
        ".",
        ControlSession::generate(),
        Arc::clone(&store),
    )
    .unwrap();
    let stt_ref = SecretRef::new(STT_KEY_REF);
    store
        .put(&stt_ref, SecretString::from("test-stt-key"))
        .await
        .unwrap();
    {
        let mut config = server.config.write();
        config.voice.stt.provider = "unsupported-provider".into();
        config.voice.stt.model = "configured-model".into();
        config.voice.stt.api_key_ref = Some(stt_ref);
    }

    let readiness = build_redacted_config(&server).await;
    assert_eq!(readiness["provider_readiness"]["stt"]["configured"], true);
    assert_eq!(readiness["provider_readiness"]["stt"]["available"], false);

    drop(server);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn voice_connection_errors_are_reduced_to_the_shared_safe_vocabulary() {
    assert_eq!(
        normalize_voice_connection_error(&VoiceProviderError::AuthFailed),
        "INVALID_CREDENTIAL"
    );
    assert_eq!(
        normalize_voice_connection_error(&VoiceProviderError::RequestFailed(404)),
        "MODEL_NOT_FOUND"
    );
    assert_eq!(
        normalize_voice_connection_error(&VoiceProviderError::RateLimited),
        "RATE_LIMITED"
    );
    assert_eq!(
        normalize_voice_connection_error(&VoiceProviderError::Timeout),
        "TIMEOUT"
    );
    assert_eq!(
        normalize_voice_connection_error(&VoiceProviderError::RequestFailed(503)),
        "PROVIDER_UNREACHABLE"
    );
    assert_eq!(
        normalize_voice_connection_error(&VoiceProviderError::InvalidText),
        "INVALID_CONFIGURATION"
    );
}

#[tokio::test]
async fn provider_connection_leaf_rejects_an_unknown_kind_without_a_network_call() {
    let path = std::env::temp_dir().join(format!(
        "yilian-settings-provider-leaf-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            store,
        )
        .unwrap(),
    );

    let error = verify_provider_handler(State(server), Path("unknown".to_string()))
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::BAD_REQUEST);
    assert_eq!(error.1 .0["error"], "INVALID_CONFIGURATION");

    let _ = std::fs::remove_file(&path);
}

#[derive(Clone, Default)]
struct ProviderMockCalls(Arc<std::sync::Mutex<Vec<(String, String)>>>);

async fn provider_mock_server(
    failure_statuses: Option<(StatusCode, StatusCode, StatusCode)>,
) -> (String, ProviderMockCalls, tokio::task::JoinHandle<()>) {
    use axum::{body::Bytes, http::header::CONTENT_TYPE, routing::post, Router};

    let calls = ProviderMockCalls::default();
    let model_calls = calls.clone();
    let stt_calls = calls.clone();
    let tts_calls = calls.clone();
    let (model_status, stt_status, tts_status) =
        failure_statuses.unwrap_or((StatusCode::OK, StatusCode::OK, StatusCode::OK));
    let app = Router::new()
        .route(
            "/chat/completions",
            post(move |Json(body): Json<serde_json::Value>| {
                let calls = model_calls.clone();
                async move {
                    calls
                        .0
                        .lock()
                        .unwrap()
                        .push(("model".to_string(), body.to_string()));
                    if model_status == StatusCode::OK {
                        (
                            StatusCode::OK,
                            Json(serde_json::json!({
                                "id": "mock-model-response",
                                "choices": [{
                                    "index": 0,
                                    "message": { "role": "assistant", "content": "ok" },
                                    "finish_reason": "stop"
                                }]
                            })),
                        )
                    } else {
                        (
                            model_status,
                            Json(serde_json::json!({ "detail": "mock-provider-response-body" })),
                        )
                    }
                }
            }),
        )
        .route(
            "/audio/transcriptions",
            post(move |body: Bytes| {
                let calls = stt_calls.clone();
                async move {
                    calls.0.lock().unwrap().push((
                        "stt".to_string(),
                        String::from_utf8_lossy(&body).to_string(),
                    ));
                    if stt_status == StatusCode::OK {
                        (
                            StatusCode::OK,
                            Json(serde_json::json!({ "text": "mock transcript" })),
                        )
                    } else {
                        (
                            stt_status,
                            Json(serde_json::json!({ "detail": "mock-provider-response-body" })),
                        )
                    }
                }
            }),
        )
        .route(
            "/audio/speech",
            post(move |Json(body): Json<serde_json::Value>| {
                let calls = tts_calls.clone();
                async move {
                    calls
                        .0
                        .lock()
                        .unwrap()
                        .push(("tts".to_string(), body.to_string()));
                    if tts_status == StatusCode::OK {
                        (
                            StatusCode::OK,
                            [(CONTENT_TYPE, "audio/mpeg")],
                            vec![1u8, 2, 3],
                        )
                    } else {
                        (
                            tts_status,
                            [(CONTENT_TYPE, "application/json")],
                            serde_json::to_vec(
                                &serde_json::json!({ "detail": "mock-provider-response-body" }),
                            )
                            .unwrap(),
                        )
                    }
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base_url, calls, task)
}

async fn provider_test_server(base_url: String) -> Arc<AppServer> {
    let path = std::env::temp_dir().join(format!(
        "yilian-settings-provider-mock-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap(),
    );
    let chat_ref = SecretRef::new("test.provider.model");
    let stt_ref = SecretRef::new("test.provider.stt");
    let tts_ref = SecretRef::new("test.provider.tts");
    for secret_ref in [&chat_ref, &stt_ref, &tts_ref] {
        store
            .put(secret_ref, SecretString::from("test-provider-key"))
            .await
            .unwrap();
    }
    let mut config = server.config.write();
    config.model.base_url = base_url.clone();
    config.model.name = "mock-model".to_string();
    config.model.api_key_ref = Some(chat_ref);
    config.voice.stt.provider = "openai-compatible".to_string();
    config.voice.stt.base_url = base_url.clone();
    config.voice.stt.model = "mock-stt".to_string();
    config.voice.stt.api_key_ref = Some(stt_ref);
    config.voice.tts.provider = "openai-compatible".to_string();
    config.voice.tts.base_url = base_url;
    config.voice.tts.model = "mock-tts".to_string();
    config.voice.tts.voice = "mock-voice".to_string();
    config.voice.tts.api_key_ref = Some(tts_ref);
    drop(config);
    server
}

#[tokio::test]
async fn provider_connection_leaf_makes_minimal_model_stt_and_tts_mock_calls() {
    let (base_url, calls, task) = provider_mock_server(None).await;
    let server = provider_test_server(base_url).await;
    for kind in ["model", "stt", "tts"] {
        let response = verify_provider_handler(State(Arc::clone(&server)), Path(kind.to_string()))
            .await
            .unwrap();
        assert_eq!(response.0["status"], "ok");
    }
    let calls = calls.0.lock().unwrap();
    assert!(calls.iter().any(|(kind, body)| kind == "model"
        && body.contains("mock-model")
        && body.contains("ping")));
    assert!(calls
        .iter()
        .any(|(kind, body)| kind == "stt" && body.contains("provider-connection-test.wav")));
    assert!(calls.iter().any(|(kind, body)| kind == "tts"
        && body.contains("你好，我是小涟。")
        && body.contains("mock-tts")));
    task.abort();
}

#[tokio::test]
async fn provider_connection_leaf_normalizes_mock_failures_without_response_bodies() {
    let (base_url, _calls, task) = provider_mock_server(Some((
        StatusCode::UNAUTHORIZED,
        StatusCode::NOT_FOUND,
        StatusCode::TOO_MANY_REQUESTS,
    )))
    .await;
    let server = provider_test_server(base_url).await;
    for (kind, expected) in [
        ("model", "INVALID_CREDENTIAL"),
        ("stt", "MODEL_NOT_FOUND"),
        ("tts", "RATE_LIMITED"),
    ] {
        let error = verify_provider_handler(State(Arc::clone(&server)), Path(kind.to_string()))
            .await
            .unwrap_err();
        assert_eq!(error.1 .0["error"], expected);
        assert!(!error
            .1
             .0
            .to_string()
            .contains("mock-provider-response-body"));
    }
    task.abort();
}

#[tokio::test]
async fn empty_key_preserves_secret_across_settings_changes_and_restart() {
    let path = std::env::temp_dir().join(format!(
        "yilian-settings-restart-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store: Arc<dyn crate::secret::SecretStore> = Arc::new(InMemorySecretStore::new());
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &path,
            ".",
            ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap(),
    );

    let mut first = server.config.read().clone();
    first.model.api_key = "test-replace-key".to_string();
    first.model.base_url = "https://first.example/v1".to_string();
    assert_eq!(
        update_handler(State(Arc::clone(&server)), Json(first))
            .await
            .0["status"],
        "saved"
    );

    let mut changed_without_key = server.config.read().clone();
    changed_without_key.model.api_key = String::new();
    changed_without_key.model.base_url = "https://second.example/v1".to_string();
    assert_eq!(
        update_handler(State(Arc::clone(&server)), Json(changed_without_key))
            .await
            .0["status"],
        "saved"
    );
    assert_eq!(
        server.config.read().model.api_key_ref,
        Some(SecretRef::new(CHAT_KEY_REF))
    );

    drop(server);
    let restarted = AppServer::new_with_control_session_and_store(
        &path,
        ".",
        ControlSession::generate(),
        store,
    )
    .unwrap();
    let redacted = build_redacted_config(&restarted).await;
    assert_eq!(redacted["model"]["api_key"], "");
    assert_eq!(redacted["model"]["api_key_configured"], true);
    assert_eq!(redacted["model"]["base_url"], "https://second.example/v1");
    assert!(matches!(
        restarted
            .secret_resolver
            .resolve_ref(&SecretRef::new(CHAT_KEY_REF))
            .await,
        Ok(Some(_))
    ));

    drop(restarted);
    let _ = std::fs::remove_file(&path);
}
