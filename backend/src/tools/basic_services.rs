use crate::{
    db::{basic_services::BasicServiceProfile, model_pools::ModelPool, Database},
    integrations::{llm::model_pool::ModelRouteEvent, secret::SecretResolver},
    modules::settings::application::basic_services::{
        invoke_profile, invoke_service_pool, service_request, BasicServiceRequest,
    },
    safety::{
        DescriptorError, PermissionId, ResourceDescriptor, ResourceScope, SideEffectKind,
        ToolSecurityDescriptor,
    },
    tools::{RiskLevel, Tool, ToolExecutionContext, ToolResult},
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
pub struct BasicServiceTool {
    db: Database,
    resolver: Arc<SecretResolver>,
    kind: String,
    agent_id: Option<String>,
    conversation_id: String,
    on_route: Option<Arc<dyn Fn(ModelRouteEvent) + Send + Sync>>,
}
impl BasicServiceTool {
    pub fn new(db: Database, resolver: Arc<SecretResolver>, kind: &str) -> Self {
        Self {
            db,
            resolver,
            kind: kind.into(),
            agent_id: None,
            conversation_id: String::new(),
            on_route: None,
        }
    }
    pub fn with_agent_id(mut self, id: String) -> Self {
        self.agent_id = Some(id);
        self
    }
    pub fn with_conversation_id(mut self, id: String) -> Self {
        self.conversation_id = id;
        self
    }
    pub fn with_route_callback(
        mut self,
        callback: Arc<dyn Fn(ModelRouteEvent) + Send + Sync>,
    ) -> Self {
        self.on_route = Some(callback);
        self
    }
    fn selected(&self) -> Result<(Option<ModelPool>, Vec<BasicServiceProfile>), String> {
        let explicit = if let Some(agent) = &self.agent_id {
            let binding = self
                .db
                .resolve_connector_bindings(agent)?
                .into_iter()
                .find(|b| {
                    b.kind == "agent_model" && b.service_kind.as_deref() == Some(self.kind.as_str())
                });
            if let Some(b) = binding {
                Some((
                    b.model_source.ok_or("模型来源缺失")?,
                    b.target_id
                        .strip_prefix("model:")
                        .ok_or("模型引用无效")?
                        .to_owned(),
                ))
            } else {
                None
            }
        } else {
            None
        };
        let (source, id) = if let Some(explicit) = explicit {
            explicit
        } else if let Some(pool) = self
            .db
            .get_model_pool_binding(&format!("service:{}", self.kind))?
        {
            ("pool".into(), pool)
        } else {
            (
                "profile".into(),
                self.db
                    .get_basic_service_default(&self.kind)?
                    .ok_or("请先设置该用途的默认来源")?,
            )
        };
        if source == "profile" {
            let profile = self
                .db
                .get_basic_service_profile(&id)?
                .ok_or("服务档案已删除")?;
            if profile.service_kind != self.kind {
                return Err("服务用途不匹配".into());
            }
            return Ok((None, vec![profile]));
        }
        if source != "pool" {
            return Err("服务来源无效".into());
        }
        let pool = self.db.get_model_pool(&id)?.ok_or("模型池已删除")?;
        if !pool.enabled || pool.service_kind != self.kind {
            return Err("模型池未启用或用途不匹配".into());
        }
        let mut profiles = Vec::new();
        for tier in &pool.tiers {
            for id in &tier.profile_ids {
                let p = self
                    .db
                    .get_basic_service_profile(id)?
                    .ok_or("池候选档案已删除")?;
                if p.service_kind != self.kind {
                    return Err("池候选用途不匹配".into());
                }
                profiles.push(p);
            }
        }
        Ok((Some(pool), profiles))
    }
    fn endpoint(&self, p: &BasicServiceProfile) -> Result<String, String> {
        let a = self
            .db
            .list_provider_accounts()?
            .into_iter()
            .find(|a| a.id == p.provider_account_id)
            .ok_or("厂商账户不存在")?;
        service_request(
            p,
            a.base_url.trim_end_matches('/'),
            BasicServiceRequest {
                prompt: "endpoint".into(),
                image_url: Some("data:image/png;base64,endpoint".into()),
            },
        )
        .map(|(url, _)| url)
    }
}
#[async_trait]
impl Tool for BasicServiceTool {
    fn name(&self) -> &str {
        if self.kind == "vision" {
            "vision_analysis"
        } else {
            "image_generation"
        }
    }
    fn description(&self) -> &str {
        if self.kind == "vision" {
            "使用已绑定或默认的识图档案/模型池分析图片，返回真实描述。"
        } else {
            "使用已绑定或默认的生图档案/模型池生成图片，返回可查看图像。"
        }
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{"prompt":{"type":"string"},"image_url":{"type":"string"}},"required":if self.kind=="vision"{vec!["prompt","image_url"]}else{vec!["prompt"]}})
    }
    fn security_descriptor(
        &self,
        _args: &Value,
    ) -> Result<ToolSecurityDescriptor, DescriptorError> {
        let (_, profiles) = self
            .selected()
            .map_err(DescriptorError::InvalidDescriptor)?;
        let mut urls = Vec::new();
        for p in &profiles {
            let url = self
                .endpoint(p)
                .map_err(DescriptorError::InvalidDescriptor)?;
            if !urls.contains(&url) {
                urls.push(url)
            }
        }
        let d = ToolSecurityDescriptor {
            tool_name: self.name().into(),
            requested_permissions: vec![
                PermissionId::NetworkRequest.in_scope(ResourceScope::NetworkTarget)
            ],
            resources: urls
                .into_iter()
                .map(|url| ResourceDescriptor::Network {
                    url,
                    method: "POST".into(),
                })
                .collect(),
            default_risk: RiskLevel::Medium,
            side_effects: vec![SideEffectKind::NetworkEgress],
        };
        d.validate()?;
        Ok(d)
    }
    async fn execute(&self, _args: Value) -> ToolResult {
        ToolResult::error("该服务必须通过执行网关调用")
    }
    async fn execute_with_context(
        &self,
        args: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        let (pool, profiles) = match self.selected() {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };
        for p in &profiles {
            let endpoint = match self
                .endpoint(p)
                .and_then(|s| url::Url::parse(&s).map_err(|_| "服务地址无效".into()))
            {
                Ok(v) => v,
                Err(e) => return ToolResult::error(e),
            };
            if context
                .authorized_network(
                    endpoint.scheme(),
                    endpoint.host_str().unwrap_or(""),
                    endpoint.port_or_known_default().unwrap_or(443),
                    "POST",
                )
                .is_none()
            {
                return ToolResult::error("服务目标缺少执行网关的授权");
            }
        }
        let request = BasicServiceRequest {
            prompt: args
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            image_url: args
                .get("image_url")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        let events = Mutex::new(Vec::new());
        let callback = |event: ModelRouteEvent| {
            if let Some(on_route) = &self.on_route {
                on_route(event.clone());
            }
            events.lock().unwrap().push(event);
        };
        let result = if let Some(pool) = pool {
            invoke_service_pool(
                &self.db,
                &self.resolver,
                &pool,
                &profiles,
                request,
                &self.conversation_id,
                Some(&callback),
            )
            .await
        } else {
            invoke_profile(&self.db, &self.resolver, &profiles[0], request).await
        };
        match result {
            Ok(result) => {
                let text = if let Some(image) = result.get("image_url").and_then(Value::as_str) {
                    format!("![生成的图像]({image})")
                } else {
                    result
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into()
                };
                ToolResult::success_with_structured(text, Some(result))
            }
            Err(e) => ToolResult::error(e.to_string())
                .with_structured(Some(json!({"model_routes":events.into_inner().unwrap()}))),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_specific_profile_is_resolved_before_default() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.init_basic_services().unwrap();
        db.init_provider_accounts().unwrap();
        db.save_provider_account(&crate::db::model_pools::ProviderAccount {
            id: "account".into(),
            provider_id: "fixture".into(),
            label: "fixture".into(),
            base_url: "https://example.invalid/v1".into(),
            credential_id: Some("llm.fixture".into()),
            profile_ids: vec![],
        })
        .unwrap();
        for id in ["default", "explicit"] {
            db.save_basic_service_profile(
                id,
                crate::db::basic_services::BasicServiceInput {
                    label: id.into(),
                    provider_account_id: "account".into(),
                    service_kind: "image".into(),
                    model: id.into(),
                    adapter: "openai".into(),
                    parameters: json!({}),
                    expected_revision: None,
                },
            )
            .unwrap();
        }
        db.set_basic_service_default("image", "default").unwrap();
        db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('actor','Agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
        db.save_connector_binding(
            "actor",
            &crate::db::connectors::ConnectorBinding {
                id: String::new(),
                kind: "agent_model".into(),
                source_id: "agent:actor".into(),
                target_id: "model:explicit".into(),
                service_kind: Some("image".into()),
                model_source: Some("profile".into()),
            },
        )
        .unwrap();
        let resolver = Arc::new(SecretResolver::new(Arc::new(
            crate::integrations::secret::InMemorySecretStore::new(),
        )));
        let defaults = BasicServiceTool::new(db.clone_connection(), resolver.clone(), "image");
        assert_eq!(defaults.selected().unwrap().1[0].id, "default");
        let bound = BasicServiceTool::new(db, resolver, "image").with_agent_id("actor".into());
        assert_eq!(bound.selected().unwrap().1[0].id, "explicit");
        assert_eq!(
            bound.endpoint(&bound.selected().unwrap().1[0]).unwrap(),
            "https://example.invalid/v1/images/generations"
        );
    }
}
