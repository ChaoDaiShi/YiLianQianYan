use serde::{Deserialize, Serialize};

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
