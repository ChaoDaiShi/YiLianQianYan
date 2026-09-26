// ============================================================
// 忆涟千言 Backend Library
// Exposes server creation for both standalone binary and Tauri embedding
// ============================================================

pub mod agent;
pub mod api;
pub mod app;
pub mod capability;
pub mod config;
pub mod db;
pub mod execution;
pub mod interaction;
pub mod isolation;
pub mod llm;
pub mod mcp;
pub mod mcp_runtime;
pub mod memory_skill;
pub mod plugin;
pub mod resource_input;
pub mod safety;
pub mod secret;
pub mod server;
pub mod shared;
pub mod skill_management;
pub mod task;
pub mod tools;
pub mod utils;
pub mod voice;
pub mod workflow;
pub mod workspace;

/// Create the server instance (but don't start listening yet).
pub use app::bootstrap::create_server;
/// Serving entrypoints for the standalone binary and Tauri embedding.
pub use app::lifecycle::{serve, serve_in_background, serve_in_background_with_control_token};
pub use app::state::AppServer;
