use async_trait::async_trait;
use std::sync::Arc;

use super::{RiskLevel, Tool, ToolExecutionContext, ToolResult};
use crate::{
    db::Database,
    integrations::secret::SecretStore,
    modules::cloud::search::SearchRequest,
    safety::{
        DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
        ToolSecurityDescriptor,
    },
};

pub struct AiSearchTool {
    db: Database,
    secret_store: Arc<dyn SecretStore>,
}

impl AiSearchTool {
    pub fn new(db: Database, secret_store: Arc<dyn SecretStore>) -> Self {
        Self { db, secret_store }
    }

    fn endpoint(&self) -> Result<(String, String, &'static str), DescriptorError> {
        let (provider, base, _) = self.db.cloud_search_config().map_err(|_| {
            DescriptorError::InvalidDescriptor("search configuration unavailable".into())
        })?;
        let (url, method) =
            crate::modules::cloud::search::configured_search_endpoint(&provider, &base)
                .map_err(|e| DescriptorError::InvalidDescriptor(e.to_string()))?;
        let official = matches!(
            (provider.as_str(), url.as_str()),
            ("anysearch", "https://api.anysearch.com/v1/search")
                | ("tavily", "https://api.tavily.com/search")
                | ("brave", "https://api.search.brave.com/res/v1/web/search")
                | ("bocha", "https://api.bochaai.com/v1/web-search")
                | ("bocha", "https://api.bocha.cn/v1/web-search")
        );
        if !official {
            return Err(DescriptorError::InvalidDescriptor(
                "agent search requires selected official provider endpoint".into(),
            ));
        }
        Ok((provider, url, method))
    }
}

#[async_trait]
impl Tool for AiSearchTool {
    fn name(&self) -> &str {
        "ai_search"
    }

    fn description(&self) -> &str {
        "搜索当前公开信息并返回结构化标题、摘要与来源。连接已配置的官方搜索服务。"
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
        let (_, endpoint, method) = self.endpoint()?;
        let descriptor = ToolSecurityDescriptor {
            tool_name: self.name().into(),
            requested_permissions: vec![
                PermissionId::NetworkRequest.in_scope(ResourceScope::NetworkTarget)
            ],
            resources: vec![ResourceDescriptor::Network {
                url: endpoint,
                method: method.into(),
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
        let (_, endpoint, method) = match self.endpoint() {
            Ok(v) => v,
            Err(e) => return ToolResult::error(e.to_string()),
        };
        let parsed = match url::Url::parse(&endpoint) {
            Ok(v) => v,
            Err(_) => return ToolResult::error("搜索地址无效"),
        };
        if context
            .authorized_network(
                parsed.scheme(),
                parsed.host_str().unwrap_or(""),
                parsed.port_or_known_default().unwrap_or(443),
                method,
            )
            .is_none()
        {
            return ToolResult::error("AI 搜索缺少执行网关的网络授权");
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
        let (provider, base, secret_ref) = match self.db.cloud_search_config() {
            Ok(v) => v,
            Err(_) => return ToolResult::error("搜索配置不可用"),
        };
        match crate::modules::cloud::search::search_configured(
            &provider,
            base,
            Arc::clone(&self.secret_store),
            secret_ref,
            request,
        )
        .await
        {
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
