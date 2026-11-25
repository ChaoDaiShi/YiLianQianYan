use std::{collections::BTreeMap, sync::Arc};

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use serde_json::{json, Value};

use crate::app::state::AppServer;

/// Account and upstream model ID identify a model. Per-purpose profiles keep
/// their original IDs because pools, agents and workflows reference them.
#[derive(Serialize)]
pub struct ProviderModel {
    provider_account_id: String,
    model: String,
    profiles: BTreeMap<String, Vec<String>>,
}

pub async fn list(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<Vec<ProviderModel>>, (StatusCode, Json<Value>)> {
    let error = |_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "无法读取供应商模型"})),
        )
    };
    let db = &server.db;
    let accounts = db.list_provider_accounts().map_err(error)?;
    let chats = db.list_llm_models().map_err(error)?;
    let services = db.list_basic_service_profiles().map_err(error)?;
    let mut entities = BTreeMap::<(String, String), BTreeMap<String, Vec<String>>>::new();
    for chat in chats {
        if let Some(account) = accounts
            .iter()
            .find(|account| account.profile_ids.contains(&chat.id))
        {
            entities
                .entry((account.id.clone(), chat.model))
                .or_default()
                .entry("chat".into())
                .or_default()
                .push(chat.id);
        }
    }
    for profile in services {
        entities
            .entry((profile.provider_account_id, profile.model))
            .or_default()
            .entry(profile.service_kind)
            .or_default()
            .push(profile.id);
    }
    Ok(Json(
        entities
            .into_iter()
            .map(|((provider_account_id, model), profiles)| ProviderModel {
                provider_account_id,
                model,
                profiles,
            })
            .collect(),
    ))
}
