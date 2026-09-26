// ============================================================
// Application bootstrap — construct the shared AppServer
// ============================================================
//
// Construction is a separate concern from state ownership so that the
// assembly sequence (database, recovery, providers, runtimes, adapters) is
// readable in one place.

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::db::Database;
use crate::interaction::{ApprovalVoiceAdapter, InteractionVoiceDispatch};
use crate::isolation::ManagedProcessRegistry;
use crate::mcp_runtime::McpRuntimeManager;
use crate::safety::{approval::ApprovalStore, AuditRecorder, ControlSession};
use crate::secret::{OsSecretStore, SecretResolver, SecretStore};
use crate::shared::command::CommandRouter;
use crate::shared::context::{ContextRequest, TaskProjectionProvider};
use crate::shared::event::EventHub;
use crate::shared::resource::ResourceService;
use crate::task::{TaskCommandService, TaskPresenceAdapter, TaskWorldRuntime};
use crate::tools::registry::ToolRegistry;
use crate::tools::skill::SkillDiscovery;
use crate::voice::GlobalVoiceSessionRuntime;

use super::state::{AppServer, LogBuffer};

impl AppServer {
    pub fn new(db_path: &std::path::Path, workspace_root: &str) -> Result<Self, String> {
        let control_session =
            ControlSession::from_environment_or_generate().map_err(|error| error.to_string())?;
        Self::new_with_control_session(db_path, workspace_root, control_session)
    }

    pub fn new_with_control_session(
        db_path: &std::path::Path,
        workspace_root: &str,
        control_session: ControlSession,
    ) -> Result<Self, String> {
        Self::new_with_control_session_and_store(
            db_path,
            workspace_root,
            control_session,
            Arc::new(OsSecretStore::new()),
        )
    }

