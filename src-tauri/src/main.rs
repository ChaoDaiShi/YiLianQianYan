// ============================================================
// 忆涟千言 Desktop App
// Embeds the backend HTTP server + opens Tauri webview
// ============================================================

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing;

const BACKEND_START_TIMEOUT: Duration = Duration::from_secs(8);
const BACKEND_HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupDiagnosticCategory {
    BackendSpawnFailed,
    BackendHealthTimeout,
    BackendPortUnavailable,
}

impl StartupDiagnosticCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::BackendSpawnFailed => "BackendSpawnFailed",
            Self::BackendHealthTimeout => "BackendHealthTimeout",
            Self::BackendPortUnavailable => "BackendPortUnavailable",
        }
    }
}

#[derive(Clone, Debug)]
struct StartupDiagnostic {
    category: StartupDiagnosticCategory,
    message: &'static str,
    timestamp: u64,
}

impl StartupDiagnostic {
    fn new(category: StartupDiagnosticCategory) -> Self {
        let message = match category {
            StartupDiagnosticCategory::BackendSpawnFailed => "Local backend failed to start",
            StartupDiagnosticCategory::BackendHealthTimeout => {
                "Local backend health check timed out"
            }
            StartupDiagnosticCategory::BackendPortUnavailable => {
                "Local backend port 9420 is unavailable"
            }
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64;

        Self {
            category,
            message,
            timestamp,
        }
    }
}

fn classify_backend_start_error(error: &str) -> StartupDiagnosticCategory {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("address already in use") || normalized.contains("os error 10048") {
        StartupDiagnosticCategory::BackendPortUnavailable
    } else {
        StartupDiagnosticCategory::BackendSpawnFailed
    }
}

fn log_startup_diagnostic(diagnostic: &StartupDiagnostic) {
    tracing::error!(
        category = diagnostic.category.as_str(),
        diagnostic_message = diagnostic.message,
        timestamp = diagnostic.timestamp,
        "desktop startup diagnostic"
    );
}

fn probe_backend_health(port: u16, timeout: Duration) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    if stream
        .write_all(b"GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }

    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() && response.is_empty() {
        return false;
    }

    response.starts_with("HTTP/1.1 200")
        && response.contains("\"status\":\"healthy\"")
        && response.contains("\"service\":\"yilian-backend\"")
        && response.contains("\"database\":\"healthy\"")
}

fn wait_for_backend_health(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if probe_backend_health(port, Duration::from_millis(500)) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

#[derive(Clone)]
struct ControlSessionState(String);

#[tauri::command]
fn get_control_session_token(state: tauri::State<'_, ControlSessionState>) -> String {
    state.0.clone()
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Set workspace root to project root (Tauri runs from src-tauri/)
    let exe_dir = std::env::current_dir().unwrap_or_default();
    let project_root = exe_dir.parent().unwrap_or(&exe_dir).to_path_buf();
    std::env::set_var("YILIAN_WORKSPACE", &project_root);
    tracing::info!("Workspace root: {}", project_root.display());

    let control_session = yilian_backend::safety::ControlSession::generate();
    let control_token = control_session.token().to_string();
    let backend_control_token = control_token.clone();

    // Start the backend server on a background thread with its own tokio runtime.
    // The shared token stays in memory and is never logged. Startup failures are
    // reported as bounded, redacted diagnostics; the desktop shell still opens.
    let (startup_tx, startup_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(_) => {
                let diagnostic =
                    StartupDiagnostic::new(StartupDiagnosticCategory::BackendSpawnFailed);
                log_startup_diagnostic(&diagnostic);
                let _ = startup_tx.send(Err(diagnostic));
                return;
            }
        };
        rt.block_on(async {
            match yilian_backend::serve_in_background_with_control_token(backend_control_token)
                .await
            {
                Ok(port) => {
                    tracing::info!("Backend server started on port {}", port);
                    let _ = startup_tx.send(Ok(port));
                    // Keep the thread alive so the runtime doesn't drop.
                    loop {
                        tokio::time::sleep(Duration::from_secs(3600)).await;
                    }
                }
                Err(error) => {
                    let diagnostic = StartupDiagnostic::new(classify_backend_start_error(&error));
                    log_startup_diagnostic(&diagnostic);
                    let _ = startup_tx.send(Err(diagnostic));
                }
            }
        });
    });

    match startup_rx.recv_timeout(BACKEND_START_TIMEOUT) {
        Ok(Ok(port)) => {
            if wait_for_backend_health(port, BACKEND_HEALTH_TIMEOUT) {
                tracing::info!("Backend health check passed on port {}", port);
            } else {
                log_startup_diagnostic(&StartupDiagnostic::new(
                    StartupDiagnosticCategory::BackendHealthTimeout,
                ));
            }
        }
        Ok(Err(_)) => {}
        Err(mpsc::RecvTimeoutError::Timeout) => {
            log_startup_diagnostic(&StartupDiagnostic::new(
                StartupDiagnosticCategory::BackendHealthTimeout,
            ));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            log_startup_diagnostic(&StartupDiagnostic::new(
                StartupDiagnosticCategory::BackendSpawnFailed,
            ));
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(ControlSessionState(control_token))
        .invoke_handler(tauri::generate_handler![get_control_session_token])
        .setup(|_app| {
            tracing::info!("Tauri desktop window ready, backend on http://127.0.0.1:9420");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    #[test]
    fn address_in_use_is_classified_as_port_unavailable() {
        assert_eq!(
            classify_backend_start_error("Address already in use (os error 10048)"),
            StartupDiagnosticCategory::BackendPortUnavailable,
        );
    }

    #[test]
    fn other_start_errors_are_classified_as_spawn_failed() {
        assert_eq!(
            classify_backend_start_error("permission denied"),
            StartupDiagnosticCategory::BackendSpawnFailed,
        );
    }

    #[test]
    fn startup_diagnostics_use_static_redacted_messages() {
        let diagnostic = StartupDiagnostic::new(StartupDiagnosticCategory::BackendHealthTimeout);

        assert_eq!(diagnostic.category.as_str(), "BackendHealthTimeout");
        assert_eq!(diagnostic.message, "Local backend health check timed out",);
        assert!(diagnostic.timestamp > 0);
    }

    #[test]
    fn health_probe_accepts_only_the_expected_healthy_service() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"status\":\"healthy\",\"service\":\"yilian-backend\",\"database\":\"healthy\"}",
                )
                .unwrap();
        });

        assert!(probe_backend_health(port, Duration::from_secs(1)));
        server.join().unwrap();
    }

    #[test]
    fn health_probe_rejects_an_unrelated_service() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"status\":\"healthy\",\"service\":\"other\",\"database\":\"healthy\"}",
                )
                .unwrap();
        });

        assert!(!probe_backend_health(port, Duration::from_secs(1)));
        server.join().unwrap();
    }
}
