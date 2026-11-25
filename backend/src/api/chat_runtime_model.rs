use super::{ordinary_conversation, ChatRuntime};
use crate::{
    agent::engine::AgentEvent,
    app::state::AppServer,
    db::{
        workspace_service_settings::{ChatServiceSelection, WorkspaceServiceKind},
        LlmModelRow,
    },
    integrations::llm::client::LlmClient,
    modules::harness_profiles as harness,
    workspace::WorkspaceId,
};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

pub enum ChatModelSource {
    Fixed {
        config: crate::config::types::ModelConfig,
        row: Option<LlmModelRow>,
    },
    Pool(crate::db::model_pools::ModelPool),
}

impl ChatModelSource {
    /// Build a text client for a caller that already resolved its trusted scope.
    pub(crate) fn into_client(
        self,
        db: &crate::db::Database,
        default: &crate::config::types::ModelConfig,
        resolver: Arc<crate::integrations::secret::SecretResolver>,
        correlation: &str,
        usage_kind: &str,
        timeout_limit_ms: Option<u64>,
    ) -> Result<LlmClient, String> {
        let bounded = |mut config: crate::config::types::ModelConfig| {
            if let Some(limit) = timeout_limit_ms {
                config.invoke_timeout_ms = config.invoke_timeout_ms.min(limit);
            }
            config
        };
        match self {
            Self::Fixed { config, row } => {
                if row.as_ref().is_some_and(|row| row.api_format != "openai") {
                    return Err("绑定模型不支持对话协议".into());
                }
                let recorder = row.map(|row| {
                    Arc::new(crate::integrations::llm::usage::DatabaseUsageRecorder::new(
                        db.clone_connection(),
                        row.id,
                        row.provider,
                        row.model,
                        usage_kind,
                    ))
                        as Arc<dyn crate::integrations::llm::client::UsageRecorder>
                });
                Ok(
                    LlmClient::new_with_usage_recorder(&bounded(config), resolver, recorder)
                        .with_profile_permissions(db.clone_connection()),
                )
            }
            Self::Pool(pool) => {
                if pool.service_kind != "chat" {
                    return Err("模型池用途与文本调用不匹配".into());
                }
                let runtime = crate::integrations::llm::model_pool::load_model_pool_runtime(
                    db,
                    resolver.clone(),
                    &pool.id,
                    correlation,
                    Arc::new(|_| {}),
                )?;
                Ok(LlmClient::new(&bounded(default.clone()), resolver).with_model_pool(runtime))
            }
        }
    }
}

/// Resolve only model ownership/source. This never constructs tools or calls providers.
pub fn resolve_model_source(
    server: &AppServer,
    conversation: Option<&str>,
) -> Result<ChatModelSource, String> {
    let mut agent_id = conversation
        .map(|id| server.db.connector_agent_for_conversation(id))
        .transpose()?
        .flatten();
    if agent_id.is_none()
        && conversation
            .map(|id| ordinary_conversation(server, id))
            .transpose()?
            .unwrap_or(true)
    {
        agent_id = harness::active(&server.db)?.and_then(|selection| selection.profile.agent_id);
    }
    let workspace = conversation
        .map(|id| server.db.conversation_workspace(id))
        .transpose()?
        .flatten();
    resolve_scoped_model_source(
        &server.db,
        &server.config.read().model,
        agent_id.as_deref(),
        workspace.as_deref(),
    )
}

pub(crate) fn resolve_agent_model_source(
    db: &crate::db::Database,
    default: &crate::config::types::ModelConfig,
    agent_id: Option<&str>,
) -> Result<ChatModelSource, String> {
    resolve_scoped_model_source(db, default, agent_id, None)
}

