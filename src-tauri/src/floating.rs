use std::{fs, path::PathBuf};
use tauri::menu::CheckMenuItem;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

pub(crate) const PET_LABEL: &str = "desktop-pet";
pub(crate) const DESKTOP_WIDGET_LABEL: &str = "desktop-widget";
pub(crate) const DESKTOP_PET_ENABLED: bool = true;
const CALL_LABEL: &str = "call-overlay";
const MONITOR_LABEL: &str = "monitor-overlay";
pub(crate) const TRAY_WINDOW_LABELS: [&str; 2] = [CALL_LABEL, MONITOR_LABEL];
const PET_OPEN_CHANGED: &str = "desktop-pet:open-changed";
const PET_WIDTH: f64 = 104.0;
const PET_HEIGHT: f64 = 112.0;

pub(crate) struct DesktopPetTrayItem(pub CheckMenuItem<tauri::Wry>);

#[derive(Default)]
pub(crate) struct DesktopPetFullscreen(
    std::sync::Mutex<Option<tokio_util::sync::CancellationToken>>,
);

fn notify_desktop_pet_open(app: &AppHandle, open: bool) {
    if let Some(item) = app.try_state::<DesktopPetTrayItem>() {
        let _ = item.0.set_checked(open);
    }
    let _ = app.emit_to("main", PET_OPEN_CHANGED, open);
}

#[derive(serde::Deserialize, serde::Serialize)]
struct DesktopPetState {
    position: (i32, i32),
    topmost: bool,
    #[serde(default)]
    open: bool,
}

fn desktop_pet_state_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    let directory = match std::env::var_os("YILIAN_DATA_DIR") {
        Some(directory) => PathBuf::from(directory),
        None => app.path().app_local_data_dir()?,
    };
    Ok(directory.join("desktop-pet-position.json"))
}

fn load_desktop_pet_state(app: &AppHandle) -> Option<DesktopPetState> {
    let bytes = fs::read(desktop_pet_state_path(app).ok()?).ok()?;
    serde_json::from_slice(&bytes).ok().or_else(|| {
        let position = serde_json::from_slice(&bytes).ok()?;
        Some(DesktopPetState {
            position,
            topmost: true,
            open: false,
        })
    })
}

pub fn save_desktop_pet_state(app: &AppHandle, open: bool) {
    let Some(window) = app.get_webview_window(PET_LABEL) else {
        return;
    };
    let save = || -> Result<(), Box<dyn std::error::Error>> {
        let position = window.outer_position()?;
        let state = DesktopPetState {
            position: (position.x, position.y),
            topmost: window.is_always_on_top()?,
            open,
        };
        let path = desktop_pet_state_path(app)?;
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }
        let pending = path.with_extension("tmp");
        fs::write(&pending, serde_json::to_vec(&state)?)?;
        fs::rename(pending, path)?;
        Ok(())
    };
    if let Err(error) = save() {
        tracing::warn!(%error, "Could not save the desktop pet state");
    }
}

pub fn restore_desktop_pet(app: &AppHandle) {
    if let Some(saved) = load_desktop_pet_state(app).filter(|state| state.open) {
        if let Err(error) = open_desktop_pet(app, Some(saved)) {
            tracing::warn!(%error, "Could not restore the desktop pet");
        }
    }
}

pub fn toggle_desktop_pet(app: &AppHandle) {
    let result = if let Some(window) = app.get_webview_window(PET_LABEL) {
        window.close().map_err(|error| error.to_string())
    } else {
        open_desktop_pet(app, load_desktop_pet_state(app))
    };
    if let Err(error) = result {
        tracing::warn!(%error, "Could not toggle the desktop pet from the tray");
        notify_desktop_pet_open(app, app.get_webview_window(PET_LABEL).is_some());
    }
}

