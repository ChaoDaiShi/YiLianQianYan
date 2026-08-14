// ============================================================
// Settings API handlers
// ============================================================

use axum::{extract::State, Json};
use std::sync::Arc;

use crate::config::types::AppConfig;
use crate::server::AppServer;

fn redacted_config(config: &AppConfig) -> serde_json::Value {
    let mut value = serde_json::to_value(config).unwrap_or_else(|_| serde_json::json!({}));
    let chat_configured = config.model.resolve_api_key().is_some();
    let embedding_configured = config.model.resolve_embedding_api_key().is_some();

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
            serde_json::Value::Bool(embedding_configured),
        );
    }

    value
}

fn preserve_secrets(mut incoming: AppConfig, existing: &AppConfig) -> AppConfig {
    if incoming.model.api_key.is_empty() {
        incoming.model.api_key = existing.model.api_key.clone();
    }
    if incoming.model.embedding_api_key.is_empty() {
        incoming.model.embedding_api_key = existing.model.embedding_api_key.clone();
    }
    incoming
}

pub async fn get_handler(State(server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(redacted_config(&server.config.read()))
}

pub async fn update_handler(
    State(server): State<Arc<AppServer>>,
    Json(config): Json<AppConfig>,
) -> Json<serde_json::Value> {
    let existing = server.config.read().clone();
    let config = preserve_secrets(config, &existing);
    match server.db.save_settings(&config) {
        Ok(_) => {
            *server.config.write() = config;
            Json(serde_json::json!({"status": "saved"}))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::AppConfig;

    #[test]
    fn redacted_config_hides_chat_and_embedding_keys() {
        let mut config = AppConfig::default();
        config.model.api_key = "chat-secret".to_string();
        config.model.embedding_api_key = "embedding-secret".to_string();

        let json = redacted_config(&config);
        let model = &json["model"];

        assert_eq!(model["api_key"], "");
        assert_eq!(model["embedding_api_key"], "");
        assert_eq!(model["api_key_configured"], true);
        assert_eq!(model["embedding_api_key_configured"], true);
        assert!(!json.to_string().contains("chat-secret"));
        assert!(!json.to_string().contains("embedding-secret"));
    }

    #[test]
    fn empty_incoming_keys_preserve_existing_secrets() {
        let mut existing = AppConfig::default();
        existing.model.api_key = "chat-secret".to_string();
        existing.model.embedding_api_key = "embedding-secret".to_string();

        let mut incoming = AppConfig::default();
        incoming.model.api_key = String::new();
        incoming.model.embedding_api_key = String::new();

        let merged = preserve_secrets(incoming, &existing);
        assert_eq!(merged.model.api_key, "chat-secret");
        assert_eq!(merged.model.embedding_api_key, "embedding-secret");
    }
}
