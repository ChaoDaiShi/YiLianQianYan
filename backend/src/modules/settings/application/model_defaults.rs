use crate::{
    app::state::AppServer,
    db::{basic_services::BasicServiceProfile, model_pools::ProviderAccount},
    integrations::secret::SecretRef,
};

pub(crate) fn uses_single_default(kind: &str) -> bool {
    matches!(kind, "stt" | "tts" | "embedding")
}

pub(crate) async fn use_account_model(
    server: &AppServer,
    profile: &BasicServiceProfile,
) -> Result<(), String> {
    let account = server
        .db
        .list_provider_accounts()?
        .into_iter()
        .find(|account| account.id == profile.provider_account_id)
        .ok_or("厂商账户不存在")?;
    let reference = account.credential_id.clone().map(SecretRef::new);
    match &reference {
        Some(reference)
            if server
                .secret_resolver
                .resolve_ref(reference)
                .await
                .map_err(|_| "凭据库不可用")?
                .is_some() => {}
        _ => return Err("请为该账户配置可用的 API Key".into()),
    }
    let provider = match profile.adapter.as_str() {
        "openai" => "openai-compatible",
        "minimax" => "minimax",
        _ => return Err("此用途的服务协议不受支持".into()),
    };
    let _settings_update = server.settings_update_lock.lock().await;
    let mut current = server.config.write();
    let mut next = current.clone();
    match profile.service_kind.as_str() {
        "stt" => {
            next.voice.stt.provider = provider.into();
            next.voice.stt.base_url = account.base_url;
            next.voice.stt.model.clone_from(&profile.model);
            next.voice.stt.api_key.clear();
            next.voice.stt.api_key_env.clear();
            next.voice.stt.api_key_ref = reference;
        }
        "tts" => {
            if next.voice.tts.provider != provider || next.voice.tts.base_url != account.base_url {
                next.voice.tts.voice = if provider == "minimax" {
                    "female-shaonv"
                } else {
                    "alloy"
                }
                .into();
            }
            next.voice.tts.provider = provider.into();
            next.voice.tts.base_url = account.base_url;
            next.voice.tts.model.clone_from(&profile.model);
            next.voice.tts.api_key.clear();
            next.voice.tts.api_key_env.clear();
            next.voice.tts.api_key_ref = reference;
        }
        "embedding" => {
            next.model.embedding_base_url = account.base_url;
            next.model.embedding_model.clone_from(&profile.model);
            next.model.embedding_api_key.clear();
            next.model.embedding_api_key_env.clear();
            next.model.embedding_api_key_ref = reference;
        }
        _ => return Err("此用途不使用单一默认模型".into()),
    }
    server.db.save_settings(&next)?;
    *current = next;
    Ok(())
}

pub(crate) fn active_account_model(
    server: &AppServer,
    kind: &str,
) -> Result<Option<String>, String> {
    let current = server.config.read().clone();
    let (model, base, reference) = match kind {
        "stt" => (
            &current.voice.stt.model,
            &current.voice.stt.base_url,
            &current.voice.stt.api_key_ref,
        ),
        "tts" => (
            &current.voice.tts.model,
            &current.voice.tts.base_url,
            &current.voice.tts.api_key_ref,
        ),
        "embedding" => (
            &current.model.embedding_model,
            &current.model.embedding_base_url,
            &current.model.embedding_api_key_ref,
        ),
        _ => return Ok(None),
    };
    let Some(reference) = reference else {
        return Ok(None);
    };
    let accounts = server.db.list_provider_accounts()?;
    Ok(server
        .db
        .list_basic_service_profiles()?
        .into_iter()
        .find(|profile| {
            profile.service_kind == kind
                && &profile.model == model
                && accounts.iter().any(|account| {
                    account.id == profile.provider_account_id
                        && &account.base_url == base
                        && account.credential_id.as_deref() == Some(reference.key.as_str())
                })
        })
        .map(|profile| profile.id))
}

pub(crate) fn is_account_secret(accounts: &[ProviderAccount], reference: &SecretRef) -> bool {
    accounts
        .iter()
        .any(|account| account.credential_id.as_deref() == Some(reference.key.as_str()))
}

pub(crate) fn account_has_single_default(server: &AppServer, account: &ProviderAccount) -> bool {
    let current = server.config.read();
    let matches = |base: &str, reference: Option<&SecretRef>| {
        base == account.base_url
            && reference.is_some_and(|reference| {
                account.credential_id.as_deref() == Some(reference.key.as_str())
            })
    };
    matches(
        &current.voice.stt.base_url,
        current.voice.stt.api_key_ref.as_ref(),
    ) || matches(
        &current.voice.tts.base_url,
        current.voice.tts.api_key_ref.as_ref(),
    ) || matches(
        &current.model.embedding_base_url,
        current.model.embedding_api_key_ref.as_ref(),
    )
}