fn open_desktop_pet(app: &AppHandle, saved: Option<DesktopPetState>) -> Result<(), String> {
    if !DESKTOP_PET_ENABLED {
        return Err("桌宠功能暂未开放".into());
    }
    if let Some(window) = app.get_webview_window(PET_LABEL) {
        return window.show().map_err(|error| error.to_string());
    }
    let builder = WebviewWindowBuilder::new(app, PET_LABEL, WebviewUrl::App("floating/pet".into()))
        .title("小涟桌宠")
        .inner_size(PET_WIDTH, PET_HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(saved.as_ref().map_or(true, |state| state.topmost))
        .skip_taskbar(true)
        .focused(false)
        .visible(false);
    #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
    let builder = builder.data_directory(
        crate::browser::fixture_root()
            .ok_or("Native fixture root is unavailable")?
            .join("main-webview"),
    );
    let window = builder.build().map_err(|error| error.to_string())?;

    let position = saved.map(|state| state.position);
    let saved_monitor = position.and_then(|(x, y)| {
        window
            .monitor_from_point(f64::from(x), f64::from(y))
            .ok()
            .flatten()
    });
    let restored = position.filter(|_| saved_monitor.is_some());
    let monitor = saved_monitor
        .or_else(|| {
            app.get_webview_window("main")
                .and_then(|main| main.current_monitor().ok().flatten())
        })
        .or_else(|| window.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let width = (PET_WIDTH * scale).round() as u32;
        let height = (PET_HEIGHT * scale).round() as u32;
        let right = area.position.x + area.size.width.saturating_sub(width) as i32;
        let bottom = area.position.y + area.size.height.saturating_sub(height) as i32;
        let margin = (16.0 * scale).round() as i32;
        let (x, y) = restored.unwrap_or((right - margin, bottom - margin));
        window
            .set_position(PhysicalPosition::new(
                x.clamp(area.position.x, right),
                y.clamp(area.position.y, bottom),
            ))
            .map_err(|error| error.to_string())?;
    }
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { .. } => save_desktop_pet_state(&handle, false),
        WindowEvent::Destroyed => {
            if let Some(token) = handle
                .state::<DesktopPetFullscreen>()
                .0
                .lock()
                .unwrap()
                .take()
            {
                token.cancel();
            }
            notify_desktop_pet_open(&handle, false);
        }
        _ => {}
    });
    window.show().map_err(|error| error.to_string())?;
    notify_desktop_pet_open(app, true);
    Ok(())
}

#[tauri::command]
pub(crate) async fn desktop_pet_resize(view: tauri::Webview, scale: f64) -> Result<(), String> {
    crate::trusted_desktop_pet(&view)?;
    let scale = scale.clamp(0.8, 2.5);
    let window = view.window();
    window
        .set_size(LogicalSize::new(PET_WIDTH * scale, PET_HEIGHT * scale))
        .map_err(|error| error.to_string())?;
    if let Some(monitor) = window
        .current_monitor()
        .map_err(|error| error.to_string())?
    {
        let area = monitor.work_area();
        let size = window.outer_size().map_err(|error| error.to_string())?;
        let position = window.outer_position().map_err(|error| error.to_string())?;
        window
            .set_position(PhysicalPosition::new(
                position.x.clamp(
                    area.position.x,
                    area.position.x + area.size.width.saturating_sub(size.width) as i32,
                ),
                position.y.clamp(
                    area.position.y,
                    area.position.y + area.size.height.saturating_sub(size.height) as i32,
                ),
            ))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn desktop_pet_fullscreen_policy(
    view: tauri::Webview,
    enabled: bool,
    policy: tauri::State<'_, DesktopPetFullscreen>,
) -> Result<(), String> {
    crate::trusted_desktop_pet(&view)?;
    let window = view.window();
    let mut current = policy.0.lock().unwrap();
    if let Some(token) = current.take() {
        token.cancel();
    }
    if !enabled {
        return window.show().map_err(|error| error.to_string());
    }
    #[cfg(windows)]
    {
        let token = tokio_util::sync::CancellationToken::new();
        *current = Some(token.clone());
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            let mut hidden = false;
            loop {
                tokio::select! {
                    biased;
                    _ = token.cancelled() => break,
                    _ = interval.tick() => {
                        let Ok(hwnd) = window.hwnd() else { break; };
                        let fullscreen = foreground_fills_monitor(hwnd);
                        if fullscreen != hidden {
                            let result = if fullscreen { window.hide() } else { window.show() };
                            if result.is_ok() { hidden = fullscreen; }
                        }
                    }
                }
            }
        });
        Ok(())
    }
    #[cfg(not(windows))]
    Err("当前平台不支持全屏检测".into())
}