    /// Full construction with an injectable SecretStore (production uses
    /// [`OsSecretStore`]; tests inject [`InMemorySecretStore`]).
    pub fn new_with_control_session_and_store(
        db_path: &std::path::Path,
        workspace_root: &str,
        control_session: ControlSession,
        secret_store: Arc<dyn SecretStore>,
    ) -> Result<Self, String> {
        let db = Database::new(db_path).map_err(|e| e.to_string())?;
        // v0.6 recovery: any execution left "running" by a previous process must
        // not pretend to keep running — mark interrupted and block its task.
        if let Ok(report) = crate::task::recovery::recover_interrupted(&db) {
            if report.interrupted_executions > 0 || report.interrupted_agent_executions > 0 {
                tracing::warn!(?report, "recovered interrupted task executions on startup");
            }
        }
        if let Ok(interrupted) = db.interrupt_running_conversations() {
            if interrupted > 0 {
                tracing::warn!(interrupted, "recovered interrupted conversation runs");
            }
        }
        let audit_recorder = AuditRecorder::new(db.clone_connection());
        let secret_resolver = Arc::new(SecretResolver::new(Arc::clone(&secret_store)));
        let config = db.get_settings().unwrap_or_default();

        // Auto-migrate old system prompts to the new one. (Secret migration is a
        // separate async step — see [`Self::migrate_secrets`].)
        let mut config = config;
        let needs_migration = config.agent.system_prompt.is_empty()
            || config.agent.system_prompt.contains("Flow 工作流编排 Agent")
            || config.agent.system_prompt.contains("LangGraph")
            || config.agent.system_prompt.contains("用 2-3 句话概括");
        if needs_migration {
            config.agent.system_prompt = crate::config::types::AgentConfig::default().system_prompt;
            let _ = db.save_settings(&config);
            tracing::info!("System prompt migrated to new version");
        }

        let managed_process_registry = Arc::new(ManagedProcessRegistry::new());
        let tool_registry = Arc::new(ToolRegistry::with_defaults_and_process_registry(
            workspace_root,
            Arc::clone(&managed_process_registry),
        ));

        let skill_dirs = if config.skills.directories.is_empty() {
            vec!["./skills".to_string()]
        } else {
            config.skills.directories.clone()
        };
        let skill_discovery = Arc::new(RwLock::new(SkillDiscovery::discover(
            &skill_dirs,
            workspace_root,
        )));

        let subagent_dirs = if config.subagents.directories.is_empty() {
            vec!["./.agents/agents".to_string()]
        } else {
            config.subagents.directories.clone()
        };
        let subagents = Self::discover_subagents(workspace_root, &subagent_dirs);

        let log_buffer = LogBuffer::new(2000);
        let event_hub = EventHub::new(256);
        let task_world =
            TaskWorldRuntime::new(&db, event_hub.clone()).map_err(|error| error.to_string())?;
        let resource_root = db_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("resources");
        let resource_service =
            ResourceService::new(db.clone_connection(), resource_root, event_hub.clone());
        let voice_runtime = GlobalVoiceSessionRuntime::new(event_hub.clone());
        let approval_store = Arc::new(ApprovalStore::new());
        let config = Arc::new(RwLock::new(config));
        let command_router = CommandRouter::new();
        command_router
            .register("core.echo", |request| Ok(request.payload.clone()))
            .map_err(|error| format!("register core.echo command: {error}"))?;
        TaskCommandService::new(task_world.clone())
            .register(&command_router)
            .map_err(|error| format!("register task command: {error}"))?;
        ApprovalVoiceAdapter::new(Arc::clone(&approval_store))
            .register(&command_router)
            .map_err(|error| format!("register approval command: {error}"))?;
        let presence_voice = voice_runtime.clone();
        let presence_task_world = task_world.clone();
        command_router
            .register("presence.get", move |_| {
                let request = ContextRequest::new("task-world");
                let presence = TaskPresenceAdapter::merge(
                    &presence_voice.presence(),
                    &presence_task_world.list(&request),
                );
                serde_json::to_value(presence).map_err(|error| {
                    crate::shared::command::CommandError::new(
                        "presence_serialization_failed",
                        error.to_string(),
                    )
                })
            })
            .map_err(|error| format!("register presence command: {error}"))?;

        let server = Self {
            db,
            config,
            tool_registry,
            managed_process_registry,
            skill_discovery,
            subagents,
            workspace_root: workspace_root.to_string(),
            active_tasks: Mutex::new(HashMap::new()),
            active_workflow_runs: Arc::new(Mutex::new(HashMap::new())),
            active_task_executions: Arc::new(Mutex::new(HashMap::new())),
            log_buffer,
            approval_store,
            audit_recorder,
            control_session,
            capability_registry: Arc::new(RwLock::new(None)),
            mcp_runtime_manager: Arc::new(McpRuntimeManager::with_resolver(Arc::clone(
                &secret_resolver,
            ))),
            secret_store,
            secret_resolver,
            voice_stt_requests: Arc::new(Semaphore::new(1)),
            event_hub,
            command_router,
            resource_service,
            voice_runtime,
            voice_dispatch_hook: Arc::new(RwLock::new(None)),
            task_world,
        };
        server.set_voice_dispatch_hook(Arc::new(InteractionVoiceDispatch::new(
            server.db.clone_connection(),
            server.task_world.clone(),
            Arc::clone(&server.approval_store),
            server.command_router.clone(),
        )));
        server.seed_default_grants();
        Ok(server)
    }
}

fn default_data_dir() -> std::path::PathBuf {
    std::env::var("YILIAN_DATA_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("yilianqianyan-v1")))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// Create the server instance (but don't start listening yet).
pub async fn create_server() -> Result<(Arc<AppServer>, axum::Router), String> {
    create_server_with_control_session(None).await
}

pub(crate) async fn create_server_with_control_session(
    control_session: Option<ControlSession>,
) -> Result<(Arc<AppServer>, axum::Router), String> {
    let data_dir = default_data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    let db_path = data_dir.join("yilianqianyan-v1.db");

    let workspace_root = std::env::var("YILIAN_WORKSPACE")
        .ok()
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| ".".to_string());

    tracing::info!(
        "Backend: profile=v1, data_dir={}, workspace={}",
        data_dir.display(),
        workspace_root
    );

    let server = Arc::new(match control_session {
        Some(control_session) => {
            AppServer::new_with_control_session(&db_path, &workspace_root, control_session)?
        }
        None => AppServer::new(&db_path, &workspace_root)?,
    });
    // Migrate legacy plaintext secrets BEFORE registering MCP servers, so the
    // runtime manager sees the post-migration `env_secret_refs`.
    server.migrate_secrets().await;
    // Seed base resource grants (workspace read/write + denied paths) so the
    // grant-enforced gateway has a sane starting point.
    server.seed_default_grants();
    // Register enabled MCP servers and refresh them (bounded) so the managed
    // catalog is Ready before the first Agent request. A dead server never
    // blocks startup — it just becomes Unavailable.
    server.register_mcp_servers_from_db();
    server.refresh_mcp_runtime().await;
    let router = super::router::build_router(server.clone());

    Ok((server, router))
}
