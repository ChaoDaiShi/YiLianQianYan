use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
fn label(kind: &str) -> Result<&'static str, String> {
    match kind {
        "call" => Ok("call-overlay"),
        "monitor" => Ok("monitor-overlay"),
        _ => Err("Unknown floating surface".into()),
    }
}
#[tauri::command]
pub async fn open_floating_surface(
    app: tauri::AppHandle,
    kind: String,
    expanded: bool,
) -> Result<(), String> {
    let name = label(&kind)?;
    let (width, height) = if expanded {
        (360.0, if kind == "call" { 420.0 } else { 300.0 })
    } else {
        (88.0, 64.0)
    };
    if let Some(window) = app.get_webview_window(name) {
        window
            .set_size(LogicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        name,
        WebviewUrl::App(format!("floating/{kind}?expanded={expanded}").into()),
    )
    .title(if kind == "call" {
        "小涟通话"
    } else {
        "系统监控"
    })
    .inner_size(width, height)
    .min_inner_size(64.0, 64.0)
    .transparent(true)
    .decorations(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn resize_floating_surface(
    app: tauri::AppHandle,
    kind: String,
    expanded: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window(label(&kind)?)
        .ok_or("Floating surface is not open")?;
    let (width, height) = if expanded {
        (360.0, if kind == "call" { 420.0 } else { 300.0 })
    } else {
        (88.0, 64.0)
    };
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn hide_floating_surface(app: tauri::AppHandle, kind: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(label(&kind)?) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub fn position_floating_surface(
    app: tauri::AppHandle,
    kind: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let window = app
        .get_webview_window(label(&kind)?)
        .ok_or("Floating surface is not open")?;
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("Monitor unavailable")?;
    let scale = monitor.scale_factor();
    let origin = monitor.position().to_logical::<f64>(scale);
    let screen = monitor.size().to_logical::<f64>(scale);
    let size = window
        .outer_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    window
        .set_position(LogicalPosition::new(
            x.clamp(
                origin.x,
                (origin.x + screen.width - size.width).max(origin.x),
            ),
            y.clamp(
                origin.y,
                (origin.y + screen.height - size.height).max(origin.y),
            ),
        ))
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn close_floating_surface(app: tauri::AppHandle, kind: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(label(&kind)?) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}
