//! Bounded scheduling probes for the explicitly selected local WebView2 fixture binary.
use super::*;
use std::sync::{Condvar, OnceLock};
#[cfg(windows)]
mod cookies;
mod input;

#[derive(Default)]
struct Probe {
    key: String,
    hold_ms: u64,
    entered: bool,
    released: bool,
    exited: bool,
    tab_id: Option<String>,
}
#[derive(Default)]
struct Probes {
    stages: HashMap<String, Probe>,
    operations: HashMap<String, CancellationToken>,
    results: HashMap<String, Value>,
    native_closed: Vec<String>,
}
fn probes() -> &'static (Mutex<Probes>, Condvar) {
    static VALUE: OnceLock<(Mutex<Probes>, Condvar)> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}

pub(super) fn record_native_closed(label: &str) {
    probes()
        .0
        .lock()
        .unwrap()
        .native_closed
        .push(label.to_string());
}

pub(super) fn pause(stage: &str, key: &str, tab_id: &str) {
    let (lock, changed) = probes();
    let mut state = lock.lock().unwrap();
    let Some(probe) = state.stages.get_mut(stage) else {
        return;
    };
    if probe.key != key || probe.entered {
        return;
    }
    probe.entered = true;
    probe.tab_id = Some(tab_id.into());
    let deadline = std::time::Instant::now() + Duration::from_millis(probe.hold_ms);
    loop {
        if state.stages.get(stage).is_none_or(|p| p.released) {
            break;
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        state = changed.wait_timeout(state, remaining).unwrap().0;
    }
    if let Some(p) = state.stages.get_mut(stage) {
        p.exited = true;
    }
}

#[tauri::command]
pub async fn native_browser_fixture_schedule(
    view: Webview,
    host: tauri::State<'_, Arc<NativeBrowserHost>>,
    runtime: tauri::State<'_, Arc<BrowserRuntime>>,
    action: String,
    stage: Option<String>,
    key: Option<String>,
    hold_ms: Option<u64>,
    request: Option<BrowserRequest>,
) -> Result<Value, String> {
    trusted_caller(&view)?;
    let (lock, changed) = probes();
    match action.as_str() {
        #[cfg(windows)]
        "cookie_metadata" => {
            return cookies::metadata(&host, request.ok_or("Owned request required")?).await
        }
        "human_input" => {
            return input::input(&host, request.ok_or("Owned request required")?, key).await
        }
        "move_main_for_mount" if view.label() == "main" => {
            let window = view.window();
            let origin = window.outer_position().map_err(|e| e.to_string())?;
            let next = tauri::PhysicalPosition::new(origin.x + 24, origin.y + 16);
            window.set_position(next).map_err(|e| e.to_string())?;
            let actual = window.outer_position().map_err(|e| e.to_string())?;
            return Ok(json!({"dx": actual.x - origin.x, "dy": actual.y - origin.y}));
        }
        "exit" => {
            let app = view.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                app.exit(0);
            });
            return Ok(json!({"exit_requested":true}));
        }
        "arm" => {
            let stage = stage.ok_or("stage required")?;
            if !matches!(
                stage.as_str(),
                "creation_registered" | "presentation_show" | "shell_creating" | "disposal_ui"
            ) {
                return Err("Unsupported stage".into());
            }
            let key = key.ok_or("key required")?;
            let hold_ms = hold_ms.unwrap_or(20000).clamp(1, 30000);
            lock.lock().unwrap().stages.insert(
                stage.clone(),
                Probe {
                    key: key.clone(),
                    hold_ms,
                    ..Default::default()
                },
            );
            if stage == "disposal_ui" {
                let tab = host
                    .tabs
                    .lock()
                    .unwrap()
                    .get(&key)
                    .cloned()
                    .ok_or("Owned tab required")?;
                let app = host.app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    let _ =
                        app.run_on_main_thread(move || pause("disposal_ui", &key, &tab.tab.tab_id));
                });
            }
        }
        "release" => {
            if let Some(p) = lock
                .lock()
                .unwrap()
                .stages
                .get_mut(&stage.ok_or("stage required")?)
            {
                p.released = true;
            }
            changed.notify_all();
        }
        "start_open" => {
            let mut request = request.ok_or("request required")?;
            if request.action != BrowserAction::Open {
                return Err("Only Open is supported".into());
            }
            request.owner = None;
            if let Some(p) = request.payload.as_object_mut() {
                p.remove("_source_tool_call_id");
                p.remove("_native_authorization_epoch");
            }
            let id = request.operation_id.clone();
            let token = CancellationToken::new();
            lock.lock()
                .unwrap()
                .operations
                .insert(id.clone(), token.clone());
            let runtime = runtime.inner().clone();
            tauri::async_runtime::spawn(async move {
                let result = runtime.execute(request, token).await;
                let value = match result {
                    Ok(reply) => json!({"reply":reply}),
                    Err(e) => json!({"error":e.to_string()}),
                };
                probes().0.lock().unwrap().results.insert(id, value);
            });
        }
        "cancel_open" => {
            lock.lock()
                .unwrap()
                .operations
                .get(&key.ok_or("key required")?)
                .ok_or("Unknown fixture operation")?
                .cancel();
        }
        "start_quiesce" => {
            lock.lock().unwrap().results.remove("quiesce");
            let runtime = runtime.inner().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(600)).await;
                let start = std::time::Instant::now();
                let result = runtime.quiesce(Duration::from_secs(5)).await;
                let mut state = probes().0.lock().unwrap();
                let active = state
                    .stages
                    .get("disposal_ui")
                    .is_some_and(|p| p.entered && !p.exited);
                state.results.insert("quiesce".into(),json!({"ok":result.is_ok(),"error":result.err().map(|e|e.to_string()),"elapsed_ms":start.elapsed().as_millis(),"ui_pause_active":active}));
            });
        }
        "status" => {}
        _ => return Err("Unsupported fixture action".into()),
    }
    let state = lock.lock().unwrap();
    let stages: HashMap<_,_> = state.stages.iter().map(|(name,p)| (name.clone(),json!({"key":p.key,"entered":p.entered,"released":p.released,"exited":p.exited,"tab_id":p.tab_id}))).collect();
    let tabs: Vec<_> = host
        .tabs
        .lock()
        .unwrap()
        .values()
        .map(|s| json!({"tab":s.tab,"visible":s.visible,"ready":s.ready}))
        .collect();
    Ok(
        json!({"stages":stages,"results":state.results,"owned_tabs":tabs,"native_closed":state.native_closed,"shell_labels":host.app.windows().keys().filter(|s|s.starts_with("browser-shell-")).collect::<Vec<_>>(),"pending_workers":host.pending_workers.load(Ordering::Acquire),"content_labels":host.app.webviews().keys().filter(|s|s.starts_with("browser-content-")).collect::<Vec<_>>() }),
    )
}

