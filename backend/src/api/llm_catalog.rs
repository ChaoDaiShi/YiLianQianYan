use axum::{extract::State, http::StatusCode, Json};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use crate::{app::state::AppServer, integrations::secret::SecretRef};

type Error = (StatusCode, Json<Value>);

fn error(status: StatusCode, message: &str) -> Error {
    (status, Json(json!({"error": message})))
}

#[derive(Deserialize)]
pub struct CatalogInput {
    pub provider: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub provider_account_id: Option<String>,
    pub model_id: Option<String>,
    #[serde(default)]
    pub api_key_env: String,
}

#[derive(Serialize)]
pub struct CatalogModel {
    id: String,
    name: String,
    context_window: Option<u64>,
    max_output_tokens: Option<u64>,
    input_modalities: Option<Vec<String>>,
    output_modalities: Option<Vec<String>>,
}

fn modalities(row: &Value, field: &str) -> Option<Vec<String>> {
    row.get(field)
        .or_else(|| row.get("architecture").and_then(|value| value.get(field)))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
}

fn parse_models(data: &Value) -> Result<Vec<CatalogModel>, Error> {
    let rows = data
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "上游返回的模型列表格式无效"))?;
    let mut models = BTreeMap::new();
    for row in rows {
        let Some(id) = row
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let name = row
            .get("name")
            .or_else(|| row.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(id);
        models.entry(id.to_owned()).or_insert_with(|| CatalogModel {
            id: id.to_owned(),
            name: name.to_owned(),
            context_window: row
                .get("context_window")
                .or_else(|| row.get("max_input_tokens"))
                .and_then(Value::as_u64),
            max_output_tokens: row
                .get("max_output_tokens")
                .or_else(|| row.get("max_tokens"))
                .and_then(Value::as_u64),
            input_modalities: modalities(row, "input_modalities"),
            output_modalities: modalities(row, "output_modalities"),
        });
    }
    Ok(models.into_values().collect())
}

async fn credential(
    server: &AppServer,
    input: &CatalogInput,
) -> Result<Option<SecretString>, Error> {
    if !input.api_key.trim().is_empty() {
        return Ok(Some(SecretString::from(input.api_key.trim().to_owned())));
    }
    let same_endpoint = |provider: &str, base: &str| {
        provider == input.provider
            && base.trim().trim_end_matches('/') == input.base_url.trim().trim_end_matches('/')
    };
    let reference = if let Some(id) = &input.provider_account_id {
        let account = server
            .db
            .list_provider_accounts()
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "无法读取服务商账户"))?
            .into_iter()
            .find(|account| &account.id == id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "服务商账户不存在"))?;
        if !same_endpoint(&account.provider_id, &account.base_url) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "账户与所选服务商或地址不一致",
            ));
        }
        account.credential_id
    } else if let Some(id) = &input.model_id {
        let model = server
            .db
            .get_llm_model(id)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "无法读取模型凭据"))?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "模型档案不存在"))?;
        if !same_endpoint(&model.provider, &model.base_url) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "凭据来源与所选服务商或地址不一致",
            ));
        }
        return server
            .secret_resolver
            .resolve_api_key(&super::llm_models::to_model_config(&model))
            .await
            .map_err(|_| error(StatusCode::BAD_REQUEST, "本机凭据存储暂时不可用"));
    } else {
        None
    };
    if let Some(reference) = reference {
        return server
            .secret_resolver
            .resolve_ref(&SecretRef::new(reference))
            .await
            .map_err(|_| error(StatusCode::BAD_REQUEST, "本机凭据存储暂时不可用"));
    }
    Ok((!input.api_key_env.trim().is_empty())
        .then(|| std::env::var(input.api_key_env.trim()).ok())
        .flatten()
        .map(SecretString::from))
}

pub async fn discover(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<CatalogInput>,
) -> Result<Json<Value>, Error> {
    let mut url = url::Url::parse(input.base_url.trim())
        .map_err(|_| error(StatusCode::BAD_REQUEST, "请填写有效的 API 地址"))?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "API 地址须为不含凭据或查询参数的 HTTP(S) 地址",
        ));
    }
    let path = format!("{}/models", url.path().trim_end_matches('/'));
    url.set_path(&path);
    let key = credential(&server, &input).await?;
    if key.is_none() && !matches!(input.provider.as_str(), "ollama" | "lmstudio" | "llamacpp") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "请先填写 API Key 或选择已配置凭据的账户",
        ));
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建模型列表请求"))?;
    let mut result = Vec::new();
    let mut previous_cursor = None;
    loop {
        let mut request = client.get(url.clone());
        if let Some(key) = &key {
            request = request.bearer_auth(key.expose_secret());
            if input.provider == "anthropic" {
                request = request
                    .header("x-api-key", key.expose_secret())
                    .header("anthropic-version", "2023-06-01");
            }
        }
        let response = request.send().await.map_err(|_| {
            error(
                StatusCode::BAD_GATEWAY,
                "无法读取上游模型列表，请检查地址和网络",
            )
        })?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 => {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "密钥无效或没有读取模型列表的权限",
                ))
            }
            404 | 405 => {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "此服务地址未提供模型列表，请核对地址或使用手动填写",
                ))
            }
            429 => {
                return Err(error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "上游请求限流，请稍后刷新模型列表",
                ))
            }
            _ => return Err(error(StatusCode::BAD_GATEWAY, "上游暂时无法返回模型列表")),
        }
        let data: Value = response
            .json()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "上游模型列表不是有效 JSON"))?;
        result.extend(parse_models(&data)?);
        if data.get("has_more").and_then(Value::as_bool) != Some(true) {
            break;
        }
        let cursor = data
            .get("last_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "上游模型列表分页信息无效"))?;
        if previous_cursor.as_deref() == Some(cursor) {
            return Err(error(StatusCode::BAD_GATEWAY, "上游模型列表分页重复"));
        }
        previous_cursor = Some(cursor.to_owned());
        url.query_pairs_mut()
            .clear()
            .append_pair("after_id", cursor);
    }
    result.sort_by(|left, right| left.id.cmp(&right.id));
    result.dedup_by(|left, right| left.id == right.id);
    Ok(Json(json!({"models": result})))
}

#[cfg(test)]
#[path = "llm_catalog_tests.rs"]
mod tests;
