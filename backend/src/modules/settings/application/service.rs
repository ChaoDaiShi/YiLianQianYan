//! Settings read model — the redacted configuration projection returned by
//! `GET /api/settings`.

use crate::app::state::AppServer;

use crate::modules::settings::application::secret_lifecycle::{chat_source, embedding_source};
use crate::modules::settings::domain::readiness::{
    build_provider_readiness, voice_stt_source, voice_tts_source,
};

pub(crate) async fn build_redacted_config(server: &AppServer) -> serde_json::Value {
    let config = server.config.read().clone();
    let mut value = serde_json::to_value(&config).unwrap_or_else(|_| serde_json::json!({}));

    let (chat_src, chat_configured) = chat_source(&server.secret_resolver, &config).await;
    let (embed_src, embed_configured) = embedding_source(&server.secret_resolver, &config).await;
    let (stt_src, stt_configured) = voice_stt_source(&server.secret_resolver, &config).await;
    let (tts_src, tts_configured) = voice_tts_source(&server.secret_resolver, &config).await;
    let readiness = build_provider_readiness(&server.db, &server.secret_resolver, &config).await;

    if let Some(model) = value
        .get_mut("model")
        .and_then(serde_json::Value::as_object_mut)
    {
        model.insert(
            "api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        model.insert(
            "embedding_api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        model.insert(
            "api_key_configured".to_string(),
            serde_json::Value::Bool(chat_configured),
        );
        model.insert(
            "embedding_api_key_configured".to_string(),
            serde_json::Value::Bool(embed_configured),
        );
        model.insert(
            "api_key_source".to_string(),
            serde_json::to_value(chat_src).unwrap(),
        );
        model.insert(
            "embedding_api_key_source".to_string(),
            serde_json::to_value(embed_src).unwrap(),
        );
    }

    if let Some(stt) = value
        .pointer_mut("/voice/stt")
        .and_then(serde_json::Value::as_object_mut)
    {
        stt.insert(
            "api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        stt.insert(
            "api_key_configured".to_string(),
            serde_json::Value::Bool(stt_configured),
        );
        stt.insert(
            "api_key_source".to_string(),
            serde_json::to_value(stt_src).unwrap(),
        );
    }
    if let Some(tts) = value
        .pointer_mut("/voice/tts")
        .and_then(serde_json::Value::as_object_mut)
    {
        tts.insert(
            "api_key".to_string(),
            serde_json::Value::String(String::new()),
        );
        tts.insert(
            "api_key_configured".to_string(),
            serde_json::Value::Bool(tts_configured),
        );
        tts.insert(
            "api_key_source".to_string(),
            serde_json::to_value(tts_src).unwrap(),
        );
    }

    let migration_pending = {
        let mut pending = 0usize;
        if !config.model.api_key.is_empty() {
            pending += 1;
        }
        if !config.model.embedding_api_key.is_empty() {
            pending += 1;
        }
        if !config.voice.stt.api_key.is_empty() {
            pending += 1;
        }
        if !config.voice.tts.api_key.is_empty() {
            pending += 1;
        }
        pending += server
            .db
            .list_mcp_servers()
            .unwrap_or_default()
            .iter()
            .filter(|s| s.transport == "stdio")
            .filter(|s| {
                s.env
                    .as_ref()
                    .and_then(|e| e.as_object())
                    .map(|o| !o.is_empty())
                    .unwrap_or(false)
            })
            .count();
        pending
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "secret_store_status".to_string(),
            serde_json::to_value(server.secret_resolver.status().await).unwrap(),
        );
        obj.insert(
            "migration_pending".to_string(),
            serde_json::Value::from(migration_pending as u64),
        );
        obj.insert(
            "provider_readiness".to_string(),
            serde_json::to_value(readiness).unwrap_or_else(|_| serde_json::json!({})),
        );
    }

    value
}
