//! Settings update — the write path for `PUT /api/settings`.
//!
//! The per-key state machine is preserved verbatim from `api/settings.rs`:
//!   explicit clear    -> DELETE
//!   non-empty new key -> REPLACE
//!   empty key         -> KEEP the existing reference

use axum::{http::StatusCode, Json};
use secrecy::SecretString;
use serde_json::{json, Value};

use crate::app::state::AppServer;
use crate::config::types::{AppConfig, SkillsConfig, SubagentsConfig};
use crate::db::settings::SettingsSaveError;
use crate::integrations::secret::{
    record_secret_event, SecretKind, SecretRef, CHAT_KEY_REF, EMBEDDING_KEY_REF, STT_KEY_REF,
    TTS_KEY_REF, VOICE_KEY_REF,
};
use crate::safety::AuditEventType;

use crate::modules::settings::application::model_defaults::is_account_secret;
use crate::modules::settings::application::secret_lifecycle::delete_secret_or_fail;
use crate::modules::settings::domain::policy::{
    valid_model_max_tokens, valid_model_temperature, valid_model_timeout,
};

pub(crate) async fn apply(
    server: &AppServer,
    incoming: AppConfig,
    expected_revision: Option<u64>,
) -> (StatusCode, Json<Value>) {
    let _update = server.settings_update_lock.lock().await;
    let revision = match server.db.get_settings_revision() {
        Ok(revision) => revision,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": error})),
            )
        }
    };
    let conflict = |current| {
        (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "全局配置已有更新，请重新载入后保存",
                "code": "revision_conflict",
                "revision": current,
            })),
        )
    };
    if expected_revision.is_some_and(|expected| expected != revision) {
        return conflict(revision);
    }
    let existing = server.config.read().clone();
    if let Err((field, message)) = validate_changed_values(&existing, &incoming) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": message, "code": "invalid_settings", "field": field,
            })),
        );
    }
    let pending_application = directory_actions(&existing, &incoming);
    let incoming = match apply_secrets(server, incoming, &existing).await {
        Ok(config) => config,
        Err(body) => return (StatusCode::OK, body),
    };
    let browser_permissions_changed = incoming.permissions.mode != existing.permissions.mode
        || incoming.permissions.interrupt_on != existing.permissions.interrupt_on
        || serde_json::to_value(&incoming.sandbox).ok()
            != serde_json::to_value(&existing.sandbox).ok();
    let mut current = server.config.write();
    match server
        .db
        .save_settings_with_revision(&incoming, Some(revision))
    {
        Ok(revision) => {
            *current = incoming;
            let embedding_changed =
                crate::db::memory_governance::embedding_identity(&current.model)
                    != crate::db::memory_governance::embedding_identity(&existing.model);
            let should_rebuild_local_index = current.model.has_local_embedding();
            drop(current);
            if embedding_changed {
                server.memory_indexer.cancel();
                if should_rebuild_local_index {
                    let _ = server.memory_indexer.start(server);
                }
            }
            if browser_permissions_changed {
                server.browser_runtime.invalidate_authorizations();
            }
            let mut receipt = json!({
                "status": "saved",
                "revision": revision,
                "application": "next_read",
            });
            if !pending_application.is_empty() {
                receipt["pending_application"] = json!(pending_application);
            }
            (StatusCode::OK, Json(receipt))
        }
        Err(SettingsSaveError::RevisionConflict { current }) => conflict(current),
        Err(error) => (StatusCode::OK, Json(json!({"error": error.to_string()}))),
    }
}

fn validate_changed_values(
    existing: &AppConfig,
    incoming: &AppConfig,
) -> Result<(), (&'static str, &'static str)> {
    let model = &incoming.model;
    if model.temperature != existing.model.temperature
        && !valid_model_temperature(model.temperature)
    {
        return Err(("model.temperature", "温度必须在 0 到 2 之间"));
    }
    if model.max_tokens != existing.model.max_tokens && !valid_model_max_tokens(model.max_tokens) {
        return Err(("model.max_tokens", "最大 Token 必须在 1 到 1000000 之间"));
    }
    if model.invoke_timeout_ms != existing.model.invoke_timeout_ms
        && !valid_model_timeout(model.invoke_timeout_ms)
    {
        return Err((
            "model.invoke_timeout_ms",
            "超时必须在 1000 到 600000 毫秒之间",
        ));
    }
    let compaction = &incoming.compaction;
    let enabling = !existing.compaction.enabled && compaction.enabled;
    if (enabling || compaction.context_window != existing.compaction.context_window)
        && !compaction.valid_context_window()
    {
        return Err(("compaction.context_window", "上下文窗口须大于 0"));
    }
    if (enabling || compaction.trigger_threshold != existing.compaction.trigger_threshold)
        && !compaction.valid_trigger_threshold()
    {
        return Err((
            "compaction.trigger_threshold",
            "压缩触发阈值必须在 0 到 1 之间",
        ));
    }
    Ok(())
}

