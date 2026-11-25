// ============================================================
// 忆涟千言 Desktop App
// Embeds the backend HTTP server + opens Tauri webview
// ============================================================

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tracing;
use yilian_backend::app::lifecycle::{
    serve_in_background_with_desktop_shutdown, DesktopBackendShutdown, DesktopShutdownError,
};
mod browser;
mod desktop_perception;
mod desktop_wallpaper;
mod floating;
mod software;
mod software_bridge;
mod workspace_folder;
#[cfg(all(feature = "native-browser-fixture", not(debug_assertions)))]
compile_error!("native-browser-fixture is a local debug-only diagnostic target");

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
struct DesktopBackendPort(Option<u16>);

struct DesktopExit {
    backend: Option<DesktopBackendShutdown>,
    preparing: AtomicBool,
    prepared: AtomicBool,
}

impl DesktopExit {
    fn begin(&self) -> bool {
        !self.preparing.swap(true, Ordering::AcqRel)
    }
}

async fn prepare_desktop_shutdown(app: &tauri::AppHandle) -> Result<(), String> {
    let exit = app.state::<DesktopExit>();
    floating::save_desktop_pet_state(app, true);
    let result = if let Some(backend) = &exit.backend {
        backend.prepare().await.map_err(|error| {
            if matches!(error, DesktopShutdownError::SoftwareRestoration(_)) {
                software_bridge::report_restore_failure(app);
            }
            error.to_string()
        })
    } else {
        let host = app
            .state::<std::sync::Arc<software::NativeSoftwareHost>>()
            .inner()
            .clone();
        if software_bridge::restore_before_exit(app, host).await {
            let browser = app.state::<std::sync::Arc<yilian_backend::browser::BrowserRuntime>>();
            if let Err(error) = browser.quiesce(Duration::from_secs(8)).await {
                tracing::warn!(%error, "Browser shutdown did not finish before application exit");
            }
            Ok(())
        } else {
            Err("External software restoration must finish before exit".into())
        }
    };
    match result {
        Ok(()) => {
            exit.prepared.store(true, Ordering::Release);
            Ok(())
        }
        Err(error) => {
            exit.preparing.store(false, Ordering::Release);
            Err(error)
        }
    }
}

#[tauri::command]
async fn get_control_session_token(
    view: tauri::Webview,
    state: tauri::State<'_, ControlSessionState>,
) -> Result<String, String> {
    browser::trusted_caller(&view)?;
    Ok(state.0.clone())
}

#[tauri::command]
async fn restart_application(view: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    browser::trusted_caller(&view)?;
    if !app.state::<DesktopExit>().begin() {
        return Err("Application shutdown is already in progress".into());
    }
    if let Err(error) = prepare_desktop_shutdown(&app).await {
        tracing::error!(%error, "Application restart preparation failed");
        return Err(error);
    }
    // Tauri cannot prevent a restart after the native restart request.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        app.request_restart();
    });
    Ok(())
}

fn configure_initial_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let window = app.get_window("main").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "main window is unavailable")
    })?;
    let monitor = match window.current_monitor()? {
        Some(monitor) => monitor,
        None => window.primary_monitor()?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "primary monitor is unavailable",
            )
        })?,
    };
    let work_area = monitor.work_area();
    let scale = monitor.scale_factor();
    let inner = window.inner_size()?;
    let outer = window.outer_size()?;
    let frame_width = (f64::from(outer.width) - f64::from(inner.width)) / scale;
    let frame_height = (f64::from(outer.height) - f64::from(inner.height)) / scale;
    let width = (f64::from(work_area.size.width) * 0.9 / scale - frame_width)
        .max(800.0)
        .min(1440.0);
    let height = (f64::from(work_area.size.height) * 0.9 / scale - frame_height)
        .max(600.0)
        .min(900.0);
    window.set_size(tauri::LogicalSize::new(width, height))?;
    let outer = window.outer_size()?;
    let x = i64::from(work_area.position.x)
        + (i64::from(work_area.size.width) - i64::from(outer.width)) / 2;
    let y = i64::from(work_area.position.y)
        + (i64::from(work_area.size.height) - i64::from(outer.height)) / 2;
    window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32))?;
    window.show()?;
    Ok(())
}

