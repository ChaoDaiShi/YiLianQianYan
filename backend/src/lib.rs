// ============================================================
// 忆涟千言 Backend Library
// Exposes server creation for both standalone binary and Tauri embedding
// ============================================================

pub mod agent;
pub mod api;
pub mod config;
pub mod db;
pub mod llm;
pub mod safety;
pub mod server;
pub mod tools;

use std::path::PathBuf;
use std::sync::Arc;
use tracing;

pub use server::AppServer;

fn default_data_dir() -> PathBuf {
    std::env::var("YILIAN_DATA_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("yilianqianyan")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Create the server instance (but don't start listening yet)
pub async fn create_server() -> Result<(Arc<AppServer>, axum::Router), String> {
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

    let server = Arc::new(AppServer::new(&db_path, &workspace_root)?);
    let router = api::build_router(server.clone());

    Ok((server, router))
}

/// Start the HTTP server (blocking). For standalone mode.
pub async fn serve(addr: &str) {
    let (_server, router) = create_server().await.expect("Failed to create server");

    tracing::info!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, router).await.expect("Server error");
}

/// Start the HTTP server on a background task (for Tauri embedding).
/// Returns the port it's listening on.
pub async fn serve_in_background() -> u16 {
    let (_server, router) = create_server().await.expect("Failed to create server");

    let addr = std::env::var("YILIAN_HOST").unwrap_or_else(|_| "127.0.0.1:9420".to_string());
    let port: u16 = addr
        .split(':')
        .last()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9420);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    tracing::info!("Server listening on http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    port
}
