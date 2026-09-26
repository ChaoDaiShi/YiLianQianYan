//! Settings update — the write path for `PUT /api/settings`.
//!
//! The per-key state machine is preserved verbatim from `api/settings.rs`:
//!   explicit clear    -> DELETE
//!   non-empty new key -> REPLACE
//!   empty key         -> KEEP the existing reference

use axum::Json;
use secrecy::SecretString;

use crate::app::state::AppServer;
use crate::config::types::AppConfig;
use crate::integrations::secret::{
    record_secret_event, SecretKind, SecretRef, CHAT_KEY_REF, EMBEDDING_KEY_REF, STT_KEY_REF,
    TTS_KEY_REF, VOICE_KEY_REF,
};
use crate::safety::AuditEventType;

use crate::modules::settings::application::secret_lifecycle::delete_secret_or_fail;

pub(crate) async fn apply(server: &AppServer, mut incoming: AppConfig) -> Json<serde_json::Value> {
    let existing = server.config.read().clone();

    // ── Chat API key: clear → delete; non-empty → rotate; empty → preserve. ──
    if incoming.model.clear_api_key {
        if let Some(secret_ref) = existing.model.api_key_ref.clone() {
            if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::ChatApiKey).await
            {
                return error;
            }
        }
        incoming.model.api_key_ref = None;
        incoming.model.api_key = String::new();
    } else if !incoming.model.api_key.is_empty() {
        let secret_ref = existing
            .model
            .api_key_ref
            .clone()
            .unwrap_or_else(|| SecretRef::new(CHAT_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.model.api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::ChatApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::ChatApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.model.api_key_ref = Some(secret_ref);
        incoming.model.api_key = String::new();
    } else {
        incoming.model.api_key_ref = existing.model.api_key_ref.clone();
        incoming.model.api_key = String::new();
    }
    incoming.model.clear_api_key = false;

    // ── Embedding API key ──
    if incoming.model.clear_embedding_api_key {
        if let Some(secret_ref) = existing.model.embedding_api_key_ref.clone() {
            if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::EmbeddingApiKey).await
            {
                return error;
            }
        }
        incoming.model.embedding_api_key_ref = None;
        incoming.model.embedding_api_key = String::new();
    } else if !incoming.model.embedding_api_key.is_empty() {
        let secret_ref = existing
            .model
            .embedding_api_key_ref
            .clone()
            .unwrap_or_else(|| SecretRef::new(EMBEDDING_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.model.embedding_api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::EmbeddingApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，Embedding API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::EmbeddingApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.model.embedding_api_key_ref = Some(secret_ref);
        incoming.model.embedding_api_key = String::new();
    } else {
        incoming.model.embedding_api_key_ref = existing.model.embedding_api_key_ref.clone();
        incoming.model.embedding_api_key = String::new();
    }
    incoming.model.clear_embedding_api_key = false;

    // ── STT API key ──
    if incoming.voice.stt.clear_api_key {
        if let Some(secret_ref) = existing.voice.stt.api_key_ref.clone() {
            let shared_legacy = secret_ref.key == VOICE_KEY_REF
                && existing.voice.tts.api_key_ref.as_ref() == Some(&secret_ref);
            if shared_legacy {
                record_secret_event(
                    &server.audit_recorder,
                    AuditEventType::SecretDeleted,
                    SecretKind::VoiceSttApiKey,
                    &secret_ref,
                    "clear_shared_legacy_ref",
                    true,
                );
            } else if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::VoiceSttApiKey).await
            {
                return error;
            }
        }
        incoming.voice.stt.api_key_ref = None;
        incoming.voice.stt.api_key = String::new();
    } else if !incoming.voice.stt.api_key.is_empty() {
        let secret_ref = existing
            .voice
            .stt
            .api_key_ref
            .clone()
            .filter(|value| value.key != VOICE_KEY_REF)
            .unwrap_or_else(|| SecretRef::new(STT_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.voice.stt.api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::VoiceSttApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，STT API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::VoiceSttApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.voice.stt.api_key_ref = Some(secret_ref);
        incoming.voice.stt.api_key = String::new();
    } else {
        incoming.voice.stt.api_key_ref = existing.voice.stt.api_key_ref.clone();
        incoming.voice.stt.api_key = String::new();
    }
    incoming.voice.stt.clear_api_key = false;

    // ── TTS API key ──
    if incoming.voice.tts.clear_api_key {
        if let Some(secret_ref) = existing.voice.tts.api_key_ref.clone() {
            let shared_legacy = secret_ref.key == VOICE_KEY_REF
                && existing.voice.stt.api_key_ref.as_ref() == Some(&secret_ref);
            if shared_legacy {
                record_secret_event(
                    &server.audit_recorder,
                    AuditEventType::SecretDeleted,
                    SecretKind::VoiceTtsApiKey,
                    &secret_ref,
                    "clear_shared_legacy_ref",
                    true,
                );
            } else if let Err(error) =
                delete_secret_or_fail(&server, &secret_ref, SecretKind::VoiceTtsApiKey).await
            {
                return error;
            }
        }
        incoming.voice.tts.api_key_ref = None;
        incoming.voice.tts.api_key = String::new();
    } else if !incoming.voice.tts.api_key.is_empty() {
        let secret_ref = existing
            .voice
            .tts
            .api_key_ref
            .clone()
            .filter(|value| value.key != VOICE_KEY_REF)
            .unwrap_or_else(|| SecretRef::new(TTS_KEY_REF));
        if let Err(e) = server
            .secret_store
            .put(
                &secret_ref,
                SecretString::from(incoming.voice.tts.api_key.clone()),
            )
            .await
        {
            record_secret_event(
                &server.audit_recorder,
                AuditEventType::SecretStoreUnavailable,
                SecretKind::VoiceTtsApiKey,
                &secret_ref,
                "rotate",
                false,
            );
            return Json(
                serde_json::json!({ "error": format!("系统安全凭据库不可用，TTS API Key 未保存: {e}") }),
            );
        }
        record_secret_event(
            &server.audit_recorder,
            AuditEventType::SecretRotated,
            SecretKind::VoiceTtsApiKey,
            &secret_ref,
            "rotate",
            true,
        );
        incoming.voice.tts.api_key_ref = Some(secret_ref);
        incoming.voice.tts.api_key = String::new();
    } else {
        incoming.voice.tts.api_key_ref = existing.voice.tts.api_key_ref.clone();
        incoming.voice.tts.api_key = String::new();
    }
    incoming.voice.tts.clear_api_key = false;

    match server.db.save_settings(&incoming) {
        Ok(_) => {
            *server.config.write() = incoming;
            Json(serde_json::json!({"status": "saved"}))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}
