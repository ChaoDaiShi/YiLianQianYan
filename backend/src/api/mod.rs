// ============================================================
// API — HTTP transport handlers (REST + SSE)
// ============================================================
//
// This module owns the per-domain request handlers and their DTOs. Router
// composition lives in `crate::app::router`, which assembles the surface from
// these handler modules.

pub(crate) mod access_rules;
pub(crate) mod agents;
pub use agents::{read_companion_display, CompanionDisplay};
mod desktop_perception;
pub use desktop_perception::{read_screen_permission, ScreenPermission};
pub(crate) mod activity_events;
pub(crate) mod approvals;
pub(crate) mod artifact_download;
pub(crate) mod automations;
pub(crate) mod basic_services;
pub(crate) mod browser_sites;
pub(crate) mod capabilities;
pub(crate) mod channels;
pub(crate) mod chat;
pub(crate) mod chat_presets;
mod chat_stop;
pub(crate) mod cloud;
mod cloud_errors;
pub(crate) mod cloud_lifecycle;
pub(crate) mod commands;
pub(crate) mod connector_messages;
pub(crate) mod connectors;
pub(crate) mod conversation_inputs;
pub(crate) mod conversation_modes;
pub(crate) mod conversation_outputs;
pub(crate) mod conversations;
pub(crate) mod creations;
pub(crate) mod events;
pub(crate) mod harnesses;
pub(crate) mod knowledge;
pub(crate) mod knowledge_build;
pub(crate) mod learning;
pub(crate) mod llm_catalog;
pub(crate) mod llm_models;
pub(crate) mod logs;
pub(crate) mod mcp_runtime;
pub(crate) mod memories;
pub(crate) mod model_pools;
pub(crate) mod plugin_model_proxy;
pub(crate) mod plugin_services;
pub(crate) mod plugin_settings;
pub(crate) mod plugins;
pub(crate) mod projections;
pub(crate) mod provider_models;
pub(crate) mod resources;
pub(crate) mod rich_content;
pub(crate) mod search;
pub(crate) mod secrets;
pub(crate) mod security;
pub(crate) mod security_grants;
pub(crate) mod settings;
pub(crate) mod settings_canvas;
pub(crate) mod skill_candidates;
pub(crate) mod skills_route;
pub(crate) mod software;
pub(crate) mod subagents;
pub(crate) mod system;
pub(crate) mod task_world;
pub(crate) mod tasks;
pub(crate) mod tools;
pub(crate) mod voice;
pub(crate) mod workflow_runtime;
pub(crate) mod workflows;
pub(crate) mod workspace_code;
pub(crate) mod workspace_files;
pub(crate) mod workspace_git;
pub(crate) mod workspace_pages;
pub(crate) mod workspace_service_settings;
pub(crate) mod workspaces;

pub use chat::chat_handler;
pub use chat::stop_handler;

/// Router composition is owned by the application layer; re-exported here so
/// existing `crate::api::build_router` call sites keep compiling.
pub use crate::app::router::build_router;

#[cfg(test)]
mod llm_models_tests;
#[cfg(test)]
mod tests;

pub(crate) mod artifacts;

pub(crate) mod work_index;

pub(crate) mod outputs;

pub(crate) mod context_manifest;

#[cfg(test)]
mod approvals_contract_tests;
