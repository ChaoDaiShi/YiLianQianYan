//! Trusted desktop lifecycle only; website content never chooses the parent HWND.
use crate::software::NativeSoftwareHost;
use std::sync::Arc;
use tauri::{Manager, WindowEvent};

fn trusted_app_origin(view: &tauri::Webview) -> Result<(), String> {
    let url = view.url().map_err(|e| e.to_string())?;
    let app_origin = url.scheme() == "tauri" && url.host_str() == Some("localhost")
        || url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none();
    let dev_origin = cfg!(debug_assertions)
        && view
            .app_handle()
            .config()
            .build
            .dev_url
            .as_ref()
            .is_some_and(|dev| dev.origin() == url.origin());
    if !app_origin && !dev_origin {
        return Err("Untrusted application origin".into());
    }
    Ok(())
}

pub fn bind(app: &tauri::AppHandle, host: &NativeSoftwareHost) -> Result<(), String> {
    #[cfg(windows)]
    if let Some(window) = app.get_window("main") {
        host.set_parent(window.hwnd().map_err(|e| e.to_string())?.0 as usize as u64)?;
    }
    let _ = (app, host);
    Ok(())
}

/// Resolve the parent only after a trusted main WebView can issue commands.
/// During native creation even queued HWND getters can contend with creation
/// messages; window lifecycle callbacks must never resolve Tauri handles.
#[tauri::command]
pub async fn native_software_bind(
    view: tauri::Webview,
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<NativeSoftwareHost>>,
) -> Result<(), String> {
    trusted_app_origin(&view)?;
    if view.label() != "main" {
        return Err("Software mounting belongs to the trusted main workspace".into());
    }
    let host = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || bind(&app, &host))
        .await
        .map_err(|error| error.to_string())?
}

pub fn window_event(
    window: &tauri::Window,
    event: &WindowEvent,
    host: &Arc<NativeSoftwareHost>,
) -> bool {
    if window.label() != "main" {
        return true;
    }
    #[cfg(windows)]
    if matches!(
        event,
        WindowEvent::Moved(_) | WindowEvent::Focused(true) | WindowEvent::ScaleFactorChanged { .. }
    ) {
        host.host_moved_or_activated();
    }
    // Restore the external window before hiding/minimizing its parent. Merely
    // losing focus is not cancellation and must not start another operation.
    let restore = matches!(event, WindowEvent::CloseRequested { .. })
        || matches!(event, WindowEvent::Resized(size) if size.width == 0 || size.height == 0);
    if restore {
        if let Err(error) = host.suspend_embedding() {
            tracing::error!(%error, "External software window restoration failed");
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
            }
            return false;
        }
    }
    true
}

pub fn hide_main(app: &tauri::AppHandle) {
    let host = app.state::<Arc<NativeSoftwareHost>>();
    if let Err(error) = host.suspend_embedding() {
        tracing::error!(%error, "Main window remains visible until external windows are restored");
        return;
    }
    if let Some(window) = app.get_window("main") {
        let _ = window.hide();
    }
}

pub async fn restore_before_exit(app: &tauri::AppHandle, host: Arc<NativeSoftwareHost>) -> bool {
    use yilian_backend::software::SoftwareHost;
    let result = tokio::task::spawn_blocking(move || host.shutdown()).await;
    if matches!(result, Ok(Ok(()))) {
        return true;
    }
    report_restore_failure(app);
    false
}

pub fn report_restore_failure(app: &tauri::AppHandle) {
    use tauri::Emitter;
    tracing::error!("Application exit deferred: external window restoration was not confirmed");
    let _ = app.emit(
        "software:restore-failed",
        "软件窗口尚未确认恢复，请在软件接入页面核对后再退出。",
    );
    if let Some(window) = app.get_window("main") {
        let _ = window.show();
    }
}

#[tauri::command]
pub async fn native_software_recognize_crop(
    view: tauri::Webview,
    png: String,
) -> Result<String, String> {
    trusted_app_origin(&view)?;
    if view.label() != "main" {
        return Err("请在主工作台识别选区".into());
    }
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || {
        crate::software::chat_capture::recognize_png(&png)
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    {
        let _ = png;
        Err("微信群聊识别需要 Windows 桌面版".into())
    }
}
