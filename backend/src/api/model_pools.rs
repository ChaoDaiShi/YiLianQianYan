use crate::{
    app::state::AppServer,
    db::model_pools::{ModelPool, ModelPoolInput, ProviderAccount},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
type Error = (StatusCode, String);
fn error(e: String) -> Error {
    (
        if e == "revision_conflict" {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        },
        e,
    )
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/provider-presets", get(presets))
        .route("/api/model-pool-attempts", get(attempts))
        .route("/api/model-pools", get(list).post(create))
        .route("/api/model-pools/:id", put(update).delete(remove))
        .route(
            "/api/model-pools/:id/invoke",
            axum::routing::post(invoke_pool),
        )
        .route("/api/model-pool-bindings/:subject", get(binding).put(bind))
        .route("/api/provider-accounts", get(accounts).post(save_account))
        .route(
            "/api/provider-accounts/:id",
            put(update_account).delete(remove_account),
        )
}
async fn list(State(s): State<Arc<AppServer>>) -> Result<Json<Vec<ModelPool>>, Error> {
    s.db.list_model_pools().map(Json).map_err(error)
}
async fn create(
    State(s): State<Arc<AppServer>>,
    Json(input): Json<ModelPoolInput>,
) -> Result<Json<ModelPool>, Error> {
    s.db.save_model_pool(&uuid::Uuid::new_v4().to_string(), input)
        .map(Json)
        .map_err(error)
}
async fn update(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<ModelPoolInput>,
) -> Result<Json<ModelPool>, Error> {
    if input.expected_revision.is_none() {
        return Err(error("更新必须提供 expected_revision".into()));
    }
    s.db.save_model_pool(&id, input).map(Json).map_err(error)
}
#[derive(Deserialize)]
struct Revision {
    expected_revision: u64,
}
async fn remove(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(q): Query<Revision>,
) -> Result<StatusCode, Error> {
    s.db.delete_model_pool(&id, q.expected_revision)
        .map_err(error)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(serde::Serialize, Deserialize)]
struct Binding {
    pool_id: Option<String>,
}
async fn binding(
    State(s): State<Arc<AppServer>>,
    Path(subject): Path<String>,
) -> Result<Json<Binding>, Error> {
    Ok(Json(Binding {
        pool_id: s.db.get_model_pool_binding(&subject).map_err(error)?,
    }))
}
async fn bind(
    State(s): State<Arc<AppServer>>,
    Path(subject): Path<String>,
    Json(b): Json<Binding>,
) -> Result<Json<Binding>, Error> {
    s.db.set_model_pool_binding(&subject, b.pool_id.as_deref())
        .map_err(error)?;
    Ok(Json(b))
}
#[derive(serde::Serialize)]
struct AccountView {
    #[serde(flatten)]
    account: ProviderAccount,
    credential_status: &'static str,
    credential_configured: bool,
}
async fn account_view(s: &AppServer, account: ProviderAccount) -> AccountView {
    let required = !matches!(account.provider_id.as_str(), "ollama" | "lmstudio");
    let credential = match account.credential_id.as_ref() {
        Some(id) => {
            s.secret_resolver
                .resolve_ref(&crate::integrations::secret::SecretRef::new(id.clone()))
                .await
        }
        None => Ok(None),
    };
    let (credential_status, credential_configured) = match credential {
        Ok(Some(_)) => ("configured", true),
        Ok(None) if !required => ("not_required", false),
        Ok(None) => ("missing", false),
        Err(_) => ("unavailable", false),
    };
    AccountView {
        account,
        credential_status,
        credential_configured,
    }
}
async fn accounts(State(s): State<Arc<AppServer>>) -> Result<Json<Vec<AccountView>>, Error> {
    let accounts = s.db.list_provider_accounts().map_err(error)?;
    let mut views = Vec::with_capacity(accounts.len());
    for account in accounts {
        views.push(account_view(&s, account).await);
    }
    Ok(Json(views))
}
#[derive(Deserialize)]
struct AccountInput {
    #[serde(flatten)]
    account: ProviderAccount,
    #[serde(default)]
    credential_action: Option<String>,
    #[serde(default)]
    credential_model_id: Option<String>,
    #[serde(default)]
    credential_fields: std::collections::HashMap<String, String>,
}
async fn persist_account(s: &AppServer, mut a: AccountInput) -> Result<ProviderAccount, Error> {
    let parsed =
        url::Url::parse(&a.account.base_url).map_err(|_| error("账户地址必须是有效 URL".into()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(error("账户地址必须是 http(s) URL".into()));
    }
    s.db.validate_provider_account(&a.account).map_err(error)?;
    let old =
        s.db.list_provider_accounts()
            .map_err(error)?
            .into_iter()
            .find(|old| old.id == a.account.id);
    let existing_key = old.as_ref().and_then(|o| o.credential_id.clone());
    // Reuse is an explicit configuration operation. Runtime resolution still
    // reads exactly the selected account and never searches other models.
    a.account.credential_id = match a.credential_action.as_deref().unwrap_or("keep") {
        "replace" => {
            let key = existing_key.unwrap_or_else(|| format!("llm.account.{}", a.account.id));
            let secret_ref = crate::integrations::secret::SecretRef::new(key.clone());
            let value = a
                .credential_fields
                .get("api_key")
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| error("替换凭据需要 API Key".into()))?;
            s.secret_store
                .put(&secret_ref, secrecy::SecretString::from(value.clone()))
                .await
                .map_err(|_| error("凭据库不可用".into()))?;
            Some(key)
        }
        "clear" => {
            if let Some(key) = &existing_key {
                s.secret_store
                    .delete(&crate::integrations::secret::SecretRef::new(key.clone()))
                    .await
                    .map_err(|_| error("凭据清除失败".into()))?;
            }
            existing_key
        }
        "keep" => existing_key,
        "reuse_model" => {
            let id = a
                .credential_model_id
                .as_deref()
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| error("请选择要复用凭据的模型".into()))?;
            let model =
                s.db.get_llm_model(id)
                    .map_err(error)?
                    .ok_or_else(|| error("所选凭据来源模型不存在".into()))?;
            if model.provider != a.account.provider_id
                || model.base_url.trim().trim_end_matches('/')
                    != a.account.base_url.trim().trim_end_matches('/')
            {
                return Err(error("只能复用同一服务商和 API 地址的模型凭据".into()));
            }
            let reference = crate::integrations::secret::SecretRef::new(model.api_key_ref.clone());
            if s.secret_resolver
                .resolve_ref(&reference)
                .await
                .map_err(|_| error("凭据库不可用".into()))?
                .is_none()
            {
                return Err(error(
                    "所选模型没有可复用的已保存凭据，请填写 API Key".into(),
                ));
            }
            Some(model.api_key_ref)
        }
        _ => return Err(error("无效凭据操作".into())),
    };
    s.db.save_provider_account(&a.account).map_err(error)?;
    Ok(a.account)
}

