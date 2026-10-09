//! Provider readiness — the redacted `configured`/`source` projection the UI
//! reads. Never carries secret material.
//!
//! This is a domain module, so it names the two things it actually needs — the
//! database and the secret resolver — rather than the composition root. Taking
//! `&AppServer` here would make the domain layer depend on `app`, which the
//! dependency rules forbid; the boundary check enforces it.

use std::sync::Arc;

use crate::config::types::AppConfig;
use crate::db::Database;
use crate::integrations::secret::{SecretResolver, SecretSource};

use crate::modules::settings::application::secret_lifecycle::chat_source;
use crate::modules::settings::domain::policy::{supported_voice_provider, valid_provider_url};

#[derive(serde::Serialize)]
pub(crate) struct ProviderReadinessItem {
    configured: bool,
    available: bool,
    provider: String,
    model: String,
}

#[derive(serde::Serialize)]
pub(crate) struct ProviderReadiness {
    model: ProviderReadinessItem,
    stt: ProviderReadinessItem,
    tts: ProviderReadinessItem,
}

pub(crate) async fn voice_stt_source(
    resolver: &SecretResolver,
    config: &AppConfig,
) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.voice.stt.api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.voice.stt.api_key_env.is_empty() {
        let ok = std::env::var(&config.voice.stt.api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.voice.stt.api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

pub(crate) async fn voice_tts_source(
    resolver: &SecretResolver,
    config: &AppConfig,
) -> (SecretSource, bool) {
    if let Some(secret_ref) = &config.voice.tts.api_key_ref {
        let ok = matches!(resolver.resolve_ref(secret_ref).await, Ok(Some(_)));
        return (SecretSource::SecretStore, ok);
    }
    if !config.voice.tts.api_key_env.is_empty() {
        let ok = std::env::var(&config.voice.tts.api_key_env).is_ok();
        return (SecretSource::Environment, ok);
    }
    if !config.voice.tts.api_key.is_empty() {
        return (SecretSource::LegacyPending, true);
    }
    (SecretSource::None, false)
}

pub(crate) async fn build_provider_readiness(
    db: &Database,
    resolver: &Arc<SecretResolver>,
    config: &AppConfig,
) -> ProviderReadiness {
    // Keep the same precedence as chat_handler: the active profile is the
    // runtime model, while legacy settings are only its fallback.
    let active = db.get_active_llm_model().ok().flatten();
    let (runtime_model, provider, model) = match active {
        Some(profile) => {
            let provider = profile.provider.clone();
            let model = profile.model.clone();
            (profile.to_model_config(), provider, model)
        }
        None => (
            config.model.clone(),
            config.model.provider.clone(),
            config.model.name.clone(),
        ),
    };
    let runtime_config = AppConfig {
        model: runtime_model.clone(),
        ..config.clone()
    };
    let (_, model_configured) = chat_source(resolver, &runtime_config).await;
    let (_, stt_configured) = voice_stt_source(resolver, config).await;
    let (_, tts_configured) = voice_tts_source(resolver, config).await;

    ProviderReadiness {
        model: ProviderReadinessItem {
            configured: model_configured,
            available: model_configured
                && !provider.trim().is_empty()
                && !model.trim().is_empty()
                && valid_provider_url(&runtime_model.base_url)
                && runtime_model.invoke_timeout_ms > 0,
            provider,
            model,
        },
        stt: ProviderReadinessItem {
            configured: stt_configured,
            available: stt_configured
                && supported_voice_provider(&config.voice.stt.provider)
                && config.voice.stt.structurally_configured(),
            provider: config.voice.stt.provider.clone(),
            model: config.voice.stt.model.clone(),
        },
        tts: ProviderReadinessItem {
            configured: tts_configured,
            available: tts_configured
                && supported_voice_provider(&config.voice.tts.provider)
                && config.voice.tts.structurally_configured(),
            provider: config.voice.tts.provider.clone(),
            model: config.voice.tts.model.clone(),
        },
    }
}

pub(crate) fn active_runtime_model(
    db: &Database,
    config: &AppConfig,
) -> crate::config::types::ModelConfig {
    db.get_active_llm_model()
        .ok()
        .flatten()
        .map(|profile| profile.to_model_config())
        .unwrap_or_else(|| config.model.clone())
}