fn show_main(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("Main window unavailable")?;
    window.show().map_err(|error| error.to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    refresh_main_window_tray_item(app);
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
async fn show_main_from_desktop_pet(
    view: tauri::Webview,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if view.label() == floating::DESKTOP_WIDGET_LABEL {
        browser::trusted_app_origin(&view)?;
    } else {
        trusted_desktop_pet(&view)?;
    }
    show_main(&app)
}

fn trusted_desktop_pet(view: &tauri::Webview) -> Result<(), String> {
    if view.label() != floating::PET_LABEL {
        return Err("This command is restricted to the desktop pet".into());
    }
    browser::trusted_app_origin(view)
}

#[tauri::command]
async fn desktop_pet_companion_display(
    view: tauri::Webview,
    control: tauri::State<'_, ControlSessionState>,
    backend: tauri::State<'_, DesktopBackendPort>,
) -> Result<yilian_backend::api::CompanionDisplay, String> {
    trusted_desktop_pet(&view)?;
    let port = backend.0.ok_or("Local companion service is unavailable")?;
    yilian_backend::api::read_companion_display(port, &control.0).await
}

#[tauri::command]
async fn show_desktop_pet_menu(view: tauri::Webview, app: tauri::AppHandle) -> Result<(), String> {
    trusted_desktop_pet(&view)?;
    let window = app
        .get_webview_window(floating::PET_LABEL)
        .ok_or("Desktop pet unavailable")?;
    let popup = || -> tauri::Result<()> {
        let open = MenuItem::with_id(
            &app,
            "desktop-pet-open-main",
            "打开主窗口",
            true,
            None::<&str>,
        )?;
        let pin = CheckMenuItem::with_id(
            &app,
            "desktop-pet-toggle-topmost",
            "置顶",
            true,
            window.is_always_on_top()?,
            None::<&str>,
        )?;
        let chat = MenuItem::with_id(
            &app,
            "desktop-pet-open-chat",
            "与小涟对话",
            true,
            None::<&str>,
        )?;
        let close = MenuItem::with_id(&app, "desktop-pet-close", "关闭", true, None::<&str>)?;
        window.popup_menu(&Menu::with_items(&app, &[&open, &chat, &pin, &close])?)
    };
    popup().map_err(|error| error.to_string())
}

fn handle_desktop_pet_menu(app: &tauri::AppHandle, event: MenuEvent) {
    let Some(window) = app.get_webview_window(floating::PET_LABEL) else {
        return;
    };
    let result = match event.id().as_ref() {
        "desktop-pet-open-main" => show_main(app),
        "desktop-pet-open-chat" => show_main(app).and_then(|()| {
            app.emit_to("main", "desktop-pet:open-chat", ())
                .map_err(|error| error.to_string())
        }),
        "desktop-pet-toggle-topmost" => window
            .is_always_on_top()
            .and_then(|topmost| {
                window.set_always_on_top(!topmost)?;
                floating::save_desktop_pet_state(app, true);
                Ok(())
            })
            .map_err(|error| error.to_string()),
        "desktop-pet-close" => window.close().map_err(|error| error.to_string()),
        _ => return,
    };
    if let Err(error) = result {
        tracing::warn!(%error, "Desktop pet menu action failed");
    }
}

struct MainWindowTrayItem(MenuItem<tauri::Wry>);

fn main_window_is_shown(window: &tauri::Window) -> tauri::Result<bool> {
    Ok(window.is_visible()? && !window.is_minimized()?)
}

fn refresh_main_window_tray_item(app: &tauri::AppHandle) {
    let (Some(window), Some(item)) = (
        app.get_window("main"),
        app.try_state::<MainWindowTrayItem>(),
    ) else {
        return;
    };
    let update = || -> tauri::Result<()> {
        let text = if main_window_is_shown(&window)? {
            "隐藏主窗口"
        } else {
            "显示主窗口"
        };
        if item.0.text()? != text {
            item.0.set_text(text)?;
        }
        Ok(())
    };
    if let Err(error) = update() {
        tracing::warn!(%error, "Could not update the main window tray action");
    }
}

fn toggle_main_window_from_tray(
    app: &tauri::AppHandle,
    should_hide: impl FnOnce(&tauri::Window) -> tauri::Result<bool>,
) {
    let Some(window) = app.get_window("main") else {
        return;
    };
    match should_hide(&window) {
        Ok(true) => {
            software_bridge::hide_main(app);
            refresh_main_window_tray_item(app);
        }
        Ok(false) => {
            if let Err(error) = show_main(app) {
                tracing::warn!(%error, "Could not show the main window from the tray");
            }
        }
        Err(error) => tracing::warn!(%error, "Could not read the main window state"),
    }
}

struct FloatingWindowsTrayItem {
    menu: Menu<tauri::Wry>,
    item: MenuItem<tauri::Wry>,
}

fn floating_tray_windows(
    app: &tauri::AppHandle,
) -> tauri::Result<(Vec<tauri::WebviewWindow>, bool)> {
    let windows: Vec<_> = floating::TRAY_WINDOW_LABELS
        .iter()
        .filter_map(|label| app.get_webview_window(label))
        .collect();
    for window in &windows {
        if window.is_visible()? {
            return Ok((windows, true));
        }
    }
    Ok((windows, false))
}

fn refresh_floating_tray_item(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.try_state::<FloatingWindowsTrayItem>() else {
            return;
        };
        let update = || -> tauri::Result<()> {
            let (windows, visible) = floating_tray_windows(&handle)?;
            let items = tray.menu.items()?;
            let present = items.iter().any(|item| item.id() == tray.item.id());
            if windows.is_empty() {
                if present {
                    tray.menu.remove(&tray.item)?;
                }
            } else {
                tray.item.set_text(if visible {
                    "隐藏浮窗"
                } else {
                    "显示浮窗"
                })?;
                if !present {
                    tray.menu.insert(&tray.item, items.len() - 1)?;
                }
            }
            Ok(())
        };
        if let Err(error) = update() {
            tracing::warn!(%error, "Could not update the floating windows tray action");
        }
    });
}

