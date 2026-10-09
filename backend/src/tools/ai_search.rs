use async_trait::async_trait;
use std::sync::Arc;

use super::{RiskLevel, Tool, ToolExecutionContext, ToolResult};
use crate::{
    db::Database,
    integrations::secret::SecretStore,
    modules::cloud::search::{AnySearchProvider, SearchProvider, SearchRequest},
    safety::{
        DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
        ToolSecurityDescriptor,
    },
};

const ANYSEARCH_ENDPOINT: &str = "https://api.anysearch.com/v1/search";

pub struct AiSearchTool {
    db: Database,
    secret_store: Arc<dyn SecretStore>,
}

impl AiSearchTool {
    pub fn new(db: Database, secret_store: Arc<dyn SecretStore>) -> Self {
        Self { db, secret_store }
    }

    fn official_base_url(&self) -> Result<String, DescriptorError> {
        let (_, base, _) = self.db.cloud_search_config().map_err(|_| {
            DescriptorError::InvalidDescriptor("search configuration is unavailable".into())
        })?;
        let parsed = url::Url::parse(&base).map_err(|_| {
            DescriptorError::InvalidDescriptor("search provider URL is invalid".into())
        })?;
        if parsed.scheme() != "https"
            || parsed.host_str() != Some("api.anysearch.com")
            || parsed.port_or_known_default() != Some(443)
        {
            return Err(DescriptorError::InvalidDescriptor(
                "agent search only permits the official AnySearch endpoint".into(),
            ));
        }
        Ok(base.trim_end_matches('/').to_string())
    }
}

#[async_trait]
impl Tool for AiSearchTool {
    fn name(&self) -> &str {
        "ai_search"
    }

    fn description(&self) -> &str {
        "搜索当前公开信息并返回结构化标题、摘要与来源。仅连接官方 AnySearch endpoint。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "query":{"type":"string"},
                "max_results":{"type":"integer","minimum":1,"maximum":10},
                "tag":{"type":"string"},
                "zone":{"type":"string","enum":["cn","intl"]},
                "language":{"type":"string"}
            },
            "required":["query"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    fn security_descriptor(
        &self,
        _args: &serde_json::Value,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        self.official_base_url()?;
        let descriptor = ToolSecurityDescriptor {
            tool_name: self.name().into(),
            requested_permissions: vec![
                PermissionId::NetworkRequest.in_scope(ResourceScope::NetworkTarget)
            ],
            resources: vec![ResourceDescriptor::Network {
                url: ANYSEARCH_ENDPOINT.into(),
                method: "POST".into(),
            }],
            default_risk: RiskLevel::Medium,
            side_effects: vec![SideEffectKind::NetworkEgress],
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("ai_search requires SecurityExecutionGateway trusted context")
    }

    async fn execute_with_context(
        &self,
        args: serde_json::Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        if context
            .authorized_network("https", "api.anysearch.com", 443, "POST")
            .is_none()
        {
            return ToolResult::error("AI 搜索缺少安全网关创建的网络授权证据");
        }
        let query = args
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let max = args
            .get("max_results")
            .and_then(|value| value.as_u64())
            .unwrap_or(8)
            .min(10) as u8;
        let mut request = match SearchRequest::new(query, max) {
            Ok(value) => value,
            Err(error) => return ToolResult::error(error),
        };
        request.tag = args
            .get("tag")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        request.zone = args
            .get("zone")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        request.language = args
            .get("language")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let base = match self.official_base_url() {
            Ok(value) => value,
            Err(error) => return ToolResult::error(error.to_string()),
        };
        let provider = AnySearchProvider::new(base, Arc::clone(&self.secret_store));
        match provider.search(request).await {
            Ok(mut response) => {
                for result in &mut response.results {
                    result.snippet = result
                        .snippet
                        .take()
                        .map(|value| crate::utils::text::truncate_chars(&value, 2_000));
                    result.content = result
                        .content
                        .take()
                        .map(|value| crate::utils::text::truncate_chars(&value, 6_000));
                }
                let count = response.results.len();
                ToolResult::success_with_structured(
                    format!("搜索完成，共返回 {count} 条结果，请保留来源链接。"),
                    serde_json::to_value(response).ok(),
                )
            }
            Err(error) => ToolResult::error(error.to_string()),
        }
    }
}
