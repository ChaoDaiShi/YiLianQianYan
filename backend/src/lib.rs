// ============================================================
// 忆涟千言 Backend Library
// Exposes server creation for both standalone binary and Tauri embedding
// ============================================================

pub mod agent;
pub mod api;
pub mod app;
pub mod config;
pub mod db;
pub mod execution;
pub mod integrations;
pub mod interaction;
pub mod isolation;
pub mod modules;
pub mod plugin;
pub mod safety;
pub mod server;
pub mod shared;
pub mod task;
pub mod tools;
pub mod utils;
pub mod workspace;

/// Create the server instance (but don't start listening yet).
pub use app::bootstrap::create_server;
/// Serving entrypoints for the standalone binary and Tauri embedding.
pub use app::lifecycle::{serve, serve_in_background, serve_in_background_with_control_token};
pub use app::state::AppServer;