#[cfg(windows)]
#[tauri::command]
pub async fn native_browser_fixture_native_state(
    view: Webview,
    host: tauri::State<'_, Arc<NativeBrowserHost>>,
    tab_id: String,
) -> Result<Value, String> {
    trusted_caller(&view)?;
    if !tab_id.starts_with("browser-content-") {
        return Err("Owned content label required".into());
    }
    let Some(view) = host.app.get_webview(&tab_id) else {
        return Ok(json!({"registered":false}));
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let result=(|| -> windows::core::Result<Value> { unsafe {
            use windows::core::Interface;
            let controller=native.controller();
            let mut visible=windows::core::BOOL::default();
            controller.IsVisible(&mut visible)?;
            let settings=controller.CoreWebView2()?.Settings()?;
            let mut messages=windows::core::BOOL::default();
            settings.IsWebMessageEnabled(&mut messages)?;
            let mut hosts=windows::core::BOOL::default();
            settings.AreHostObjectsAllowed(&mut hosts)?;
            let profile=controller.CoreWebView2()?.cast::<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_13>()?.Profile()?;
            let mut private=windows::core::BOOL::default();
            profile.IsInPrivateModeEnabled(&mut private)?;
            let mut path=windows::core::PWSTR::null();
            profile.ProfilePath(&mut path)?;
            let profile_path=webview2_com::take_pwstr(path);
            let mut browser_process_id=0;
            controller.CoreWebView2()?.BrowserProcessId(&mut browser_process_id)?;
            let mut context_menus=windows::core::BOOL::default();
            settings.AreDefaultContextMenusEnabled(&mut context_menus)?;
            let mut script_dialogs=windows::core::BOOL::default();
            settings.AreDefaultScriptDialogsEnabled(&mut script_dialogs)?;
            Ok(json!({"registered":true,"native_visible":visible.as_bool(),"web_messages":messages.as_bool(),"host_objects":hosts.as_bool(),"in_private":private.as_bool(),"profile_path":profile_path,"browser_process_id":browser_process_id,"context_menus":context_menus.as_bool(),"script_dialogs":script_dialogs.as_bool()}))
        }})();
        let _=tx.send(result.map_err(|e|e.to_string()));
    }).map_err(|e|e.to_string())?;
    tokio::time::timeout(Duration::from_secs(3), rx)
        .await
        .map_err(|_| "Native snapshot timeout")?
        .map_err(|_| "Native snapshot closed")?
}
