// ============================================================
// Application lifecycle — listening, background serving, shutdown
// ============================================================

use std::sync::Arc;

use crate::safety::ControlSession;

use super::bootstrap;
use super::state::AppServer;

/// Start the HTTP server (blocking). For standalone mode.
pub async fn serve(addr: &str) {
    let (server, router) = bootstrap::create_server()
        .await
        .expect("Failed to create server");

    tracing::info!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    let server_for_shutdown = Arc::clone(&server);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received; closing MCP runtime and managed processes");
            shutdown(&server_for_shutdown).await;
        })
        .await
        .expect("Server error");
}

/// Release application-lifetime resources in dependency order.
///
/// MCP sessions and managed child processes are closed before the process
/// exits so no external server or child process is orphaned.
pub async fn shutdown(server: &AppServer) {
    server.mcp_runtime_manager.shutdown_all().await;
    server.managed_process_registry.shutdown_all();
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
    let (_server, router) = bootstrap::create_server_with_control_session(control_session).await?;

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
