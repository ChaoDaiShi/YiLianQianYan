// ============================================================
// HTTP router composition
// ============================================================
//
// Assembles the REST/SSE surface from the per-domain handler modules in
// `crate::api`. Route paths and middleware order are unchanged: `/api/health`
// and provider-authenticated channel callbacks are public. Management routes
// remain behind the desktop control-session layer.

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

use super::state::AppServer;
use crate::api::{
    agents, approvals, artifact_download, capabilities, chat, cloud, commands, conversations,
    events, llm_models, logs, mcp_runtime, memories, plugins, projections, resources, search,
    secrets, security, security_grants, settings, skill_candidates, skills_route, subagents,
    system, tasks, tools, voice, workflow_runtime, workflows, workspaces,
};
mod automations;
mod control;
mod task_world;
use control::{control_plane_cors, require_control_session};

pub fn build_router(server: Arc<AppServer>) -> Router {
    let protected = Router::new()
        .merge(crate::modules::onboarding::router())
        .merge(crate::modules::resource::policy::router())
        .merge(crate::api::activity_events::router())
        .route(
            "/api/workflow-runs/:run_id/context-manifest",
            get(crate::api::context_manifest::get_workflow_handler),
        )
        .route("/api/work-index", get(crate::api::work_index::list))
        .route(
            "/api/work-index/reconcile",
            post(crate::api::work_index::reconcile),
        )
        .route("/api/outputs", get(crate::api::outputs::list))
        .route(
            "/api/outputs/:kind/:id/preview",
            get(crate::api::outputs::preview),
        )
        .route(
            "/api/outputs/:kind/:id/download",
            get(crate::api::outputs::download),
        )
        .route(
            "/api/conversations/:id/context-manifest",
            get(crate::api::context_manifest::get_handler),
        )
        .merge(crate::api::connectors::router())
        .merge(crate::api::knowledge::router())
        .merge(crate::api::knowledge_build::router())
        .merge(crate::api::conversation_outputs::router())
        .merge(crate::api::rich_content::router())
        .merge(workspaces::router())
        .merge(crate::api::creations::router())
        .merge(crate::api::browser_sites::router())
        .merge(crate::api::conversations::messages::router())
        .merge(crate::api::learning::router())
        .merge(crate::api::workspace_code::router())
        .merge(crate::api::workspace_files::router())
        .merge(crate::api::channels::router())
        .merge(automations::router())
        .route("/api/memories/withdrawn", get(memories::withdrawn_handler))
        .route(
            "/api/memories/:id/governance",
            get(memories::governance_handler),
        )
        .route("/api/memories/:id/history", get(memories::history_handler))
        .route("/api/memories/:id/correct", post(memories::correct_handler))
        .route("/api/memories/:id/restore", post(memories::restore_handler))
        .route(
            "/api/memories/:id/withdraw",
            post(memories::withdraw_handler),
        )
        .merge(skills_route::discovery_router())
        .merge(crate::api::workspace_git::router())
        .merge(crate::api::conversation_modes::router())
        .merge(crate::api::settings_canvas::router())
        .merge(crate::api::plugin_settings::router())
        .merge(crate::api::plugin_services::router())
        .merge(crate::api::software::router())
        .route("/api/logs/tools", get(logs::get_tool_logs))
        .merge(crate::api::connector_messages::router())
        .merge(crate::api::access_rules::router())
        .route(
            "/api/plugins/features",
            get(capabilities::list_plugin_features),
        )
        .route(
            "/api/capabilities/local-discovery",
            post(capabilities::discover_local_services),
        )
        .route(
            "/api/agents/:id/profile",
            get(agents::get_agent_profile)
                .put(agents::update_agent_profile)
                .delete(agents::reset_agent_profile),
        )
        .route("/api/companion/display", get(agents::companion_display))
        .route(
            "/api/agents/:id/generation-options",
            get(agents::get_agent_generation_options),
        )
        .merge(crate::api::model_pools::router())
        .merge(crate::api::basic_services::router())
        .merge(crate::api::workspace_service_settings::router())
        .route("/api/conversations/:id", put(conversations::rename_handler))
        .route(
            "/api/conversations/:id/turns",
            get(conversations::turns_handler),
        )
        .route(
            "/api/conversations/:id/workspace",
            get(conversations::workspace_handler).put(conversations::workspace_update_handler),
        )
        .route(
            "/api/conversations/:id/goal",
            get(conversations::goal::get_handler).put(conversations::goal::put_handler),
        )
        .route(
            "/api/conversations/:id/inputs",
            get(crate::api::conversation_inputs::list_handler),
        )
        .route(
            "/api/conversation-inputs/:id/answer",
            post(crate::api::conversation_inputs::answer_handler),
        )
        .route("/api/chat", post(chat::chat_handler))
        .route("/api/chat/readiness", get(chat::readiness_handler))
        .route("/api/chat/cli-providers", get(chat::cli::list))
        .route("/api/chat/task-command", post(chat::task_commands::handle))
        .route("/api/chat/steer", post(chat::steering::handle))
        .merge(chat::lanes::router())
        .route("/api/cloud/identity", get(cloud::identity_handler))
        .route("/api/cloud/profile", get(cloud::profile_handler))
        .route(
            "/api/cloud/profile/switch",
            post(cloud::profile_switch_handler),
        )
        .route("/api/cloud/login", post(cloud::login_handler))
        .route("/api/cloud/register", post(cloud::register_handler))
        .route("/api/cloud/refresh", post(cloud::refresh_handler))
        .route("/api/cloud/logout", post(cloud::logout_handler))
        .route("/api/cloud/sessions", get(cloud::sessions_handler))
        .route(
            "/api/cloud/sessions/:id/revoke",
            post(cloud::revoke_session_handler),
        )
        .route("/api/cloud/sync/status", get(cloud::sync_status_handler))
        .route("/api/cloud/sync/choice", put(cloud::sync_choice_handler))
        .route(
            "/api/cloud/sync/policy",
            get(cloud::sync_policy_handler).put(cloud::update_sync_policy_handler),
        )
        .route(
            "/api/cloud/sync/conflicts",
            get(cloud::sync_conflicts_handler),
        )
        .route(
            "/api/cloud/sync/conflicts/:id/resolve",
            post(cloud::resolve_sync_conflict_handler),
        )
        .route("/api/cloud/sync/now", post(cloud::sync_now_handler))
        .route(
            "/api/cloud/privacy/summary",
            get(cloud::privacy_summary_handler),
        )
        .route("/api/cloud/export", post(cloud::request_export_handler))
        .route(
            "/api/cloud/export/:id/download",
            get(cloud::download_export_handler),
        )
        .route(
            "/api/cloud/account/delete",
            post(cloud::request_deletion_handler),
        )
        .route(
            "/api/cloud/jobs/:kind/:id",
            get(cloud::lifecycle_job_handler),
        )
        .route(
            "/api/search/config",
            get(search::config_handler).put(search::update_config_handler),
        )
        .route("/api/search", post(search::search_handler))
        .route("/api/chat/stop", post(chat::stop_handler))
        .route("/api/events", get(events::events_handler))
        .route("/api/commands", post(commands::execute_handler))
        .route(
            "/api/resources/ingest",
            post(resources::ingest_handler).layer(DefaultBodyLimit::max(
                crate::shared::resource::DEFAULT_MAX_RESOURCE_BYTES,
            )),
        )
        .route("/api/resources", get(resources::list_handler))
        .route(
            "/api/resources/:id/preview",
            get(resources::preview_handler),
        )
        .route(
            "/api/resources/:id/content",
            get(resources::content_handler),
        )
        .route("/api/resources/:id", get(resources::get_handler))
        .route(
            "/api/resource-bindings",
            post(resources::bind_handler).get(resources::list_bindings_handler),
        )
        .route(
            "/api/resource-bindings/:id",
            delete(resources::unbind_handler),
        )
        .route("/api/presence", get(voice::presence_handler))
        .route("/api/voice/providers", get(voice::providers_handler))
        .route(
            "/api/voice/approvals/:approval_id/displayed",
            post(voice::approval_displayed_handler).delete(voice::approval_display_revoke_handler),
        )
        .route("/api/voice/session", get(voice::session_handler))
        .route("/api/voice/sessions/start", post(voice::start_handler))
        .route(
            "/api/voice/sessions/reinitialize",
            post(voice::reinitialize_handler),
        )
        .route("/api/voice/sessions/stop", post(voice::stop_handler))
        .route(
            "/api/voice/sessions/interrupt",
            post(voice::interrupt_handler),
        )
        .route("/api/voice/sessions/cancel", post(voice::cancel_handler))
        .route("/api/voice/leases", post(voice::lease_handler))
        .route("/api/voice/context", post(voice::context_handler))
        .route(
            "/api/voice/transcript/partial",
            post(voice::partial_handler),
        )
        .route("/api/voice/transcribe", post(voice::transcribe_handler))
        .route(
            "/api/voice/transcribe/partial",
            post(voice::transcribe_partial_handler),
        )
        .route("/api/voice/turns/dispatch", post(voice::dispatch_handler))
        .route("/api/voice/speak", post(voice::speak_handler))
        .route("/api/voice/speech/start", post(voice::speech_start_handler))
        .route(
            "/api/voice/speech/finished",
            post(voice::speech_finished_handler),
        )
        .route(
            "/api/projections/tasks",
            get(projections::task_projections_handler),
        )
        .merge(task_world::router())
        .route("/api/conversations", get(conversations::list_handler))
        .route("/api/conversations", post(conversations::create_handler))
        .route("/api/conversations/:id", get(conversations::load_handler))
        .route(
            "/api/conversations/:id",
            delete(conversations::delete_handler),
        )
        .route("/api/skills", get(skills_route::list_skills))
        .route("/api/skills", post(skills_route::create_skill))
        .route("/api/skills/:name", get(skills_route::load_skill))
        .route("/api/skills/:name", put(skills_route::update_skill))
        .route("/api/skills/:name", delete(skills_route::delete_skill))
        .route("/api/settings", get(settings::get_handler))
        .route("/api/settings", put(settings::update_handler))
        .route(
            "/api/providers/:kind/verify",
            post(settings::verify_provider_handler),
        )
        .route("/api/llm/models", get(llm_models::list_handler))
        .route(
            "/api/llm/provider-models",
            get(crate::api::provider_models::list),
        )
        .route("/api/llm/models", post(llm_models::create_handler))
        .route(
            "/api/llm/models/discover",
            post(crate::api::llm_catalog::discover),
        )
        .route("/api/llm/models/:id", put(llm_models::update_handler))
        .route("/api/llm/models/:id", delete(llm_models::delete_handler))
        .route(
            "/api/llm/models/:id/verify",
            post(llm_models::verify_handler),
        )
        .route(
            "/api/llm/models/:id/activate",
            post(llm_models::activate_handler),
        )
        .route("/api/llm/usage", get(llm_models::usage_handler))
        .route("/api/tools", get(tools::list_handler))
        .route(
            "/api/harnesses",
            get(crate::api::harnesses::get).put(crate::api::harnesses::put),
        )
        .route(
            "/api/harnesses/active",
            post(crate::api::harnesses::activate),
        )
        .route("/api/system", get(system::system_info))
        .route("/api/system/cpu", get(system::cpu_info))
        .route("/api/system/memory", get(system::memory_info))
        .route(
            "/api/system/preferences",
            get(system::get_preferences).put(system::put_preferences),
        )
        // Memories API
        .route("/api/memories", get(memories::list_handler))
        .route("/api/memories/library", get(memories::library_handler))
        .merge(memories::graph::router())
        .route("/api/memories/activity", get(memories::activity_handler))
        .route(
            "/api/memories/activity/:id",
            delete(memories::remove_activity_handler),
        )
        .route("/api/memories", post(memories::create_handler))
        .route("/api/memories/stats", get(memories::stats_handler))
        .route("/api/memories/extract", post(memories::candidates::extract))
        .route(
            "/api/memories/candidates/feedback",
            post(memories::candidates::stage_feedback),
        )
        .route("/api/memories/candidates", get(memories::candidates::list))
        .route(
            "/api/memories/candidates/:id/accept",
            post(memories::candidates::accept),
        )
        .route(
            "/api/memories/candidates/:id/dismiss",
            post(memories::candidates::dismiss),
        )
        .route("/api/memories/retrieve", get(memories::retrieve_handler))
        // Reserved future endpoints
        .route(
            "/api/memories/batch-import",
            post(memories::batch_import_handler),
        )
        .route(
            "/api/memories/batch-delete",
            post(memories::batch_delete_handler),
        )
        .route("/api/memories/export", get(memories::export_handler))
        .route("/api/memories/merge", post(memories::merge_handler))
        .route("/api/memories/reindex", post(memories::reindex_handler))
        .route(
            "/api/memories/index-job",
            get(memories::indexing::status)
                .post(memories::indexing::start)
                .delete(memories::indexing::cancel),
        )
        // Must be after fixed paths
        .route("/api/memories/:id", get(memories::get_handler))
        .route("/api/memories/:id", put(memories::update_handler))
        .route("/api/memories/:id", delete(memories::delete_handler))
        // Plugins / MCP API
        .route("/api/plugins", get(plugins::list_plugins))
        .route("/api/plugins/mcp", post(plugins::create_mcp))
        .route("/api/plugins/mcp/:id", put(plugins::update_mcp))
        .route("/api/plugins/mcp/:id", delete(plugins::delete_mcp))
        .route("/api/plugins/mcp/:id/toggle", post(plugins::toggle_mcp))
        .route("/api/plugins/mcp/:id/test", post(plugins::test_mcp))
        // Subagent metadata is read-only and excludes private instructions/paths.
        .route("/api/subagents", get(subagents::list_subagents))
        // Workflows API
        .route("/api/workflows", get(workflows::list_workflows))
        .route("/api/workflows", post(workflows::create_workflow))
        .route("/api/workflows/:id", get(workflows::get_workflow))
        .route("/api/workflows/:id", put(workflows::update_workflow))
        .route("/api/workflows/:id", delete(workflows::delete_workflow))
        .route(
            "/api/workflows/:id/activate",
            post(workflows::activate_workflow),
        )
        // Workflow runtime (executable graph + run) API
        .route(
            "/api/workflow-graphs",
            get(workflow_runtime::list_workflow_graphs),
        )
        .route(
            "/api/workflow-graphs",
            post(workflow_runtime::create_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id",
            get(workflow_runtime::get_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id",
            put(workflow_runtime::update_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id",
            delete(workflow_runtime::delete_workflow_graph),
        )
        .route(
            "/api/workflow-graphs/:id/run",
            post(workflow_runtime::run_workflow_graph),
        )
        .route(
            "/api/workflow-runs",
            get(workflow_runtime::list_workflow_runs),
        )
        .route(
            "/api/workflow-runs/:run_id",
            get(workflow_runtime::get_workflow_run),
        )
        .route(
            "/api/workflow-runs/:run_id/cancel",
            post(workflow_runtime::cancel_workflow_run),
        )
        // Task API
        .route("/api/tasks", get(tasks::list_tasks))
        .route("/api/tasks", post(tasks::create_task))
        .route("/api/tasks/:id", get(tasks::get_task))
        .route("/api/tasks/:id", put(tasks::update_task))
        .route("/api/tasks/:id", delete(tasks::delete_task))
        .route("/api/tasks/:id/start", post(tasks::start_task))
        .route("/api/tasks/:id/retry", post(tasks::retry_task))
        .route("/api/tasks/:id/cancel", post(tasks::cancel_task))
        .route("/api/tasks/:id/executions", get(tasks::list_executions))
        .route("/api/tasks/:id/timeline", get(tasks::list_timeline))
        .route("/api/tasks/:id/artifacts", get(tasks::list_artifacts))
        .route("/api/artifacts", get(tasks::list_all_artifacts))
        .route(
            "/api/artifacts/from-execution",
            post(crate::api::artifacts::materialize),
        )
        .route(
            "/api/artifacts/:id/download",
            get(artifact_download::download),
        )
        .route(
            "/api/artifacts/:id/preview",
            get(artifact_download::preview),
        )
        .route("/api/artifacts/:id", get(tasks::get_artifact))
        .route(
            "/api/skill-candidates",
            post(skill_candidates::create).get(skill_candidates::list),
        )
        .route("/api/skill-candidates/:id", put(skill_candidates::update))
        .route(
            "/api/skill-candidates/:id/validate",
            post(skill_candidates::validate),
        )
        .route(
            "/api/skill-candidates/:id/confirm",
            post(skill_candidates::confirm),
        )
        .route(
            "/api/skill-candidates/:id/reject",
            post(skill_candidates::reject),
        )
        .route(
            "/api/managed-skill-versions/:name",
            get(skill_candidates::versions),
        )
        .route(
            "/api/managed-skill-versions/:name/rollback",
            post(skill_candidates::rollback),
        )
        .route(
            "/api/managed-skill-versions/:name/deactivate",
            post(skill_candidates::deactivate),
        )
        .route("/api/task-decisions", get(tasks::list_pending_decisions))
        .route(
            "/api/task-decisions/:id/resolve",
            post(tasks::resolve_decision),
        )
        // Agent / Team API
        .route("/api/agents", get(agents::list_agents))
        .route("/api/agents", post(agents::create_agent))
        .route("/api/agents/:id", get(agents::get_agent))
        .route("/api/agent-teams", get(agents::list_teams))
        .route("/api/agent-teams", post(agents::create_team))
        .route("/api/agent-teams/:id", get(agents::get_team))
        // Capability API (discovery-only)
        .route("/api/capabilities", get(capabilities::list_capabilities))
        .route(
            "/api/capabilities/refresh",
            post(capabilities::refresh_capabilities),
        )
        .route("/api/capabilities/imports", get(capabilities::list_imports))
        .route(
            "/api/capabilities/imports/inspect",
            post(capabilities::inspect_import).layer(DefaultBodyLimit::max(3 * 1024 * 1024)),
        )
        .route(
            "/api/capabilities/imports/confirm",
            post(capabilities::confirm_import),
        )
        .route(
            "/api/capabilities/imports/previews/:id",
            delete(capabilities::discard_import_preview),
        )
        .route(
            "/api/capabilities/plugins/template",
            post(capabilities::plugin_template),
        )
        .route(
            "/api/capabilities/imports/:id",
            put(capabilities::change_import).delete(capabilities::remove_import),
        )
        .route("/api/capabilities/:id", get(capabilities::get_capability))
        // MCP runtime API (read-only; no direct tool execution)
        .route("/api/mcp/servers", get(mcp_runtime::list_servers))
        .route("/api/mcp/servers/:id", get(mcp_runtime::get_server))
        .route(
            "/api/mcp/servers/:id/refresh",
            post(mcp_runtime::refresh_server),
        )
        .route("/api/mcp/servers/:id/tools", get(mcp_runtime::list_tools))
        .route(
            "/api/mcp/servers/:id/resources",
            get(mcp_runtime::list_resources),
        )
        .route(
            "/api/mcp/servers/:id/resource-templates",
            get(mcp_runtime::list_resource_templates),
        )
        .route(
            "/api/mcp/servers/:id/resources/read",
            post(mcp_runtime::read_resource),
        )
        .route(
            "/api/mcp/servers/:id/prompts",
            get(mcp_runtime::list_prompts),
        )
        .route(
            "/api/mcp/servers/:id/prompts/get",
            post(mcp_runtime::get_prompt),
        )
        // Secrets status (value-free; no secret-read endpoint)
        .route("/api/secrets/status", get(secrets::status_handler))
        // Security grants (protected CRUD) + isolation status
        .route("/api/security/grants", get(security_grants::list_grants))
        .route("/api/security/grants", post(security_grants::create_grant))
        .route(
            "/api/security/grants/:id",
            delete(security_grants::delete_grant),
        )
        .route(
            "/api/security/isolation/status",
            get(security_grants::isolation_status),
        )
        // Logs API
        .route("/api/logs", get(logs::get_logs))
        .route("/api/logs", post(logs::push_log))
        // Approval API
        .route("/api/approvals/pending", get(approvals::pending_handler))
        .route("/api/approvals/:id", get(approvals::get_handler))
        .route(
            "/api/approvals/:id/approve",
            post(approvals::approve_handler),
        )
        .route("/api/approvals/:id/reject", post(approvals::reject_handler))
        .route("/api/approvals/:id/cancel", post(approvals::cancel_handler))
        // Security audit API (read/export only; deliberately no delete route)
        .route("/api/security/audit", get(security::list_audit))
        .route("/api/security/audit/export", post(security::export_audit))
        .route("/api/security/health", get(security::security_health))
        .route_layer(middleware::from_fn_with_state(
            server.clone(),
            require_control_session,
        ));

    Router::new()
        .merge(crate::api::channels::callback_router())
        .route("/api/health", get(system::health))
        .merge(protected)
        .layer(control_plane_cors())
        .with_state(server)
}
