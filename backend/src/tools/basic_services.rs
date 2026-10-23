use crate::{db::conversation_outputs::ConversationOutput, shared::resource::ResourceService};
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
use base64::Engine;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

static OUTPUT_INGEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub struct BasicServiceTool {
    db: Database,
    resolver: Arc<SecretResolver>,
    kind: String,
    agent_id: Option<String>,
    conversation_id: String,
    resource_service: Option<ResourceService>,
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
            resource_service: None,
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
    pub fn with_resource_service(mut self, service: ResourceService) -> Self {
        self.resource_service = Some(service);
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

    fn output(&self, id: &str) -> Result<ConversationOutput, String> {
        self.db
            .conversation_output(&self.conversation_id, id)?
            .ok_or("当前对话中不存在该图像成果".into())
    }

    fn vision_image(&self, args: &Value) -> Result<Option<String>, String> {
        let resource_id = args.get("resource_id").and_then(Value::as_str);
        let image_url = args.get("image_url").and_then(Value::as_str);
        if resource_id.is_some() && image_url.is_some() {
            return Err("resource_id与image_url只能提供一个".into());
        }
        let Some(id) = resource_id else {
            return Ok(image_url.map(str::to_owned));
        };
        let bound = self
            .db
            .resource_bindings(&crate::db::ResourceTarget::Conversation {
                conversation_id: self.conversation_id.clone(),
            })?
            .iter()
            .any(|binding| binding.resource_id == id);
        let generated = self
            .db
            .conversation_outputs(&self.conversation_id)?
            .iter()
            .any(|output| output.status == "ready" && output.resource_id.as_deref() == Some(id));
        if !bound && !generated {
            return Err("识图资源未绑定到当前对话".into());
        }
        let service = self.resource_service.as_ref().ok_or("识图资源服务未接入")?;
        let resource = service
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("识图资源不存在")?;
        if !matches!(resource.mime_type.as_str(), "image/png" | "image/jpeg") {
            return Err("识图资源必须为PNG或JPEG图片".into());
        }
        let bytes = service
            .read_bytes(id)
            .map_err(|_| "识图资源文件缺失或完整性校验失败")?;
        let preview =
            crate::modules::resource::extract_preview(&resource.name, &resource.mime_type, &bytes);
        if preview.status != "ready" {
            return Err("识图资源不是有效图片".into());
        }
        Ok(Some(format!(
            "data:{};base64,{}",
            resource.mime_type,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )))
    }

    async fn persist_image(
        &self,
        output: &ConversationOutput,
        bytes: Vec<u8>,
    ) -> Result<ConversationOutput, String> {
        let _guard = OUTPUT_INGEST.lock().await;
        let current = self.output(&output.id)?;
        if current.status == "ready" {
            return Ok(current);
        }
        let (mime, extension) =
            match image::guess_format(&bytes).map_err(|_| "返回内容不是可识别图片")? {
                image::ImageFormat::Png => ("image/png", "png"),
                image::ImageFormat::Jpeg => ("image/jpeg", "jpg"),
                _ => return Err("当前成果预览支持PNG与JPEG，服务返回了其他格式".into()),
            };
        let name = format!("generated-{}.{}", output.id, extension);
        let preview = crate::modules::resource::extract_preview(&name, mime, &bytes);
        if preview.status != "ready" {
            return Err(preview.error.unwrap_or_else(|| "生成图片解码失败".into()));
        }
        let service = self.resource_service.as_ref().ok_or("图像资源服务未接入")?;
        let resource = service.ingest(&name, mime, &bytes, json!({
            "source":"conversation-output","conversation_id":self.conversation_id,"output_id":output.id,
            "source_tool":"image_generation","profile_id":output.profile_id,"preview":preview,
        })).map_err(|e| e.to_string())?;
        // Read-back integrity is provided by the shared resource service.
        service
            .read_bytes(&resource.id)
            .map_err(|_| "生成图片保存后的完整性校验失败")?;
        self.db
            .complete_conversation_output(&self.conversation_id, &output.id, &resource.id)
    }

    async fn download_image(
        &self,
        output: &ConversationOutput,
        context: &ToolExecutionContext,
    ) -> Result<ConversationOutput, String> {
        if output.status == "ready" {
            return Ok(output.clone());
        }
        let raw_url = output
            .source_url
            .as_deref()
            .ok_or("该成果没有可重新下载的来源，请重新生成")?;
        let url = url::Url::parse(raw_url).map_err(|_| "图像下载地址无效")?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("图像下载地址无效".into());
        }
        let host = url.host_str().ok_or("图像下载地址缺少主机")?;
        let port = url.port_or_known_default().ok_or("图像下载地址端口无效")?;
        let Some(crate::safety::grant::AuthorizedResource::Network { zone, .. }) =
            context.authorized_network(url.scheme(), host, port, "GET")
        else {
            return Err("图像下载目标尚未获得执行网关GET授权".into());
        };
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| "图像下载地址解析失败")?
            .collect::<Vec<_>>();
        if addresses.is_empty()
            || addresses.iter().any(|address| {
                use crate::safety::grant::{classify_ip, NetworkZone};
                let actual = classify_ip(address.ip());
                match zone {
                    NetworkZone::Public => actual != NetworkZone::Public,
                    NetworkZone::Loopback => actual != NetworkZone::Loopback,
                    NetworkZone::Private => actual == NetworkZone::Public,
                }
            })
        {
            return Err("图像下载目标地址类别与网关授权不一致".into());
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|_| "无法创建图像下载客户端")?;
        let mut response = client.get(url).send().await.map_err(|_| "图像下载失败")?;
        if !response.status().is_success() {
            return Err(format!(
                "图像下载未成功（HTTP {}）；未跟随未授权重定向",
                response.status().as_u16()
            ));
        }
        let max = crate::modules::resource::MAX_INPUT_BYTES;
        if response
            .content_length()
            .is_some_and(|size| size > max as u64)
        {
            return Err("图像超过25MB资源限制".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "图像下载未完成")? {
            if bytes.len() + chunk.len() > max {
                return Err("图像超过25MB资源限制".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        self.persist_image(output, bytes).await
    }

    fn output_result(output: ConversationOutput) -> ToolResult {
        if output.status == "ready" {
            let url = output.download_url.clone().unwrap_or_default();
            ToolResult::success_with_structured("图片已保存到当前对话成果，可点击聊天顶部“成果”预览并下载。不要将需会话鉴权的资源地址渲染为普通Markdown图片。",
                Some(json!({"service_kind":"image","output_id":output.id,"resource_id":output.resource_id,"image_url":url,"output":output})))
        } else if output.can_download {
            ToolResult::success_with_structured(format!("图像服务已返回图片地址，图片尚未下载到本地。请调用 image_generation {{\"output_id\":\"{}\"}}，经执行网关授权后下载并登记成果。", output.id),
                Some(json!({"service_kind":"image","output_id":output.id,"resource_id":null,"output":output,"next_action":"download_output"})))
        } else {
            ToolResult::error(output.error.clone().unwrap_or_else(||"图片尚未保存为可用资源，请重新生成".into()))
                .with_structured(Some(json!({"service_kind":"image","output_id":output.id,"resource_id":null,"output":output})))
        }
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
            "分析当前对话已绑定resource_id的真实图片，或提供image_url；返回真实描述。"
        } else {
            "使用prompt生成图片并保存当前对话成果。若返回awaiting_download，必须再次仅传output_id，经网关授权下载真实图片；不应重复生成。"
        }
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }
    fn parameters(&self) -> Value {
        if self.kind == "vision" {
            json!({"type":"object","properties":{"prompt":{"type":"string"},"image_url":{"type":"string"},"resource_id":{"type":"string"}},"required":["prompt"],"oneOf":[{"required":["image_url"]},{"required":["resource_id"]}],"additionalProperties":false})
        } else {
            json!({"type":"object","properties":{"prompt":{"type":"string"},"output_id":{"type":"string"}},"oneOf":[{"required":["prompt"]},{"required":["output_id"]}],"additionalProperties":false})
        }
    }
    fn security_descriptor(&self, args: &Value) -> Result<ToolSecurityDescriptor, DescriptorError> {
        if self.kind == "image" {
            if let Some(id) = args.get("output_id").and_then(Value::as_str) {
                let output = self
                    .output(id)
                    .map_err(DescriptorError::InvalidDescriptor)?;
                let url = output.source_url.ok_or_else(|| {
                    DescriptorError::InvalidDescriptor("该成果没有远端下载地址".into())
                })?;
                let descriptor = ToolSecurityDescriptor {
                    tool_name: self.name().into(),
                    requested_permissions: vec![
                        PermissionId::NetworkRequest.in_scope(ResourceScope::NetworkTarget)
                    ],
                    resources: vec![ResourceDescriptor::Network {
                        url,
                        method: "GET".into(),
                    }],
                    default_risk: RiskLevel::Medium,
                    side_effects: vec![SideEffectKind::NetworkEgress],
                };
                descriptor.validate()?;
                return Ok(descriptor);
            }
        }
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
        if self.kind == "image" {
            if self.conversation_id.is_empty() || self.resource_service.is_none() {
                return ToolResult::error("生图需要当前对话与资源服务上下文");
            }
            if self.db.get_conversation(&self.conversation_id).is_err() {
                return ToolResult::error("生图所属对话不存在");
            }
            if let Some(id) = args.get("output_id").and_then(Value::as_str) {
                if args.get("prompt").is_some() {
                    return ToolResult::error("prompt与output_id只能提供一个");
                }
                let output = match self.output(id) {
                    Ok(output) => output,
                    Err(error) => return ToolResult::error(error),
                };
                return match self.download_image(&output, context).await {
                    Ok(output) => Self::output_result(output),
                    Err(error) => {
                        if let Err(persistence) =
                            self.db
                                .fail_conversation_output(&self.conversation_id, id, &error)
                        {
                            return ToolResult::error(persistence);
                        }
                        ToolResult::error(error).with_structured(Some(
                            json!({"output_id":id,"status":"download_failed"}),
                        ))
                    }
                };
            }
            match self
                .db
                .conversation_output_for_call(&self.conversation_id, &context.tool_call_id)
            {
                Ok(Some(output)) => return Self::output_result(output),
                Ok(None) => {}
                Err(error) => return ToolResult::error(error),
            }
        }
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
        let image_url = if self.kind == "vision" {
            match self.vision_image(&args) {
                Ok(image) => image,
                Err(error) => return ToolResult::error(error),
            }
        } else {
            None
        };
        let request = BasicServiceRequest {
            prompt: args
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            image_url,
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
                if self.kind == "image" {
                    let Some(image) = result.get("image_url").and_then(Value::as_str) else {
                        return ToolResult::error("生图服务未返回图片内容");
                    };
                    let is_data = image.starts_with("data:");
                    if !is_data
                        && url::Url::parse(image).ok().is_none_or(|url| {
                            !matches!(url.scheme(), "http" | "https")
                                || url.host_str().is_none()
                                || !url.username().is_empty()
                                || url.password().is_some()
                        })
                    {
                        return ToolResult::error("生图服务返回无效图片地址");
                    }
                    let output = match self.db.create_conversation_image_output(
                        &self.conversation_id,
                        &context.tool_call_id,
                        &crate::utils::text::truncate_chars(
                            args.get("prompt")
                                .and_then(Value::as_str)
                                .unwrap_or("生成图片"),
                            120,
                        ),
                        result.get("profile_id").and_then(Value::as_str),
                        if is_data { None } else { Some(image) },
                    ) {
                        Ok(output) => output,
                        Err(error) => return ToolResult::error(error),
                    };
                    if !is_data {
                        return Self::output_result(output);
                    }
                    let bytes = decode_image_data_url(image);
                    let saved = match bytes {
                        Ok(bytes) => self.persist_image(&output, bytes).await,
                        Err(error) => Err(error),
                    };
                    return match saved {
                        Ok(output) => Self::output_result(output),
                        Err(error) => {
                            if let Err(persistence) = self.db.fail_conversation_output(
                                &self.conversation_id,
                                &output.id,
                                &error,
                            ) {
                                return ToolResult::error(persistence);
                            }
                            ToolResult::error(error).with_structured(Some(
                                json!({"output_id":output.id,"status":"download_failed"}),
                            ))
                        }
                    };
                }
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
fn decode_image_data_url(value: &str) -> Result<Vec<u8>, String> {
    let (header, encoded) = value.split_once(',').ok_or("图像data URL格式无效")?;
    if !matches!(header, "data:image/png;base64" | "data:image/jpeg;base64") {
        return Err("生成图像必须为PNG或JPEG base64内容".into());
    }
    if encoded.len() > crate::modules::resource::MAX_INPUT_BYTES.div_ceil(3) * 4 {
        return Err("生成图像超过25MB资源限制".into());
    }
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "图像base64内容无效".into())
}
#[cfg(test)]
mod tests;