#[cfg(test)]
#[path = "model_pools/credential_tests.rs"]
mod credential_tests;
async fn save_account(
    State(s): State<Arc<AppServer>>,
    Json(mut a): Json<AccountInput>,
) -> Result<Json<AccountView>, Error> {
    a.account.id = uuid::Uuid::new_v4().to_string();
    let account = persist_account(&s, a).await?;
    Ok(Json(account_view(&s, account).await))
}
async fn update_account(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(mut a): Json<AccountInput>,
) -> Result<Json<AccountView>, Error> {
    a.account.id = id;
    let account = persist_account(&s, a).await?;
    Ok(Json(account_view(&s, account).await))
}
async fn remove_account(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    s.db.delete_provider_account(&id).map_err(error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct AttemptQuery {
    request_id: Option<String>,
}
async fn attempts(
    State(s): State<Arc<AppServer>>,
    Query(q): Query<AttemptQuery>,
) -> Result<Json<Vec<serde_json::Value>>, Error> {
    s.db.list_pool_attempts(q.request_id.as_deref())
        .map(Json)
        .map_err(error)
}

async fn presets() -> Result<Json<Vec<serde_json::Value>>, Error> {
    let catalog: serde_json::Map<String, serde_json::Value> = serde_json::from_str(include_str!(
        "../../../frontend/src/features/llm/providerCatalog.json"
    ))
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "厂商目录无效".into()))?;
    Ok(Json(catalog.into_values().collect()))
}

async fn invoke_pool(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<crate::modules::settings::application::basic_services::BasicServiceRequest>,
) -> Result<Json<serde_json::Value>, Error> {
    let pool =
        s.db.get_model_pool(&id)
            .map_err(error)?
            .ok_or_else(|| error("模型池不存在".into()))?;
    if !pool.enabled || !matches!(pool.service_kind.as_str(), "vision" | "image") {
        return Err(error("请使用已启用的识图或生图模型池".into()));
    }
    let profiles = s.db.list_basic_service_profiles().map_err(error)?;
    crate::modules::settings::application::basic_services::invoke_service_pool(
        &s.db,
        &s.secret_resolver,
        &pool,
        &profiles,
        request,
        "",
        None,
    )
    .await
    .map(Json)
    .map_err(|e| error(e.to_string()))
}