fn directory_actions(existing: &AppConfig, incoming: &AppConfig) -> Vec<Value> {
    let mut actions = Vec::new();
    if directories_changed(
        &existing.skills.directories,
        &incoming.skills.directories,
        &SkillsConfig::default().directories,
    ) {
        actions.push(json!({"field":"skills.directories","action":"refresh_skill_discovery"}));
    }
    if directories_changed(
        &existing.subagents.directories,
        &incoming.subagents.directories,
        &SubagentsConfig::default().directories,
    ) {
        actions.push(json!({"field":"subagents.directories","action":"restart_application"}));
    }
    actions
}

fn directories_changed(existing: &[String], incoming: &[String], default: &[String]) -> bool {
    let before = if existing.is_empty() {
        default
    } else {
        existing
    };
    let after = if incoming.is_empty() {
        default
    } else {
        incoming
    };
    before != after
}

async fn apply_secrets(
    server: &AppServer,
    mut incoming: AppConfig,
    existing: &AppConfig,
) -> Result<AppConfig, Json<Value>> {
    incoming.agent.system_prompt = existing.agent.system_prompt.clone();
    let accounts = match server.db.list_provider_accounts() {
        Ok(accounts) => accounts,
        Err(_) => return Err(Json(json!({ "error": "无法读取厂商凭据引用" }))),
    };
    let owned_secret = |reference: &SecretRef| !is_account_secret(&accounts, reference);

    // ── Chat API key: clear → delete; non-empty → rotate; empty → preserve. ──
    if incoming.model.clear_api_key {
        if let Some(secret_ref) = existing.model.api_key_ref.clone() {
            delete_secret_or_fail(server, &secret_ref, SecretKind::ChatApiKey).await?;
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
            return Err(Json(
                json!({ "error": format!("系统安全凭据库不可用，API Key 未保存: {e}") }),
            ));
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
        if let Some(secret_ref) = existing
            .model
            .embedding_api_key_ref
            .clone()
            .filter(&owned_secret)
        {
            delete_secret_or_fail(server, &secret_ref, SecretKind::EmbeddingApiKey).await?;
        }
        incoming.model.embedding_api_key_ref = None;
        incoming.model.embedding_api_key = String::new();
    } else if !incoming.model.embedding_api_key.is_empty() {
        let secret_ref = existing
            .model
            .embedding_api_key_ref
            .clone()
            .filter(&owned_secret)
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
            return Err(Json(
                json!({ "error": format!("系统安全凭据库不可用，Embedding API Key 未保存: {e}") }),
            ));
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
        incoming.model.embedding_api_key_ref =
            existing
                .model
                .embedding_api_key_ref
                .clone()
                .filter(|reference| {
                    owned_secret(reference)
                        || incoming.model.embedding_base_url == existing.model.embedding_base_url
                });
        incoming.model.embedding_api_key = String::new();
    }
    incoming.model.clear_embedding_api_key = false;

    // ── STT API key ──
    if incoming.voice.stt.clear_api_key {
        if let Some(secret_ref) = existing.voice.stt.api_key_ref.clone().filter(&owned_secret) {
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
            } else {
                delete_secret_or_fail(server, &secret_ref, SecretKind::VoiceSttApiKey).await?;
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
            .filter(|value| value.key != VOICE_KEY_REF && owned_secret(value))
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
            return Err(Json(
                json!({ "error": format!("系统安全凭据库不可用，STT API Key 未保存: {e}") }),
            ));
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
        incoming.voice.stt.api_key_ref =
            existing.voice.stt.api_key_ref.clone().filter(|reference| {
                owned_secret(reference)
                    || (incoming.voice.stt.base_url == existing.voice.stt.base_url
                        && incoming.voice.stt.provider == existing.voice.stt.provider)
            });
        incoming.voice.stt.api_key = String::new();
    }
    incoming.voice.stt.clear_api_key = false;

    // ── TTS API key ──
    if incoming.voice.tts.clear_api_key {
        if let Some(secret_ref) = existing.voice.tts.api_key_ref.clone().filter(&owned_secret) {
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
            } else {
                delete_secret_or_fail(server, &secret_ref, SecretKind::VoiceTtsApiKey).await?;
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
            .filter(|value| value.key != VOICE_KEY_REF && owned_secret(value))
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
            return Err(Json(
                json!({ "error": format!("系统安全凭据库不可用，TTS API Key 未保存: {e}") }),
            ));
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
        incoming.voice.tts.api_key_ref =
            existing.voice.tts.api_key_ref.clone().filter(|reference| {
                owned_secret(reference)
                    || (incoming.voice.tts.base_url == existing.voice.tts.base_url
                        && incoming.voice.tts.provider == existing.voice.tts.provider)
            });
        incoming.voice.tts.api_key = String::new();
    }
    incoming.voice.tts.clear_api_key = false;

    Ok(incoming)
}
