//! Conversation runtime bindings, shared by chat and approval continuation.
#[path = "chat_runtime_context.rs"]
mod context;
#[path = "chat_runtime_model.rs"]
mod model;
#[path = "chat_runtime_workflow.rs"]
mod workflow;
use crate::{
    agent::engine::AgentEvent,
    app::state::AppServer,
    config::types::AppConfig,
    db::{connectors::ConnectorRuntimeBindings, LlmModelRow},
    modules::harness_profiles as harness,
    safety::ResourceDescriptor,
    tools::ToolRegistry,
};
use model::route_callback;
pub use model::{client, readiness, resolve_model_source, ChatModelSource, ChatReadiness};
pub(crate) use model::{resolve_agent_model_source, resolve_scoped_model_source};
use serde_json::json;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc::Sender;
use workflow::ConnectorWorkflowTool;

pub(crate) fn explicit_chat_model_profile(
    db: &crate::db::Database,
    id: &str,
) -> Result<LlmModelRow, String> {
    let row = db.get_llm_model(id)?.ok_or("所选模型档案已删除")?;
    if row.api_format != "openai" {
        return Err("所选模型不支持对话协议".into());
    }
    Ok(row)
}

pub(super) fn ordinary_conversation(server: &AppServer, id: &str) -> Result<bool, String> {
    Ok(server.db.channel_for_conversation(id)?.is_none()
        && server.db.robot_run_for_conversation(id)?.is_none()
        && server.db.connector_agent_for_conversation(id)?.is_none()
        && !crate::modules::automation::personal::is_delegated(server, id)?)
}
pub struct ChatRuntime {
    pub execution_scope: Option<crate::agent::delegation::ExecutionScope>,
    pub agent_id: Option<String>,
    pub config: AppConfig,
    pub bindings: ConnectorRuntimeBindings,
    pub tools: Arc<ToolRegistry>,
    pub prompt: String,
    pub context_manifest: crate::execution::context_manifest::ContextManifest,
    pub model_row: Option<LlmModelRow>,
    pub model_pool_id: Option<String>,
    pub generation: crate::integrations::llm::generation::GenerationOverrides,
}
impl ChatRuntime {
    pub(crate) fn context_model_id(&self) -> String {
        if let Some(pool) = &self.model_pool_id {
            format!("pool:{pool}")
        } else if let Some(row) = &self.model_row {
            row.id.clone()
        } else {
            format!("{}/{}", self.config.model.provider, self.config.model.name)
        }
    }
}
pub async fn prepare(
    server: &Arc<AppServer>,
    conversation: Option<&str>,
    events: Option<&Sender<AgentEvent>>,
) -> Result<ChatRuntime, String> {
    prepare_with_model(server, conversation, events, None).await
}
pub async fn prepare_with_model(
    server: &Arc<AppServer>,
    conversation: Option<&str>,
    events: Option<&Sender<AgentEvent>>,
    model_profile_id: Option<&str>,
) -> Result<ChatRuntime, String> {
    prepare_for_turn(server, conversation, events, model_profile_id, None).await
}

