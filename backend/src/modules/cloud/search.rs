use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::integrations::secret::{anysearch_key_ref, SecretStore};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub max_results: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl SearchRequest {
    pub fn new(query: impl Into<String>, max_results: u8) -> Result<Self, &'static str> {
        let query = query.into().trim().to_string();
        if query.is_empty() {
            return Err("query is required");
        }
        if query.chars().count() > 2_000 {
            return Err("query is too long");
        }
        Ok(Self {
            query,
            max_results: max_results.clamp(1, 10),
            tag: None,
            zone: None,
            language: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchFailureKind {
    InvalidRequest,
    Authentication,
    Quota,
    RateLimited,
    Unavailable,
    Provider,
}

pub fn classify_anysearch_status(status: u16) -> SearchFailureKind {
    match status {
        400 | 415 => SearchFailureKind::InvalidRequest,
        401 | 403 => SearchFailureKind::Authentication,
        402 => SearchFailureKind::Quota,
        429 => SearchFailureKind::RateLimited,
        502..=599 => SearchFailureKind::Unavailable,
        _ => SearchFailureKind::Provider,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResponse {
    pub request_id: String,
    pub results: Vec<SearchResult>,
    pub total_results: u64,
    pub search_time_ms: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("search provider request failed: {kind:?}")]
pub struct SearchError {
    pub kind: SearchFailureKind,
    pub status: Option<u16>,
}

#[async_trait::async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, request: SearchRequest) -> Result<SearchResponse, SearchError>;
}

pub struct AnySearchProvider {
    base_url: String,
    secret_store: Arc<dyn SecretStore>,
    client: reqwest::Client,
}

impl AnySearchProvider {
    pub fn new(base_url: impl Into<String>, secret_store: Arc<dyn SecretStore>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("valid AnySearch HTTP client");
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            secret_store,
            client,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnySearchEnvelope {
    code: i64,
    request_id: String,
    data: Option<AnySearchData>,
}

#[derive(Debug, Deserialize)]
struct AnySearchData {
    results: Vec<SearchResult>,
    metadata: AnySearchMetadata,
}

#[derive(Debug, Deserialize)]
struct AnySearchMetadata {
    total_results: u64,
    search_time_ms: u64,
}

#[async_trait::async_trait]
impl SearchProvider for AnySearchProvider {
    async fn search(&self, request: SearchRequest) -> Result<SearchResponse, SearchError> {
        let mut builder = self
            .client
            .post(format!("{}/v1/search", self.base_url))
            .json(&request);
        if let Ok(Some(key)) = self.secret_store.get(&anysearch_key_ref()).await {
            builder = builder.bearer_auth(key.expose_secret());
        }
        let response = builder.send().await.map_err(|_| SearchError {
            kind: SearchFailureKind::Unavailable,
            status: None,
        })?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(SearchError {
                kind: classify_anysearch_status(status),
                status: Some(status),
            });
        }
        let envelope: AnySearchEnvelope = response.json().await.map_err(|_| SearchError {
            kind: SearchFailureKind::Provider,
            status: Some(status),
        })?;
        if envelope.code != 0 {
            return Err(SearchError {
                kind: SearchFailureKind::Provider,
                status: Some(status),
            });
        }
        let data = envelope.data.ok_or(SearchError {
            kind: SearchFailureKind::Provider,
            status: Some(status),
        })?;
        Ok(SearchResponse {
            request_id: envelope.request_id,
            results: data.results,
            total_results: data.metadata.total_results,
            search_time_ms: data.metadata.search_time_ms,
        })
    }
}
pub fn configured_search_endpoint(
    provider: &str,
    base: &str,
) -> Result<(String, &'static str), SearchError> {
    let (path, method) = match provider {
        "anysearch" => ("/v1/search", "POST"),
        "tavily" => ("/search", "POST"),
        "brave" => ("/res/v1/web/search", "GET"),
        "bocha" => ("/v1/web-search", "POST"),
        _ => {
            return Err(SearchError {
                kind: SearchFailureKind::InvalidRequest,
                status: None,
            })
        }
    };
    Ok((format!("{}{path}", base.trim_end_matches('/')), method))
}
pub async fn search_configured(
    provider: &str,
    base: String,
    store: Arc<dyn SecretStore>,
    secret_ref: Option<crate::integrations::secret::SecretRef>,
    request: SearchRequest,
) -> Result<SearchResponse, SearchError> {
    if provider == "anysearch" {
        return AnySearchProvider::new(base, store).search(request).await;
    }
    let fail = || SearchError {
        kind: SearchFailureKind::Provider,
        status: None,
    };
    let key = store
        .get(&secret_ref.ok_or(SearchError {
            kind: SearchFailureKind::Authentication,
            status: None,
        })?)
        .await
        .map_err(|_| fail())?
        .ok_or(SearchError {
            kind: SearchFailureKind::Authentication,
            status: None,
        })?;
    let (endpoint, method) = configured_search_endpoint(provider, &base)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| fail())?;
    let start = std::time::Instant::now();
    let builder = if method == "GET" {
        client
            .get(endpoint)
            .header("X-Subscription-Token", key.expose_secret())
            .query(&[
                ("q", request.query.clone()),
                ("count", request.max_results.to_string()),
            ])
    } else {
        let body = if provider == "tavily" {
            serde_json::json!({"query":request.query,"max_results":request.max_results,"search_depth":"basic"})
        } else {
            serde_json::json!({"query":request.query,"count":request.max_results,"summary":true})
        };
        client
            .post(endpoint)
            .bearer_auth(key.expose_secret())
            .json(&body)
    };
    let response = builder.send().await.map_err(|_| SearchError {
        kind: SearchFailureKind::Unavailable,
        status: None,
    })?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(SearchError {
            kind: classify_anysearch_status(status),
            status: Some(status),
        });
    }
    let value: serde_json::Value = response.json().await.map_err(|_| fail())?;
    let entries = match provider {
        "tavily" => value.get("results"),
        "brave" => value.pointer("/web/results"),
        "bocha" => value.pointer("/data/webPages/value"),
        _ => None,
    }
    .and_then(serde_json::Value::as_array)
    .ok_or_else(fail)?;
    let mut results = Vec::new();
    for entry in entries.iter().take(request.max_results as usize) {
        let title = entry
            .get(if provider == "bocha" { "name" } else { "title" })
            .and_then(serde_json::Value::as_str)
            .ok_or_else(fail)?;
        let url = entry
            .get("url")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(fail)?;
        let snippet = entry
            .get(match provider {
                "tavily" => "content",
                "brave" => "description",
                _ => "summary",
            })
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        results.push(SearchResult {
            title: title.into(),
            url: url.into(),
            snippet,
            content: None,
        });
    }
    Ok(SearchResponse {
        request_id: value
            .get("request_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        total_results: results.len() as u64,
        results,
        search_time_ms: start.elapsed().as_millis() as u64,
    })
}
#[cfg(test)]
mod configured_tests {
    use super::*;
    use crate::integrations::secret::{InMemorySecretStore, SecretRef};
    use axum::{
        routing::{get, post},
        Json, Router,
    };
    use serde_json::json;
    #[tokio::test]
    async fn configured_search_http_adapters() {
        for provider in ["tavily", "brave", "bocha"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let app=Router::new().route("/search",post(|Json(v):Json<serde_json::Value>|async move{assert_eq!(v["query"],"fixture");Json(json!({"request_id":"tavily-id","results":[{"title":"title","url":"https://example.invalid","content":"snippet"}]}))})).route("/res/v1/web/search",get(||async{Json(json!({"web":{"results":[{"title":"title","url":"https://example.invalid","description":"snippet"}]}}))})).route("/v1/web-search",post(|Json(v):Json<serde_json::Value>|async move{assert_eq!(v["summary"],true);Json(json!({"code":200,"data":{"webPages":{"value":[{"name":"title","url":"https://example.invalid","summary":"snippet"}]}}}))}));
            let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let store = Arc::new(InMemorySecretStore::new());
            let key = SecretRef::new("search.fixture");
            store
                .put(&key, secrecy::SecretString::from("fixture"))
                .await
                .unwrap();
            let result = search_configured(
                provider,
                base,
                store,
                Some(key),
                SearchRequest::new("fixture", 3).unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(result.results[0].title, "title");
            assert_eq!(result.results[0].snippet.as_deref(), Some("snippet"));
            task.abort();
        }
    }
}