#[cfg(windows)]
fn foreground_fills_monitor(pet: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL},
        UI::WindowsAndMessaging::{
            GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect,
        },
    };
    // Only compare geometry. The desktop pet does not read other window content.
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid()
            || [pet, GetDesktopWindow(), GetShellWindow()].contains(&foreground)
        {
            return false;
        }
        let monitor = MonitorFromWindow(pet, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() || MonitorFromWindow(foreground, MONITOR_DEFAULTTONULL) != monitor {
            return false;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut rect = RECT::default();
        if !GetMonitorInfoW(monitor, &mut info).as_bool()
            || GetWindowRect(foreground, &mut rect).is_err()
        {
            return false;
        }
        let screen = info.rcMonitor;
        rect.left <= screen.left
            && rect.top <= screen.top
            && rect.right >= screen.right
            && rect.bottom >= screen.bottom
    }
}

#[tauri::command]
pub(crate) async fn desktop_widget_set(
    view: tauri::Webview,
    app: tauri::AppHandle,
    open: bool,
) -> Result<(), String> {
    crate::desktop_perception::trusted_main(&view)?;
    if let Some(window) = app.get_webview_window(DESKTOP_WIDGET_LABEL) {
        return if open { window.show() } else { window.close() }
            .map_err(|error| error.to_string());
    }
    if !open {
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(
        &app,
        DESKTOP_WIDGET_LABEL,
        WebviewUrl::App("floating/desktop-widget".into()),
    )
    .title("忆涟桌面组件")
    .inner_size(272.0, 112.0)
    .decorations(false)
    .resizable(false)
    .always_on_bottom(true)
    .skip_taskbar(true)
    .visible(false);
    #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
    let builder = builder.data_directory(
        crate::browser::fixture_root()
            .ok_or("Native fixture root is unavailable")?
            .join("main-webview"),
    );
    let window = builder.build().map_err(|error| error.to_string())?;
    if let Some(monitor) = window
        .primary_monitor()
        .map_err(|error| error.to_string())?
    {
        let area = monitor.work_area();
        window
            .set_position(PhysicalPosition::new(
                area.position.x + 24,
                area.position.y + 48,
            ))
            .map_err(|error| error.to_string())?;
    }
    window.show().map_err(|error| error.to_string())
}
fn label(kind: &str) -> Result<&'static str, String> {
    match kind {
        "call" => Ok(CALL_LABEL),
        "monitor" => Ok(MONITOR_LABEL),
        _ => Err("Unknown floating surface".into()),
    }
}
#[tauri::command]
pub async fn open_floating_surface(
    view: tauri::Webview,
    app: tauri::AppHandle,
    kind: String,
    expanded: bool,
) -> Result<(), String> {
    crate::browser::trusted_caller(&view)?;
    if kind == "pet" {
        return open_desktop_pet(&app, load_desktop_pet_state(&app));
    }
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
        crate::refresh_floating_tray_item(&app);
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
    crate::refresh_floating_tray_item(&app);
    Ok(())
}
#[tauri::command]
pub async fn resize_floating_surface(
    view: tauri::Webview,
    app: tauri::AppHandle,
    kind: String,
    expanded: bool,
) -> Result<(), String> {
    crate::browser::trusted_caller(&view)?;
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
pub async fn hide_floating_surface(
    view: tauri::Webview,
    app: tauri::AppHandle,
    kind: String,
) -> Result<(), String> {
    crate::browser::trusted_caller(&view)?;
    if let Some(window) = app.get_webview_window(label(&kind)?) {
        window.hide().map_err(|e| e.to_string())?;
        crate::refresh_floating_tray_item(&app);
    }
    Ok(())
}
#[tauri::command]
pub async fn position_floating_surface(
    view: tauri::Webview,
    app: tauri::AppHandle,
    kind: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    crate::browser::trusted_caller(&view)?;
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
pub async fn close_floating_surface(
    view: tauri::Webview,
    app: tauri::AppHandle,
    kind: String,
) -> Result<(), String> {
    crate::browser::trusted_caller(&view)?;
    let name = if kind == "pet" {
        PET_LABEL
    } else {
        label(&kind)?
    };
    if let Some(window) = app.get_webview_window(name) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}
