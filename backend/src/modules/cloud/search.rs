use serde::{Deserialize, Serialize};
use secrecy::ExposeSecret;
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
