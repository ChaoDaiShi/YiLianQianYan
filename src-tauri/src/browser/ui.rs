//! Commands available to the trusted browser chrome, never to website content.
use super::*;

fn scope(view: &Webview, tab_id: &str) -> Result<(), String> {
    trusted_caller(view)?;
    let label = view.window().label().to_string();
    if label == "main" || label == format!("browser-shell-{tab_id}") {
        Ok(())
    } else {
        Err("This browser window does not own the requested tab".into())
    }
}

#[tauri::command]
pub async fn native_browser_ui_state(
    view: Webview,
    host: tauri::State<'_, Arc<NativeBrowserHost>>,
) -> Result<Value, String> {
    trusted_caller(&view)?;
    let mut state = host.ui_state();
    let window = view.window();
    if let Some(tab_id) = window.label().strip_prefix("browser-shell-") {
        if let Some(requests) = state["requests"].as_array_mut() {
            requests.retain(|request| request["tab_id"].as_str() == Some(tab_id));
        }
        if let Some(zoom) = state["zoom"].as_object_mut() {
            zoom.retain(|id, _| id == tab_id);
        }
    } else if window.label() != "main" {
        return Err("Browser controls are available only in a browser window".into());
    }
    Ok(state)
}

#[tauri::command]
pub async fn native_browser_resolve_request(
    view: Webview,
    host: tauri::State<'_, Arc<NativeBrowserHost>>,
    id: String,
    decision: String,
) -> Result<Value, String> {
    trusted_caller(&view)?;
    let state = host.ui_state();
    let tab_id = state["requests"]
        .as_array()
        .and_then(|requests| {
            requests
                .iter()
                .find(|request| request["id"].as_str() == Some(&id))
        })
        .and_then(|request| request["tab_id"].as_str())
        .ok_or("This browser request is no longer pending")?;
    scope(&view, tab_id)?;
    host.resolve_request(id, decision).await
}

#[tauri::command]
pub async fn native_browser_command(
    view: Webview,
    host: tauri::State<'_, Arc<NativeBrowserHost>>,
    target: BrowserTarget,
    command: String,
    #[allow(unused_variables)] payload: Option<Value>,
) -> Result<Value, String> {
    scope(&view, &target.tab_id)?;
    host.browser_command(target, command, payload.unwrap_or_else(|| json!({})))
        .await
}