pub(crate) fn resolve_scoped_model_source(
    db: &crate::db::Database,
    default: &crate::config::types::ModelConfig,
    agent_id: Option<&str>,
    workspace: Option<&str>,
) -> Result<ChatModelSource, String> {
    let mut agent_model = None;
    if let Some(id) = agent_id.filter(|id| *id != "default") {
        let agent = db
            .get_agent_definition(
                &crate::modules::task::AgentId::new(id).map_err(|e| e.to_string())?,
            )?
            .ok_or("绑定智能体已删除")?;
        if !agent.enabled {
            return Err("绑定智能体已停用".into());
        }
        agent_model = agent.model;
        if let Some(binding) = db
            .resolve_connector_bindings(id)?
            .into_iter()
            .find(|b| b.kind == "agent_model" && b.service_kind.as_deref() == Some("chat"))
        {
            let id = binding
                .target_id
                .strip_prefix("model:")
                .ok_or("绑定模型引用无效")?;
            return match binding.model_source.as_deref() {
                Some("profile") => {
                    let row = db.get_llm_model(id)?.ok_or("绑定模型档案已删除")?;
                    Ok(ChatModelSource::Fixed {
                        config: row.to_model_config(),
                        row: Some(row),
                    })
                }
                Some("pool") => Ok(ChatModelSource::Pool(
                    db.get_model_pool(id)?.ok_or("绑定模型池已删除")?,
                )),
                _ => Err("绑定模型来源无效".into()),
            };
        }
    }
    if let Some(model) = &agent_model {
        if let Some(row) = db.get_llm_model(model)? {
            return Ok(ChatModelSource::Fixed {
                config: row.to_model_config(),
                row: Some(row),
            });
        }
    }
    if let Some(workspace) = workspace {
        let workspace = WorkspaceId::new(workspace).map_err(|error| error.to_string())?;
        let setting = db
            .get_workspace_service_settings(&workspace, WorkspaceServiceKind::Chat)
            .map_err(|error| error.to_string())?;
        if let Some(reference) = setting.and_then(|setting| setting.override_value) {
            // Legacy agent model names reuse the global fixed provider and credentials.
            if agent_model.is_some() {
                return global_fixed_source(db, default, agent_model);
            }
            let selection = db
                .resolve_chat_service_reference(&reference)
                .map_err(|error| error.to_string())?;
            return Ok(match selection {
                ChatServiceSelection::Profile(row) => ChatModelSource::Fixed {
                    config: row.to_model_config(),
                    row: Some(row),
                },
                ChatServiceSelection::Pool(pool) => ChatModelSource::Pool(pool),
            });
        }
    }
    if let Some(id) = db.get_model_pool_binding("default")? {
        return Ok(ChatModelSource::Pool(
            db.get_model_pool(&id)?.ok_or("默认模型池已删除")?,
        ));
    }
    global_fixed_source(db, default, agent_model)
}

