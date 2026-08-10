// ============================================================
// 忆涟千言 Desktop App
// Embeds the backend HTTP server + opens Tauri webview
// ============================================================

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::thread;
use tracing;

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
    // The shared token stays in memory and is never logged.
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            let port =
                yilian_backend::serve_in_background_with_control_token(backend_control_token)
                    .await
                    .expect("Failed to start authenticated backend");
            tracing::info!("Backend server started on port {}", port);
            // Keep the thread alive so the runtime doesn't drop
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });
    });

    // Give the server a moment to start
    thread::sleep(std::time::Duration::from_millis(500));

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
