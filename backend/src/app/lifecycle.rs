// ============================================================
// Application lifecycle — listening, background serving, shutdown
// ============================================================

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use tokio_util::sync::CancellationToken;

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
    axum::serve(listener, with_shutdown_admission(router, &server))
        .with_graceful_shutdown(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::error!(%error, "Could not listen for the application shutdown signal");
                return;
            }
            shutdown(&server_for_shutdown).await;
        })
        .await
        .expect("Server error");
}

/// Checkpoint and release resources when the standalone server receives Ctrl+C.
pub async fn shutdown(server: &AppServer) {
    if let Err(error) = prepare_shutdown(server).await {
        tracing::error!(%error, "Application shutdown preparation failed");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopShutdownError {
    #[error("Could not save active task positions: {0}")]
    TaskPause(String),
    #[error("External software restoration was not confirmed: {0}")]
    SoftwareRestoration(String),
}

async fn prepare_shutdown(server: &AppServer) -> Result<(), DesktopShutdownError> {
    // This lifetime signal also stops the automation scheduler. The enabled
    // settings and the next scheduled times remain in their existing store.
    server.cloud_revocation_retry_cancel.cancel();
    server
        .task_world
        .pause_active_for_shutdown(
            &server.command_router,
            chrono::Utc::now().timestamp_millis(),
        )
        .map_err(DesktopShutdownError::TaskPause)?;
    for token in server.active_tasks.lock().values() {
        token.cancel();
    }
    for token in server.active_workflow_runs.lock().values() {
        token.cancel();
    }
    for token in server.active_task_executions.lock().values() {
        token.cancel();
    }

    let software = Arc::clone(&server.software_runtime);
    tokio::task::spawn_blocking(move || software.shutdown())
        .await
        .map_err(|error| DesktopShutdownError::SoftwareRestoration(error.to_string()))?
        .map_err(|error| DesktopShutdownError::SoftwareRestoration(error.to_string()))?;
    if let Err(error) = server.browser_runtime.quiesce(Duration::from_secs(8)).await {
        tracing::warn!(code=%error.code,"Native browser shutdown did not confirm completion");
    }
    server.channel_runtime.shutdown().await;
    if let Err(error) = server.plugin_services.shutdown() {
        tracing::warn!(%error, "Plugin service shutdown did not confirm completion");
    }
    server.mcp_runtime_manager.shutdown_all().await;
    server.managed_process_registry.shutdown_all();
    Ok(())
}

fn with_shutdown_admission(router: Router, server: &AppServer) -> Router {
    router.layer(middleware::from_fn_with_state(
        server.cloud_revocation_retry_cancel.clone(),
        shutdown_admission,
    ))
}

async fn shutdown_admission(
    State(shutdown): State<CancellationToken>,
    request: Request,
    next: Next,
) -> Response {
    if shutdown.is_cancelled() && !request.method().is_safe() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Application is shutting down",
        )
            .into_response();
    }
    next.run(request).await
}

/// In-process desktop lifecycle handle. It exposes no transport or credentials.
pub struct DesktopBackendShutdown {
    server: Arc<AppServer>,
    http_listener: tokio::sync::Mutex<tokio::task::JoinHandle<()>>,
}

impl DesktopBackendShutdown {
    pub async fn prepare(&self) -> Result<(), DesktopShutdownError> {
        let mut listener = self.http_listener.lock().await;
        prepare_shutdown(&self.server).await?;
        // Release the HTTP port before Tauri starts the replacement process.
        listener.abort();
        let _ = (&mut *listener).await;
        Ok(())
    }
}

/// Start the HTTP server on a background task (for Tauri embedding).
/// Returns the port it's listening on.
pub async fn serve_in_background() -> u16 {
    serve_in_background_inner(None, None, None)
        .await
        .expect("Failed to start server")
        .0
}

pub async fn serve_in_background_with_control_token(token: String) -> Result<u16, String> {
    let control_session = ControlSession::new(token).map_err(|error| error.to_string())?;
    serve_in_background_inner(Some(control_session), None, None)
        .await
        .map(|result| result.0)
}

pub async fn serve_in_background_with_browser(
    token: String,
    browser: Arc<crate::browser::BrowserRuntime>,
) -> Result<u16, String> {
    let session = ControlSession::new(token).map_err(|error| error.to_string())?;
    serve_in_background_inner(Some(session), Some(browser), None)
        .await
        .map(|result| result.0)
}

/// Workspace-internal desktop bridge. The host receives no HTTP credentials.
pub async fn serve_in_background_with_software(
    token: String,
    browser: Arc<crate::browser::BrowserRuntime>,
    software: Arc<dyn crate::software::SoftwareHost>,
) -> Result<u16, String> {
    serve_in_background_with_desktop_events(token, browser, software)
        .await
        .map(|result| result.0)
}

type DesktopEvents = tokio::sync::broadcast::Receiver<crate::shared::event::YiEvent>;
type BackgroundServer = (u16, DesktopEvents);

pub async fn serve_in_background_with_desktop_events(
    token: String,
    browser: Arc<crate::browser::BrowserRuntime>,
    software: Arc<dyn crate::software::SoftwareHost>,
) -> Result<BackgroundServer, String> {
    serve_in_background_with_desktop_shutdown(token, browser, software)
        .await
        .map(|(port, events, _)| (port, events))
}

pub async fn serve_in_background_with_desktop_shutdown(
    token: String,
    browser: Arc<crate::browser::BrowserRuntime>,
    software: Arc<dyn crate::software::SoftwareHost>,
) -> Result<(u16, DesktopEvents, DesktopBackendShutdown), String> {
    let session = ControlSession::new(token).map_err(|error| error.to_string())?;
    serve_in_background_inner(Some(session), Some(browser), Some(software)).await
}

async fn serve_in_background_inner(
    control_session: Option<ControlSession>,
    browser: Option<Arc<crate::browser::BrowserRuntime>>,
    software: Option<Arc<dyn crate::software::SoftwareHost>>,
) -> Result<(u16, DesktopEvents, DesktopBackendShutdown), String> {
    let (server, router) =
        bootstrap::create_server_with_control_session_and_browser(control_session, browser).await?;
    if let Some(host) = software {
        server.software_runtime.install_host(host);
        server.invalidate_capability_registry();
    }

    let addr = std::env::var("YILIAN_HOST").unwrap_or_else(|_| "127.0.0.1:9420".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|error| error.to_string())?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    tracing::info!("Server listening on http://{}", address);

    let router = with_shutdown_admission(router, &server);
    let http_listener = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            tracing::error!(%error, "Background HTTP server stopped");
        }
    });

    Ok((
        address.port(),
        server.event_hub.subscribe(),
        DesktopBackendShutdown {
            server,
            http_listener: tokio::sync::Mutex::new(http_listener),
        },
    ))
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