fn global_fixed_source(
    db: &crate::db::Database,
    default: &crate::config::types::ModelConfig,
    agent_model: Option<String>,
) -> Result<ChatModelSource, String> {
    let row = db.get_active_llm_model()?;
    let mut config = row
        .as_ref()
        .map(LlmModelRow::to_model_config)
        .unwrap_or_else(|| default.clone());
    if let Some(model) = agent_model {
        config.name = model;
        return Ok(ChatModelSource::Fixed { config, row: None });
    }
    Ok(ChatModelSource::Fixed { config, row })
}
#[derive(serde::Serialize)]
pub struct ChatReadiness {
    pub configured: bool,
    pub available: bool,
    pub provider: String,
    pub model: String,
    pub thinking_modes: Vec<crate::integrations::llm::thinking::ThinkingMode>,
}
fn structurally_ready(config: &crate::config::types::ModelConfig) -> bool {
    !config.provider.trim().is_empty()
        && !config.name.trim().is_empty()
        && crate::modules::settings::domain::policy::valid_provider_url(&config.base_url)
        && config.invoke_timeout_ms > 0
}
async fn credential_ready(server: &AppServer, config: &crate::config::types::ModelConfig) -> bool {
    use secrecy::ExposeSecret;
    if matches!(config.provider.as_str(), "ollama" | "lmstudio" | "llamacpp") {
        return true;
    }
    server
        .secret_resolver
        .resolve_api_key(config)
        .await
        .ok()
        .flatten()
        .is_some_and(|key| !key.expose_secret().trim().is_empty())
}
pub async fn readiness(server: &AppServer, conversation: Option<&str>) -> ChatReadiness {
    use crate::integrations::llm::thinking::{available_modes, ThinkingMode};
    let external_allowed = crate::modules::onboarding::allowed(&server.db, "external_model");
    match resolve_model_source(server, conversation) {
        Ok(ChatModelSource::Fixed { config, row }) => {
            let configured = credential_ready(server, &config).await;
            let available = configured
                && (external_allowed || config.uses_local_chat())
                && structurally_ready(&config)
                && row.as_ref().is_none_or(|row| row.api_format == "openai");
            ChatReadiness {
                configured,
                available,
                thinking_modes: available_modes(&config).to_vec(),
                provider: config.provider,
                model: config.name,
            }
        }
        Ok(ChatModelSource::Pool(pool)) => {
            let mut configured = false;
            let mut any_ready = false;
            let mut all_valid = true;
            let mut count = 0;
            let mut thinking_modes = None::<Vec<ThinkingMode>>;
            for id in pool.tiers.iter().flat_map(|tier| tier.profile_ids.iter()) {
                count += 1;
                match server.db.get_llm_model(id) {
                    Ok(Some(row)) => {
                        let config = row.to_model_config();
                        let candidate_modes = available_modes(&config);
                        match &mut thinking_modes {
                            Some(modes) => modes.retain(|mode| candidate_modes.contains(mode)),
                            None => thinking_modes = Some(candidate_modes.to_vec()),
                        }
                        let key = credential_ready(server, &config).await;
                        configured |= key;
                        let valid = structurally_ready(&config)
                            && row.api_format == "openai"
                            && (external_allowed || config.uses_local_chat());
                        all_valid &= valid;
                        any_ready |= key && valid;
                    }
                    _ => all_valid = false,
                }
            }
            ChatReadiness {
                configured,
                available: pool.enabled
                    && pool.service_kind == "chat"
                    && count > 0
                    && pool.attempt_timeout_ms > 0
                    && pool.total_timeout_ms >= pool.attempt_timeout_ms
                    && all_valid
                    && any_ready,
                provider: "model_pool".into(),
                model: pool.label,
                thinking_modes: thinking_modes.unwrap_or_else(|| vec![ThinkingMode::Standard]),
            }
        }
        Err(_) => ChatReadiness {
            configured: false,
            available: false,
            provider: String::new(),
            model: String::new(),
            thinking_modes: vec![ThinkingMode::Standard],
        },
    }
}
pub fn client(
    server: &AppServer,
    runtime: &ChatRuntime,
    conversation: &str,
    tx: &Sender<AgentEvent>,
) -> Result<LlmClient, String> {
    if !crate::modules::onboarding::allowed(&server.db, "external_model") {
        let local = match runtime.model_pool_id.as_deref() {
            Some(id) => {
                let pool = server.db.get_model_pool(id)?.ok_or("所选模型池已删除")?;
                let mut all_local = true;
                for profile in pool.tiers.iter().flat_map(|tier| &tier.profile_ids) {
                    let model = server
                        .db
                        .get_llm_model(profile)?
                        .ok_or("模型池中的档案已删除")?;
                    all_local &= model.to_model_config().uses_local_chat();
                }
                all_local
            }
            None => runtime.config.model.uses_local_chat(),
        };
        if !local {
            return Err("当前资料档案已关闭外部模型调用".into());
        }
    }
    let recorder = runtime.model_row.as_ref().map(|m| {
        Arc::new(crate::integrations::llm::usage::DatabaseUsageRecorder::new(
            server.db.clone(),
            m.id.clone(),
            m.provider.clone(),
            m.model.clone(),
            "chat",
        )) as Arc<dyn crate::integrations::llm::client::UsageRecorder>
    });
    let mut client = LlmClient::new_with_usage_recorder(
        &runtime.config.model,
        server.secret_resolver.clone(),
        recorder,
    )
    .with_generation_overrides(runtime.generation);
    client = client.with_profile_permissions(server.db.clone_connection());
    let pool = runtime.model_pool_id.clone();
    if let Some(pool_id) = pool {
        let callback = route_callback(conversation, tx);
        client = client.with_model_pool(
            crate::integrations::llm::model_pool::load_model_pool_runtime(
                &server.db,
                server.secret_resolver.clone(),
                &pool_id,
                conversation,
                callback,
            )?,
        );
    }
    Ok(client)
}
pub(crate) fn route_callback(
    conversation: &str,
    tx: &Sender<AgentEvent>,
) -> Arc<dyn Fn(crate::integrations::llm::model_pool::ModelRouteEvent) + Send + Sync> {
    let sender = tx.clone();
    let conv = conversation.to_string();
    let callback = Arc::new(
        move |route: crate::integrations::llm::model_pool::ModelRouteEvent| {
            let event = AgentEvent {
                event_type: "model_route".into(),
                conversation_id: conv.clone(),
                token: None,
                tool_call_id: None,
                tool_name: None,
                args: Some(serde_json::to_value(&route).expect("route event serializes")),
                result: route.interrupted_content.clone(),
                status: Some(route.phase.clone()),
                error: None,
                message_id: None,
                risk_level: None,
                reason: route.reason.map(|reason| format!("{reason:?}")),
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                configuration_source: None,
                should_replan: None,
            };
            let _ = sender.try_send(event);
        },
    );
    callback
}
