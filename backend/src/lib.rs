// ============================================================
// 忆涟千言 Backend Library
// Exposes server creation for both standalone binary and Tauri embedding
// ============================================================

pub mod agent;
pub mod api;
pub mod capability;
pub mod config;
pub mod db;
pub mod execution;
pub mod isolation;
pub mod llm;
pub mod mcp;
pub mod mcp_runtime;
pub mod plugin;
pub mod safety;
pub mod secret;
pub mod server;
pub mod task;
pub mod tools;
pub mod utils;
pub mod workflow;
pub mod workspace;

use std::path::PathBuf;
use std::sync::Arc;
use tracing;

pub use server::AppServer;

use crate::safety::ControlSession;

fn default_data_dir() -> PathBuf {
    std::env::var("YILIAN_DATA_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("yilianqianyan")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Create the server instance (but don't start listening yet)
pub async fn create_server() -> Result<(Arc<AppServer>, axum::Router), String> {
    create_server_with_control_session(None).await
}

async fn create_server_with_control_session(
    control_session: Option<ControlSession>,
) -> Result<(Arc<AppServer>, axum::Router), String> {
    let data_dir = default_data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    let db_path = data_dir.join("yilianqianyan.db");

    let workspace_root = std::env::var("YILIAN_WORKSPACE")
        .ok()
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| ".".to_string());

    tracing::info!(
        "Backend: data_dir={}, workspace={}",
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
    let router = api::build_router(server.clone());

    Ok((server, router))
}

/// Start the HTTP server (blocking). For standalone mode.
pub async fn serve(addr: &str) {
    let (server, router) = create_server().await.expect("Failed to create server");

    tracing::info!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    let server_for_shutdown = Arc::clone(&server);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received; closing MCP runtime");
            server_for_shutdown.mcp_runtime_manager.shutdown_all().await;
        })
        .await
        .expect("Server error");
}

/// Start the HTTP server on a background task (for Tauri embedding).
/// Returns the port it's listening on.
pub async fn serve_in_background() -> u16 {
    serve_in_background_inner(None)
        .await
        .expect("Failed to start server")
}

pub async fn serve_in_background_with_control_token(token: String) -> Result<u16, String> {
    let control_session = ControlSession::new(token).map_err(|error| error.to_string())?;
    serve_in_background_inner(Some(control_session)).await
}

async fn serve_in_background_inner(control_session: Option<ControlSession>) -> Result<u16, String> {
    let (_server, router) = create_server_with_control_session(control_session).await?;

    let addr = std::env::var("YILIAN_HOST").unwrap_or_else(|_| "127.0.0.1:9420".to_string());
    let port: u16 = addr
        .split(':')
        .last()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9420);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|error| error.to_string())?;

    tracing::info!("Server listening on http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    Ok(port)
}