fn toggle_floating_tray_windows(app: &tauri::AppHandle) {
    match floating_tray_windows(app) {
        Ok((windows, visible)) => {
            for window in windows {
                let result = if visible {
                    window.hide()
                } else {
                    window.show()
                };
                if let Err(error) = result {
                    tracing::warn!(%error, label = window.label(), "Could not toggle the floating window");
                }
            }
        }
        Err(error) => tracing::warn!(%error, "Could not read the floating windows state"),
    }
    refresh_floating_tray_item(app);
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    const TOGGLE_MAIN_WINDOW: &str = "tray-toggle-main-window";
    const TOGGLE_DESKTOP_PET: &str = "tray-toggle-desktop-pet";
    const TOGGLE_FLOATING_WINDOWS: &str = "tray-toggle-floating-windows";
    let main = MenuItem::with_id(app, TOGGLE_MAIN_WINDOW, "显示主窗口", true, None::<&str>)?;
    let floating = MenuItem::with_id(app, TOGGLE_FLOATING_WINDOWS, "显示浮窗", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit-app", "退出忆涟千言", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&main, &quit])?;
    app.manage(MainWindowTrayItem(main));
    if floating::DESKTOP_PET_ENABLED {
        let pet =
            CheckMenuItem::with_id(app, TOGGLE_DESKTOP_PET, "桌宠", true, false, None::<&str>)?;
        menu.insert(&pet, 1)?;
        app.manage(floating::DesktopPetTrayItem(pet));
    }
    app.manage(FloatingWindowsTrayItem {
        menu: menu.clone(),
        item: floating,
    });
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("application icon is configured");
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("忆涟千言")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TOGGLE_MAIN_WINDOW => toggle_main_window_from_tray(app, main_window_is_shown),
            TOGGLE_DESKTOP_PET => {
                let app = app.clone();
                // Create the WebView2 window outside the UI event handler.
                tauri::async_runtime::spawn(async move { floating::toggle_desktop_pet(&app) });
            }
            TOGGLE_FLOATING_WINDOWS => toggle_floating_tray_windows(app),
            "quit-app" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                toggle_main_window_from_tray(tray.app_handle(), |window| {
                    Ok(main_window_is_shown(window)? && window.is_focused()?)
                });
            }
        })
        .build(app)?;
    Ok(())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut context = tauri::generate_context!();
    // Isolated native acceptance runs use a random loopback Vite port. Release
    // builds ignore this override and never expose a remote-debugging listener.
    #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
    if let Ok(raw) = std::env::var("YILIAN_NATIVE_BROWSER_FRONTEND_URL") {
        let url = tauri::Url::parse(&raw).expect("Native test frontend URL is valid");
        assert!(
            url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
                && url.username().is_empty()
                && url.password().is_none(),
            "Native test frontend must be loopback HTTP without credentials"
        );
        context.config_mut().build.dev_url = Some(url);
    }
    let fixture_main: Option<(tauri::utils::config::WindowConfig, std::path::PathBuf)> = {
        #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
        {
            let root = browser::fixture_root()
                .expect("Native browser fixture requires a separate existing Temp root");
            // The diagnostic target may never fall back to the user's real database/workspace.
            let data = root.join("data");
            let workspace = root.join("workspace");
            std::fs::create_dir_all(&data).expect("Fixture data directory can be created");
            std::fs::create_dir_all(&workspace).expect("Fixture workspace can be created");
            std::env::set_var("YILIAN_DATA_DIR", data);
            std::env::set_var("YILIAN_WORKSPACE", workspace);

            let window = context
                .config_mut()
                .app
                .windows
                .iter_mut()
                .find(|w| w.label == "main")
                .expect("Main window config exists");
            let mut config = window.clone();
            config.create = true;
            config.visible = true;
            window.create = false;
            Some((config, root.join("main-webview")))
        }
        #[cfg(not(all(debug_assertions, feature = "native-browser-fixture")))]
        {
            None
        }
    };
    let bundled_skills = if cfg!(debug_assertions) {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skills")
    } else {
        tauri::utils::platform::resource_dir(context.package_info(), &tauri::utils::Env::default())
            .expect("application resource directory is available")
            .join("bundled-skills")
    };
    std::env::set_var("YILIAN_BUNDLED_SKILLS", bundled_skills);

    // Set workspace root to project root (Tauri runs from src-tauri/)
    let exe_dir = std::env::current_dir().unwrap_or_default();
    let project_root = exe_dir.parent().unwrap_or(&exe_dir).to_path_buf();
    let workspace_root = match std::env::var_os("YILIAN_WORKSPACE") {
        Some(path) => std::path::PathBuf::from(path),
        None => project_root,
    };
    std::env::set_var("YILIAN_WORKSPACE", &workspace_root);
    tracing::info!("Workspace root: {}", workspace_root.display());

    let control_session = yilian_backend::safety::ControlSession::generate();
    let control_token = control_session.token().to_string();
    let backend_control_token = control_token.clone();
    let browser_runtime = std::sync::Arc::new({
        #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
        {
            yilian_backend::browser::BrowserRuntime::for_local_fixture(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skills"),
            )
        }
        #[cfg(not(all(debug_assertions, feature = "native-browser-fixture")))]
        {
            yilian_backend::browser::BrowserRuntime::new()
        }
    });
    let backend_browser_runtime = browser_runtime.clone();
    let software_host = std::sync::Arc::new(software::NativeSoftwareHost::new());
    let backend_software_host = software_host.clone();

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
            match serve_in_background_with_desktop_shutdown(
                backend_control_token,
                backend_browser_runtime,
                backend_software_host,
            )
            .await
            {
                Ok((port, events, shutdown)) => {
                    tracing::info!("Backend server started on port {}", port);
                    let _ = startup_tx.send(Ok((port, events, shutdown)));
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

    let (backend_port, companion_events, backend_shutdown) =
        match startup_rx.recv_timeout(BACKEND_START_TIMEOUT) {
            Ok(Ok((port, events, shutdown))) => {
                if wait_for_backend_health(port, BACKEND_HEALTH_TIMEOUT) {
                    tracing::info!("Backend health check passed on port {}", port);
                } else {
                    log_startup_diagnostic(&StartupDiagnostic::new(
                        StartupDiagnosticCategory::BackendHealthTimeout,
                    ));
                }
                (Some(port), Some(events), Some(shutdown))
            }
            Ok(Err(_)) => (None, None, None),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                log_startup_diagnostic(&StartupDiagnostic::new(
                    StartupDiagnosticCategory::BackendHealthTimeout,
                ));
                (None, None, None)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log_startup_diagnostic(&StartupDiagnostic::new(
                    StartupDiagnosticCategory::BackendSpawnFailed,
                ));
                (None, None, None)
            }
        };

    let event_software_host = software_host.clone();
    let mut main_tray_item_dirty = true;
    tauri::Builder::default()
        .plugin(browser::scoped_shell())
        .on_menu_event(handle_desktop_pet_menu)
        .manage(ControlSessionState(control_token))
        .manage(DesktopBackendPort(backend_port))
        .manage(desktop_perception::CaptureSessions::default())
        .manage(floating::DesktopPetFullscreen::default())
        .manage(DesktopExit {
            backend: backend_shutdown,
            preparing: AtomicBool::new(false),
            prepared: AtomicBool::new(false),
        })
        .manage(browser_runtime.clone())
        .manage(software_host.clone())
        .invoke_handler({
            #[cfg(all(debug_assertions,feature="native-browser-fixture"))]
            {tauri::generate_handler![
get_control_session_token,
            workspace_folder::pick_workspace_folder,
            software_bridge::native_software_bind,
            software_bridge::native_software_recognize_crop,
            browser::native_browser_execute,
            browser::credentials::native_browser_credential_vault,
            browser::native_browser_tabs,
            browser::native_browser_unknown,
            browser::native_browser_acknowledge_unknown,
            browser::ui::native_browser_ui_state,
            browser::ui::native_browser_resolve_request,
            browser::ui::native_browser_command,
            restart_application,
            show_main_from_desktop_pet,
            desktop_pet_companion_display,
            floating::desktop_pet_resize,
            floating::desktop_pet_fullscreen_policy,
            floating::desktop_widget_set,
            desktop_perception::desktop_capture_screen,
            desktop_perception::desktop_cancel_capture,
            desktop_wallpaper::desktop_wallpaper_status,
            desktop_wallpaper::desktop_wallpaper_apply,
            desktop_wallpaper::desktop_wallpaper_playback,
            desktop_wallpaper::desktop_wallpaper_engine_preview,
            desktop_wallpaper::desktop_integration_status,
            desktop_wallpaper::desktop_rainmeter_invoke,
            show_desktop_pet_menu,
            floating::open_floating_surface,
            floating::resize_floating_surface,
            floating::position_floating_surface,
            floating::close_floating_surface,
            floating::hide_floating_surface,
                browser::native_browser_fixture_boundary,
                    browser::native_browser_fixture_quiesce,
                    browser::fixture::native_browser_fixture_schedule,
                    browser::fixture::native_browser_fixture_native_state
            ]}
            #[cfg(not(all(debug_assertions,feature="native-browser-fixture")))]
            {tauri::generate_handler![
get_control_session_token,
            workspace_folder::pick_workspace_folder,
            software_bridge::native_software_bind,
            software_bridge::native_software_recognize_crop,
            browser::native_browser_execute,
            browser::credentials::native_browser_credential_vault,
            browser::native_browser_tabs,
            browser::native_browser_unknown,
            browser::native_browser_acknowledge_unknown,
            browser::ui::native_browser_ui_state,
            browser::ui::native_browser_resolve_request,
            browser::ui::native_browser_command,
            restart_application,
            show_main_from_desktop_pet,
            desktop_pet_companion_display,
            floating::desktop_pet_resize,
            floating::desktop_pet_fullscreen_policy,
            floating::desktop_widget_set,
            desktop_perception::desktop_capture_screen,
            desktop_perception::desktop_cancel_capture,
            desktop_wallpaper::desktop_wallpaper_status,
            desktop_wallpaper::desktop_wallpaper_apply,
            desktop_wallpaper::desktop_wallpaper_playback,
            desktop_wallpaper::desktop_wallpaper_engine_preview,
            desktop_wallpaper::desktop_integration_status,
            desktop_wallpaper::desktop_rainmeter_invoke,
            show_desktop_pet_menu,
            floating::open_floating_surface,
            floating::resize_floating_surface,
            floating::position_floating_surface,
            floating::close_floating_surface,
            floating::hide_floating_surface
            ]}
        })
        .on_window_event(move |window, event| {
            if window.label() == "main"
                && matches!(event, tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed)
            {
                window.app_handle().state::<desktop_perception::CaptureSessions>().cancel();
            }
            if !software_bridge::window_event(window, event, &event_software_host) { return; }
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            } else if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if window.app_handle().tray_by_id("main-tray").is_some() {
                        if window.hide().is_ok() {
                            api.prevent_close();
                        }
                    }
                }
            } else if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let _ = window
                    .app_handle()
                    .emit_to("main", "floating:closed", window.label());
            }
        })
        .setup(move |app| {
            tracing::info!("desktop setup begin");
            browser::install(app.handle(),browser_runtime.clone()).map_err(std::io::Error::other)?;
            tracing::info!("desktop browser host installed");
            setup_tray(app)?;
            tracing::info!("desktop tray ready");
            if let Some(mut events) = companion_events {
                let desktop = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use tokio::sync::broadcast::error::RecvError;
                    loop {
                        match events.recv().await {
                            Ok(event) if event.event_type == "companion.profile_changed" => {},
                            Ok(_) => continue,
                            Err(RecvError::Lagged(_)) => {},
                            Err(RecvError::Closed) => break,
                        }
                        let _ = desktop.emit_to(floating::PET_LABEL, "companion:profile-changed", ());
                    }
                });
            }
            if fixture_main.is_none() {
                configure_initial_main_window(app.handle())?;
                tracing::info!("Tauri desktop window ready");
            }
            let handle = app.handle().clone();
            // Windows WebView2 creation must not block the UI event loop.
            std::thread::spawn(move || {
                if let Some((config,path))=fixture_main {
                    tracing::info!("native fixture main creation begin");
                    let result=tauri::WebviewWindowBuilder::from_config(&handle,&config).and_then(|builder|builder.data_directory(path).build());
                    match result {
                        Ok(window)=>{tracing::info!("native fixture main created with visible config");browser::fixture_diagnostics(window.as_ref());tracing::info!("native fixture controller diagnostics queued; no monitor geometry requested");},
                        Err(error)=>tracing::error!(error=%error,"native fixture main creation failed")
                    }
                }
                if floating::DESKTOP_PET_ENABLED {
                    floating::restore_desktop_pet(&handle);
                }
            });
            Ok(())
        })
        .build(context)
        .expect("error while building tauri application")
        .run(move |app, event| {
            match &event {
                tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::Destroyed, .. }
                    if floating::TRAY_WINDOW_LABELS.contains(&label.as_str()) =>
                {
                    refresh_floating_tray_item(app);
                }
                tauri::RunEvent::WindowEvent { label, event, .. }
                    if label == "main"
                        && matches!(event, tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::CloseRequested { .. }) =>
                {
                    main_tray_item_dirty = true;
                }
                tauri::RunEvent::MainEventsCleared if main_tray_item_dirty => {
                    main_tray_item_dirty = false;
                    refresh_main_window_tray_item(app);
                }
                _ => {}
            }
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                let exit = app.state::<DesktopExit>();
                if exit.prepared.load(Ordering::Acquire) {
                    return;
                }
                api.prevent_exit();
                if exit.begin() {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        match prepare_desktop_shutdown(&app).await {
                            Ok(()) => app.exit(code.unwrap_or(0)),
                            Err(error) => tracing::error!(%error, "Application exit preparation failed"),
                        }
                    });
                }
            }
        });
}

#[cfg(test)]
#[path = "startup_tests.rs"]
mod tests;
