// ============================================================
// API — HTTP transport handlers (REST + SSE)
// ============================================================
//
// This module owns the per-domain request handlers and their DTOs. Router
// composition lives in `crate::app::router`, which assembles the surface from
// these handler modules.

pub(crate) mod agents;
pub(crate) mod approvals;
pub(crate) mod artifact_download;
pub(crate) mod capabilities;
pub(crate) mod chat;
pub(crate) mod commands;
pub(crate) mod conversations;
pub(crate) mod events;
pub(crate) mod llm_models;
pub(crate) mod logs;
pub(crate) mod mcp_runtime;
pub(crate) mod memories;
pub(crate) mod plugins;
pub(crate) mod projections;
pub(crate) mod resources;
pub(crate) mod secrets;
pub(crate) mod security;
pub(crate) mod security_grants;
pub(crate) mod settings;
pub(crate) mod skill_candidates;
pub(crate) mod skills_route;
pub(crate) mod subagents;
pub(crate) mod system;
pub(crate) mod task_world;
pub(crate) mod tasks;
pub(crate) mod tools;
pub(crate) mod voice;
pub(crate) mod workflow_runtime;
pub(crate) mod workflows;
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
