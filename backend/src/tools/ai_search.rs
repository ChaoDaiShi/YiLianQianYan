use async_trait::async_trait;
use std::sync::Arc;

use super::{RiskLevel, Tool, ToolExecutionContext, ToolResult};
use crate::{
    db::Database,
    integrations::secret::SecretStore,
    modules::cloud::search::{SearchRequest, SearchResponse},
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
                | ("deepseek", "https://api.deepseek.com/v1/tools/search")
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
            Ok(response) => search_tool_result(response),
            Err(error) => ToolResult::error(error.to_string()),
        }
    }
}

fn search_tool_result(mut response: SearchResponse) -> ToolResult {
    use crate::utils::text::truncate_chars;

    let count = response.results.len();
    let mut output =
        format!("搜索完成，共返回 {count} 条结果。以下公开资料供核对，请保留来源链接。");
    for result in &mut response.results {
        result.snippet = result
            .snippet
            .take()
            .map(|value| truncate_chars(&value, 2_000));
        result.content = result
            .content
            .take()
            .map(|value| truncate_chars(&value, 6_000));
        output.push_str(&format!(
            "\n\n{}\n{}",
            truncate_chars(&result.title, 120),
            result.url
        ));
        if let Some(excerpt) = result
            .snippet
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .or(result.content.as_deref())
        {
            output.push('\n');
            output.push_str(&truncate_chars(excerpt, 440));
        }
    }
    ToolResult::success_with_structured(output, serde_json::to_value(response).ok())
}

pub(crate) fn display_result(name: &str, result: &ToolResult) -> String {
    if name != "ai_search" || !result.ok {
        return result.content.clone();
    }
    let Some(response) = result
        .structured
        .as_ref()
        .and_then(|value| serde_json::from_value::<SearchResponse>(value.clone()).ok())
    else {
        return result.content.clone();
    };
    let sources = response.results.into_iter().filter_map(|item| {
        let url = url::Url::parse(&item.url).ok()?;
        if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let domain = url.host_str()?;
        Some(serde_json::json!({
            "url": url.as_str(), "domain": domain,
            "title": crate::utils::text::truncate_chars(&item.title, 120),
            "excerpt": item.snippet.or(item.content).map(|text| crate::utils::text::truncate_chars(&text, 420)),
        }))
    }).take(10).collect::<Vec<_>>();
    serde_json::json!({"kind":"web_search_sources", "text":result.content, "sources":sources})
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::cloud::search::SearchResult;

    #[test]
    fn search_tool_result_gives_the_model_titles_links_and_excerpts() {
        let result = search_tool_result(SearchResponse {
            request_id: "fixture".into(),
            total_results: 2,
            search_time_ms: 15,
            results: vec![
                SearchResult {
                    title: "文档一".into(),
                    url: "https://example.org/first".into(),
                    snippet: Some("第一份材料摘要".into()),
                    content: None,
                },
                SearchResult {
                    title: "文档二".into(),
                    url: "https://example.org/second".into(),
                    snippet: None,
                    content: Some("第二份材料正文".repeat(1000)),
                },
            ],
        });
        let model_text = crate::agent::engine::summarize_tool_result(&result.content);
        for expected in [
            "文档一",
            "https://example.org/first",
            "第一份材料摘要",
            "文档二",
            "https://example.org/second",
            "第二份材料正文",
        ] {
            assert!(model_text.contains(expected), "Missing {expected}");
        }
        assert!(model_text.chars().count() < 2000);
        assert_eq!(
            result.structured.unwrap()["results"][1]["url"],
            "https://example.org/second"
        );
    }
}