pub(super) async fn prepare_for_turn(
    server: &Arc<AppServer>,
    conversation: Option<&str>,
    events: Option<&Sender<AgentEvent>>,
    model_profile_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<ChatRuntime, String> {
    prepare_selected(
        server,
        conversation,
        events,
        model_profile_id,
        agent_id,
        false,
    )
    .await
}

pub async fn prepare_resume_with_model(
    server: &Arc<AppServer>,
    conversation: Option<&str>,
    events: Option<&Sender<AgentEvent>>,
    model_profile_id: Option<&str>,
) -> Result<ChatRuntime, String> {
    let options = conversation
        .map(|id| server.db.load_chat_options(id))
        .transpose()?;
    let agent_id = options
        .as_ref()
        .and_then(|options| options.agent_id.as_deref());
    prepare_selected(
        server,
        conversation,
        events,
        model_profile_id,
        agent_id,
        true,
    )
    .await
}

async fn prepare_selected(
    server: &Arc<AppServer>,
    conversation: Option<&str>,
    events: Option<&Sender<AgentEvent>>,
    model_profile_id: Option<&str>,
    selected_agent_id: Option<&str>,
    resume: bool,
) -> Result<ChatRuntime, String> {
    let mut agent_id = conversation
        .map(|id| server.db.connector_agent_for_conversation(id))
        .transpose()?
        .flatten();
    let ordinary = agent_id.is_none()
        && match conversation {
            Some(id) => {
                server.db.channel_for_conversation(id)?.is_none()
                    && !crate::modules::automation::personal::is_delegated(server, id)?
                    && server.db.robot_run_for_conversation(id)?.is_none()
            }
            None => true,
        };
    let snapshot = if ordinary && resume {
        conversation
            .map(|id| {
                crate::execution::context_manifest::load_conversation_manifest(&server.db, id)
            })
            .transpose()?
            .flatten()
    } else {
        None
    };
    let selection = if !ordinary {
        None
    } else if resume {
        harness::from_manifest(&server.db, snapshot.as_ref().map(|value| &value.manifest))?
    } else {
        harness::active(&server.db)?
    };
    let harness_agent = if let Some((selection, id)) = selection.as_ref().and_then(|selection| {
        selection
            .profile
            .agent_id
            .as_deref()
            .map(|id| (selection, id))
    }) {
        if !selection.allows_resource(&server.db, id)? {
            return Err("当前 Harness 未启用该智能体所属的插件".into());
        }
        let id = crate::modules::task::AgentId::new(id).map_err(|error| error.to_string())?;
        Some(
            server
                .db
                .get_agent_definition(&id)?
                .filter(|agent| agent.enabled)
                .ok_or("Harness 绑定的智能体已停用或删除")?,
        )
    } else {
        None
    };
    let scope = selection
        .as_ref()
        .map(|selection| {
            harness::tool_scope(
                server,
                selection,
                harness_agent
                    .as_ref()
                    .map(|agent| agent.allowed_tools.as_slice()),
            )
        })
        .transpose()?;
    let turn_agent = if ordinary
        && selection
            .as_ref()
            .is_none_or(|selection| selection.profile.agent_id.is_none())
    {
        selected_agent_id
            .map(|id| {
                if let Some(selection) = selection.as_ref() {
                    if !selection.allows_resource(&server.db, id)? {
                        return Err("当前 Harness 未启用该智能体所属的插件".into());
                    }
                }
                let id =
                    crate::modules::task::AgentId::new(id).map_err(|error| error.to_string())?;
                super::super::chat_presets::get(&server.db, &id)?
                    .filter(|agent| agent.enabled)
                    .ok_or_else(|| "所选智能体已停用或删除".to_string())
            })
            .transpose()?
    } else {
        None
    };
    let workspace_id = conversation
        .map(|id| server.db.conversation_workspace(id))
        .transpose()?
        .flatten();
    let bindings = match agent_id.as_deref() {
        Some(id) => server.db.connector_runtime_bindings_for_scope(
            id,
            conversation,
            workspace_id.as_deref(),
        )?,
        None => ConnectorRuntimeBindings::default(),
    };
    let mut config = server.config.read().clone();
    let source = match model_profile_id {
        Some(id) => {
            let row = explicit_chat_model_profile(&server.db, id)?;
            ChatModelSource::Fixed {
                config: row.to_model_config(),
                row: Some(row),
            }
        }
        None => resolve_scoped_model_source(
            &server.db,
            &config.model,
            agent_id.as_deref().or_else(|| {
                harness_agent
                    .as_ref()
                    .or(turn_agent.as_ref())
                    .map(|agent| agent.id.as_str())
            }),
            workspace_id.as_deref(),
        )?,
    };
    let (model, row, pool_id) = match source {
        ChatModelSource::Fixed { config, row } => {
            if row.as_ref().is_some_and(|row| row.api_format != "openai") {
                return Err("所选模型不支持对话协议".into());
            }
            (config, row, None)
        }
        ChatModelSource::Pool(pool) => {
            if !pool.enabled || pool.service_kind != "chat" {
                return Err("所选对话模型池已停用或用途不匹配".into());
            }
            let first = pool
                .tiers
                .iter()
                .flat_map(|tier| tier.profile_ids.iter())
                .next()
                .ok_or("所选模型池没有候选档案")?;
            let row = server
                .db
                .get_llm_model(first)?
                .ok_or("模型池候选档案已删除")?;
            (row.to_model_config(), None, Some(pool.id))
        }
    };
    config.model = model;
    let model_row = row;
    let all = server
        .build_agent_tool_registry_scoped(scope.as_ref())
        .await;
    let mut tools =
        ToolRegistry::new_with_process_registry(server.managed_process_registry.clone());
    let mut prompt;
    if let Some(id) = agent_id.as_deref() {
        let agent = server
            .db
            .get_agent_definition(
                &crate::modules::task::AgentId::new(id).map_err(|e| e.to_string())?,
            )?
            .ok_or("绑定智能体已删除")?;
        if !agent.enabled {
            return Err("绑定智能体已停用".into());
        }
        let mut capabilities = bindings.capabilities.clone();
        let inactive =
            crate::api::capabilities::plugin_contributions::inactive_plugin_resource_ids(
                &server.db,
            )?;
        capabilities.retain(|(kind, id)| {
            !["workflow", "skill", "mcp"].contains(&kind.as_str()) || !inactive.contains(id)
        });

        for (_, package) in bindings
            .capabilities
            .iter()
            .filter(|(kind, _)| kind == "plugin")
        {
            let contributions = crate::api::capabilities::enabled_plugin_resources(&server.db)?;
            let selected = contributions
                .into_iter()
                .filter(|r| r["plugin_id"].as_str() == Some(package.as_str()))
                .collect::<Vec<_>>();
            for row in selected {
                if let (Some(kind), Some(id)) = (row["kind"].as_str(), row["id"].as_str()) {
                    capabilities.push((kind.into(), id.into()));
                }
            }
        }
        let mut skill_names = capabilities
            .iter()
            .filter(|(k, _)| k == "skill")
            .map(|(_, id)| id.clone())
            .collect::<HashSet<_>>();
        if let Some(scope) = scope
            .as_ref()
            .filter(|scope| scope.selection.profile.skill_ids.is_some())
        {
            let discovery = server.skill_discovery.read();
            skill_names.retain(|name| {
                scope.skill_names.contains(name)
                    && discovery
                        .get(name)
                        .is_some_and(|skill| skill.compatibility.runtime_ready)
            });
        }
        prompt = server.build_system_prompt_with_skills(Some(&skill_names));
        let mut allowed = capabilities
            .iter()
            .filter(|(kind, _)| kind == "tool")
            .map(|(_, id)| id.clone())
            .collect::<HashSet<_>>();
        let mcp = capabilities
            .iter()
            .filter(|(kind, _)| kind == "mcp")
            .map(|(_, id)| id.clone())
            .collect::<HashSet<_>>();
        for tool in all.all() {
            let matched_mcp=tool.security_descriptor(&json!({})).ok().is_some_and(|d|d.resources.iter().any(|r|matches!(r,ResourceDescriptor::Mcp{server_id,..} if mcp.contains(server_id))));
            if allowed.contains(tool.name()) || matched_mcp {
                tools.register(tool.clone());
            }
        }
        let mut discovery = server.skill_discovery.write();
        for name in &skill_names {
            let skill = discovery
                .get_mut(name)
                .ok_or_else(|| format!("绑定 Skill {name} 不存在"))?;
            prompt.push_str(&format!(
                "\n\n## 绑定 Skill：{name}\n{}",
                skill.load_content()?
            ));
        }
        drop(discovery);
        for kind in ["vision", "image"] {
            let tool_name = if kind == "vision" {
                "vision_analysis"
            } else {
                "image_generation"
            };
            let bound_profile = server
                .db
                .resolve_connector_bindings(id)?
                .iter()
                .any(|b| b.kind == "agent_model" && b.service_kind.as_deref() == Some(kind));
            if allowed.contains(tool_name) || bound_profile {
                tools.register(Arc::new(
                    crate::tools::basic_services::BasicServiceTool::new(
                        server.db.clone(),
                        server.secret_resolver.clone(),
                        kind,
                    )
                    .with_resource_service(server.resource_service.clone())
                    .with_agent_id(id.to_string()),
                ));
            }
        }
        let mut workflows = HashSet::new();
        for (_, id) in capabilities.iter().filter(|(kind, _)| kind == "workflow") {
            if let Some(recipe) = server.db.get_workflow(id).map_err(|e| e.to_string())? {
                prompt.push_str(&format!(
                    "\n\n## 绑定工作流：{}\n{}\n步骤：{}\n{}",
                    recipe.name,
                    recipe.description,
                    recipe.nodes.join(" → "),
                    recipe.system_prompt_extra
                ));
            } else if let Some(graph) = server.db.get_workflow_graph(id)? {
                workflows.insert(id.clone());
                prompt.push_str(&format!(
                    "\n\n可通过 run_workflow 调用的工作流：{}，graph_id={}。{}",
                    graph.name, id, graph.description
                ));
            } else {
                return Err("绑定工作流已删除".into());
            }
        }
        if !workflows.is_empty() {
            allowed.insert("run_workflow".into());
            tools.register(Arc::new(ConnectorWorkflowTool {
                server: server.clone(),
                graph_ids: workflows,
                agent_id: Some(id.to_string()),
                selection: None,
            }));
        }
    } else {
        prompt =
            server.build_system_prompt_with_skills(scope.as_ref().map(|scope| &scope.skill_names));
        for tool in all.all() {
            tools.register(tool.clone())
        }
        if conversation.is_some()
            && scope
                .as_ref()
                .is_none_or(|scope| scope.allows("update_memory"))
        {
            tools.register(Arc::new(crate::tools::memory::UpdateMemoryTool(
                server.db.clone_connection(),
            )));
        }
        if let Some(agent) = harness_agent.as_ref().or(turn_agent.as_ref()) {
            agent_id = Some(agent.id.to_string());
            config.agent.name = agent.name.clone();
            let heading = if harness_agent.is_some() {
                "Harness 智能体"
            } else {
                "当前智能体"
            };
            prompt.push_str(&format!(
                "\n\n## {heading}：{}\n{}",
                agent.name, agent.instructions
            ));
        }
        if let Some(selection) = &selection {
            let mut graphs = HashSet::new();
            for id in &selection.profile.workflow_ids {
                if let Some(graph) = server.db.get_workflow_graph(id)? {
                    if selection.allows_workflow(&server.db, id).unwrap_or(false) {
                        graphs.insert(id.clone());
                        prompt.push_str(&format!(
                            "\n\n可通过 run_workflow 调用的工作流：{}，graph_id={}。",
                            graph.name, id
                        ));
                    }
                }
            }
            if !graphs.is_empty() {
                tools.register(Arc::new(ConnectorWorkflowTool {
                    server: server.clone(),
                    graph_ids: graphs,
                    agent_id: agent_id.clone(),
                    selection: Some(selection.clone()),
                }));
            }
        }
    }
    // Each registered service uses the same real agent and conversation identity as this run.
    for kind in ["vision", "image"] {
        let name = if kind == "vision" {
            "vision_analysis"
        } else {
            "image_generation"
        };
        if tools.get(name).is_some() {
            let mut tool = crate::tools::basic_services::BasicServiceTool::new(
                server.db.clone(),
                server.secret_resolver.clone(),
                kind,
            )
            .with_resource_service(server.resource_service.clone())
            .with_conversation_id(conversation.unwrap_or_default().to_string());
            if let Some(id) = &agent_id {
                tool = tool.with_agent_id(id.clone());
            }
            if let Some(events) = events {
                tool = tool
                    .with_route_callback(route_callback(conversation.unwrap_or_default(), events));
            }
            tools.register(Arc::new(tool));
        }
    }
    // Necessary input is a conversation control, independent of bound action
    // capabilities. The engine suspends the turn; this tool grants no action.
    if scope.as_ref().is_none_or(|scope| scope.allows("ask_user")) {
        tools.register(Arc::new(crate::agent::user_input::AskUserTool));
    }
    if let Some(agent) = &turn_agent {
        if agent.id.as_str() != "interaction-general" {
            tools =
                tools.filtered(|name| agent.allowed_tools.iter().any(|allowed| allowed == name));
        }
    }
    let profile = crate::api::agents::persona::profile_for_agent(
        &server.db,
        agent_id.as_deref().unwrap_or("default"),
    )?;
    if ordinary {
        if let Some(fields) = crate::modules::onboarding::conversation_preferences(&server.db)? {
            prompt.push_str(&format!(
                "\n\n用户填写的个人资料与交流偏好（JSON 数据）：\n{fields}"
            ));
        }
    }
    let user_layer = crate::api::agents::persona::compose_user_persona(&profile);
    if turn_agent
        .as_ref()
        .is_some_and(|agent| super::super::chat_presets::is_preset(agent.id.as_str()))
    {
        let default = crate::api::agents::persona::profile_for_agent(&server.db, "default")?;
        let layer = crate::api::agents::persona::compose_user_persona(&default);
        if !layer.is_empty() {
            prompt.push_str(&format!("\n\n{layer}"));
        }
    }
    if !user_layer.is_empty() {
        prompt.push_str(&format!("\n\n{user_layer}"));
    }
    use crate::execution::context_manifest::{ContextManifest, ContextStatus};
    let mut context_manifest = ContextManifest::default();
    if ordinary {
        harness::record(&mut context_manifest, selection.as_ref());
    }
    context::append_bound_sources(
        server,
        conversation,
        &bindings,
        &mut prompt,
        &mut context_manifest,
    )?;
    if let Some(id) = workspace_id {
        let workspace = server
            .db
            .get_workspace_for_history(
                &crate::workspace::WorkspaceId::new(id).map_err(|e| e.to_string())?,
            )?
            .ok_or("当前对话的工作空间已不存在")?;
        let root = workspace.root_path.as_deref();
        prompt.push_str(&format!(
            "\n\n当前工作空间：{}\n目录上下文：{}\n",
            workspace.name,
            root.unwrap_or("未设置")
        ));
        prompt.push_str("用户只说“查看下”“看一下”“简单看看”，且本轮和最近相关对话都没有更明确的对象时，默认快速查看当前工作空间。按只读速览处理，简要说明空间用途、主要入口和有用的顶层结构。\n");
        if let Some(root) = root.filter(|path| std::path::Path::new(path).is_absolute()) {
            let list_args = json!({"path":root,"pattern":"*"});
            prompt.push_str(&format!(
                "入口路径已知时直接用 read_file。入口未知且提供了 glob 时，用参数 {list_args} 查看绑定根目录一层。优先读取 README.md 或其他 README 文本，其次选择项目说明、使用说明等明显入口。没有入口时按已看到的顶层内容给简短概览。调用文件工具时使用绑定目录下的绝对路径。"
            ));
        } else {
            prompt.push_str("当前没有绑定可供查看的绝对目录。先说明工作空间名称与目录状态；用户需要看本地文件时，再请其提供目录。");
        }
    }
    if conversation.is_some() {
        prompt.push_str(crate::api::rich_content::PROMPT);
    }
    let mut runtime = ChatRuntime {
        execution_scope: None,
        agent_id,
        config,
        bindings,
        tools: Arc::new(tools),
        prompt,
        context_manifest,
        model_row,
        model_pool_id: pool_id,
        generation: profile.generation(),
    };
    let prepared_prompt = runtime.prompt.clone();
    crate::modules::channel::restrict_runtime(&server.db, conversation, &mut runtime)?;
    crate::modules::channel::robot::prepare_runtime(server, conversation, &mut runtime)?;
    crate::modules::automation::personal::prepare_runtime(server, conversation, &mut runtime)?;
    if runtime.prompt != prepared_prompt {
        runtime.context_manifest = ContextManifest::default();
        runtime.context_manifest.record("runtime_scope", "restricted_prompt", None, ContextStatus::Excluded, 0, Some("Runtime replaced the prepared prompt; original memory and resource sources were not consumed"));
    }
    if let Some(entry) = snapshot
        .as_ref()
        .and_then(|snapshot| super::understanding::route_entry(&snapshot.manifest))
    {
        super::understanding::restore(&mut runtime, entry)?;
    }
    Ok(runtime)
}
