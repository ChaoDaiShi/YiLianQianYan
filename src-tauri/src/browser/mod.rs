//! A native, temporary browser session shared by manual UI and approved AI actions.
pub(crate) mod credentials;
mod dispatch;
#[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
pub mod fixture;
mod initialization;
mod interactions;
mod lifecycle;
mod network;
mod platform;
mod popup;
mod presentation;
mod profiles;
mod shell;
pub use shell::scoped_shell;
mod scripts;
pub mod ui;
use async_trait::async_trait;
pub use platform::fixture_diagnostics;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
};
use tokio_util::sync::CancellationToken;
use yilian_backend::browser::{
    BrowserAction, BrowserError, BrowserHost, BrowserReply, BrowserRequest, BrowserRuntime,
    BrowserTab, BrowserTarget,
};

#[derive(Clone)]
struct TabState {
    context: Arc<profiles::BrowserContext>,
    entry_url: String,
    tab: BrowserTab,
    proxy: Arc<network::BrowserProxy>,
    profile: PathBuf,
    chrome_profile: PathBuf,
    observation: Option<String>,
    credential_filled: bool,
    visible: bool,
    ready: bool,
    presentation_sequence: Arc<AtomicU64>,
    presentation_owner: Arc<Mutex<String>>,
    presentation_gate: Arc<Mutex<()>>,
    disposal_gate: Arc<tokio::sync::Mutex<()>>,
}
#[derive(Clone)]
pub struct NativeBrowserHost {
    app: AppHandle,
    runtime: std::sync::Weak<BrowserRuntime>,
    interactions: Arc<interactions::BrowserInteractions>,
    profile_context: Option<yilian_backend::browser::BrowserProfileContext>,
    persistent_contexts: Arc<tokio::sync::Mutex<HashMap<PathBuf, Arc<profiles::BrowserContext>>>>,
    tabs: Arc<Mutex<HashMap<String, TabState>>>,
    sequence: Arc<AtomicU64>,
    gate: Arc<tokio::sync::Mutex<()>>,
    closing: Arc<AtomicBool>,
    pending_workers: Arc<AtomicUsize>,
    retiring: Arc<Mutex<HashMap<String, TabState>>>,
}
struct NativeWorkGuard(Arc<AtomicUsize>);
impl Drop for NativeWorkGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
fn error(code: &str, message: impl Into<String>) -> BrowserError {
    BrowserError::new(code, message)
}

fn remove_temporary_profile(path: &std::path::Path) -> std::io::Result<()> {
    let temp = std::env::temp_dir().canonicalize()?;
    let actual = path.canonicalize()?;
    let expected_name = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        n.starts_with("yilian-browser-browser-session-")
            || n.starts_with("yilian-browser-trusted-chrome-")
    });
    let metadata = std::fs::symlink_metadata(path)?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = metadata.file_type().is_symlink();
    if !expected_name || actual.parent() != Some(temp.as_path()) || reparse {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Temporary profile path failed ownership checks",
        ));
    }
    std::fs::remove_dir_all(actual)
}

/// Never authorize by caller-provided label, site title, URL, or workspace ID.
pub fn trusted_caller(view: &Webview) -> Result<(), String> {
    let label = view.label();
    if !(matches!(label, "main" | "call-overlay" | "monitor-overlay")
        || label.starts_with("browser-shell-"))
    {
        return Err("This command is restricted to the trusted application UI".into());
    }
    trusted_app_origin(view)
}

pub(crate) fn trusted_app_origin(view: &Webview) -> Result<(), String> {
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

/// Only the feature-gated native test artifact may use this profile override.
pub fn fixture_root() -> Option<PathBuf> {
    if !cfg!(all(debug_assertions, feature = "native-browser-fixture")) {
        return None;
    }
    let root = PathBuf::from(std::env::var_os("YILIAN_NATIVE_FIXTURE_ROOT")?)
        .canonicalize()
        .ok()?;
    let temp = std::env::temp_dir().canonicalize().ok()?;
    (root != temp && root.starts_with(&temp)).then_some(root)
}

pub fn install(app: &AppHandle, runtime: Arc<BrowserRuntime>) -> Result<(), String> {
    if !cfg!(windows) {
        return Ok(());
    }

    let host = Arc::new(NativeBrowserHost {
        app: app.clone(),
        runtime: Arc::downgrade(&runtime),
        interactions: Arc::new(interactions::BrowserInteractions::default()),
        profile_context: runtime.profile_context().cloned(),
        persistent_contexts: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        tabs: Arc::new(Mutex::new(HashMap::new())),
        sequence: Arc::new(AtomicU64::new(1)),
        gate: Arc::new(tokio::sync::Mutex::new(())),
        closing: Arc::new(AtomicBool::new(false)),
        pending_workers: Arc::new(AtomicUsize::new(0)),
        retiring: Arc::new(Mutex::new(HashMap::new())),
    });
    app.manage(host.clone());
    runtime.install_host(host).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn native_browser_execute(
    view: Webview,
    runtime: tauri::State<'_, Arc<BrowserRuntime>>,
    mut request: BrowserRequest,
) -> Result<BrowserReply, String> {
    trusted_caller(&view)?;
    request.owner = None;
    if let Some(payload) = request.payload.as_object_mut() {
        payload.remove("_source_tool_call_id");
        payload.remove("_native_authorization_epoch");
        payload.remove("_presenter_window");
        payload.remove("_native_popup_id");
        payload.remove("_native_parent_tab_id");
        payload.remove("_native_popup_blank");
        if request.action == BrowserAction::Present {
            payload.insert("_presenter_window".into(), json!(view.window().label()));
        }
    }
    runtime
        .execute(request, CancellationToken::new())
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn native_browser_tabs(
    view: Webview,
    runtime: tauri::State<'_, Arc<BrowserRuntime>>,
    host: tauri::State<'_, Arc<NativeBrowserHost>>,
) -> Result<Vec<BrowserTab>, String> {
    trusted_caller(&view)?;
    if runtime.is_quiesced() {
        return Ok(Vec::new());
    }
    Ok(host.tab_views(false))
}
#[tauri::command]
pub async fn native_browser_unknown(
    view: Webview,
    runtime: tauri::State<'_, Arc<BrowserRuntime>>,
) -> Result<Value, String> {
    trusted_caller(&view)?;
    serde_json::to_value(runtime.unknown_operations()).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn native_browser_acknowledge_unknown(
    view: Webview,
    runtime: tauri::State<'_, Arc<BrowserRuntime>>,
    operation_id: String,
) -> Result<(), String> {
    trusted_caller(&view)?;
    runtime
        .acknowledge_unknown(&operation_id)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Test-only fault injection can only REMOVE an existing session's network access.
/// It cannot create a session, grant a path, evaluate script, or affect another process.
#[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
#[tauri::command]
pub async fn native_browser_fixture_boundary(
    view: Webview,
    host: tauri::State<'_, Arc<NativeBrowserHost>>,
    target: BrowserTarget,
    action: String,
    url: Option<String>,
) -> Result<Value, String> {
    trusted_caller(&view)?;
    let state = host
        .lookup_control(Some(&target))
        .map_err(|e| e.to_string())?;
    match action.as_str() {
        "shutdown_proxy" => {
            state.proxy.shutdown();
            host.update(&state.tab.tab_id,|s|s.tab.error=Some("Browser network boundary is unavailable. Close this temporary session to start another.".into()));
        }
        "revoke_local_origin" => state
            .proxy
            .revoke_local_origin(url.as_deref().ok_or("A local origin is required")?)?,
        _ => return Err("Unsupported native browser fixture fault".into()),
    }
    Ok(json!({"network_access_removed":true,"tab_id":state.tab.tab_id}))
}

#[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
#[tauri::command]
pub async fn native_browser_fixture_quiesce(
    view: Webview,
    runtime: tauri::State<'_, Arc<BrowserRuntime>>,
) -> Result<(), String> {
    trusted_caller(&view)?;
    runtime
        .quiesce(Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())
}

impl NativeBrowserHost {
    fn tab_views(&self, redact_login_pages: bool) -> Vec<BrowserTab> {
        if self.closing.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut tabs: Vec<_> = self
            .tabs
            .lock()
            .unwrap()
            .values()
            .filter(|state| state.ready)
            .map(|state| {
                let mut tab = state.tab.clone();
                if redact_login_pages && state.credential_filled {
                    tab.url = tauri::Url::parse(&tab.url)
                        .map(|url| url.origin().ascii_serialization())
                        .unwrap_or_default();
                    tab.title = "登录页面".into();
                    tab.error = None;
                }
                tab
            })
            .collect();
        tabs.sort_by(|a, b| a.tab_id.cmp(&b.tab_id));
        tabs
    }

    fn id(&self, prefix: &str) -> String {
        format!(
            "{prefix}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            self.sequence.fetch_add(1, Ordering::Relaxed)
        )
    }
    fn lookup(&self, target: Option<&BrowserTarget>) -> Result<TabState, BrowserError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(error(
                "permission_denied",
                "Browser sessions were revoked; restart the application",
            ));
        }
        let target = target.ok_or_else(|| error("invalid_target", "Browser target is required"))?;
        let tab = self
            .tabs
            .lock()
            .unwrap()
            .get(&target.tab_id)
            .cloned()
            .ok_or_else(|| error("tab_not_found", "Browser tab no longer exists"))?;
        if !tab.ready {
            return Err(error(
                "tab_initializing",
                "Browser initialization has not completed",
            ));
        }
        if tab.tab.session_id != target.session_id {
            return Err(error("session_mismatch", "Browser session does not match"));
        }
        if tab.tab.navigation_id != target.navigation_id {
            return Err(error(
                "stale_navigation",
                "The page changed; inspect the current tab before acting",
            ));
        }
        Ok(tab)
    }
    fn view(&self, tab: &TabState) -> Result<Webview, BrowserError> {
        self.app
            .get_webview(&tab.tab.tab_id)
            .ok_or_else(|| error("tab_not_found", "Native browser view is closed"))
    }
    fn update(&self, id: &str, change: impl FnOnce(&mut TabState)) {
        if let Some(tab) = self.tabs.lock().unwrap().get_mut(id) {
            change(tab);
        }
    }
    async fn on_worker<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, BrowserError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(error(
                "permission_denied",
                "Browser sessions were revoked; restart the desktop application",
            ));
        }
        self.pending_workers.fetch_add(1, Ordering::AcqRel);
        let guard = NativeWorkGuard(self.pending_workers.clone());
        let closing = self.closing.clone();
        tokio::time::timeout(
            Duration::from_secs(15),
            tokio::task::spawn_blocking(move || {
                let _guard = guard;
                if closing.load(Ordering::Acquire) {
                    return Err("Browser sessions were revoked before native dispatch".into());
                }
                f()
            }),
        )
        .await
        .map_err(|_| error("native_timeout", "Native browser UI operation timed out"))?
        .map_err(|_| error("native_closed", "Native browser worker was interrupted"))?
        .map_err(|reason| error("native_operation_failed", reason))
    }
    async fn ensure_visible(
        &self,
        state: TabState,
        target: &BrowserTarget,
        cancel: &CancellationToken,
    ) -> Result<TabState, BrowserError> {
        let view = self.view(&state)?;
        if state.visible
            && view.window().is_visible().unwrap_or(false)
            && !view.window().is_minimized().unwrap_or(true)
        {
            return Ok(state);
        }
        if state.tab.mode == "detached" {
            let window = view.window();
            let tab_id = state.tab.tab_id.clone();
            let closing = self.closing.clone();
            let visible_cancel = cancel.clone();
            self.on_worker(move || {
                if visible_cancel.is_cancelled() {
                    return Err("cancelled_before_execution".into());
                }
                if closing.load(Ordering::Acquire) {
                    return Err("Browser session revoked".into());
                }
                window.show().map_err(|e| e.to_string())?;
                window.unminimize().map_err(|e| e.to_string())?;
                window.set_focus().map_err(|e| e.to_string())?;
                view.show().map_err(|e| e.to_string())?;
                Ok(())
            })
            .await?;
            self.update(&tab_id, |s| s.visible = true);
        } else {
            self.app
                .emit_to("main", "browser:opened", json!({"tab_id":state.tab.tab_id}))
                .map_err(|e| error("native_dispatch_failed", e.to_string()))?;
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            if cancel.is_cancelled() {
                return Err(error(
                    "cancelled_before_execution",
                    "Browser action cancelled before the original page was presented",
                ));
            }
            let current = self.lookup(Some(target))?;
            let view = self.view(&current)?;
            if current.visible
                && view.window().is_visible().unwrap_or(false)
                && !view.window().is_minimized().unwrap_or(true)
            {
                return Ok(current);
            }
            if std::time::Instant::now() > deadline {
                return Err(error("invalid_visibility","The original browser page could not be presented; no page action was dispatched"));
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    }
    async fn eval(
        &self,
        view: &Webview,
        script: String,
        cancel: &CancellationToken,
    ) -> Result<Value, BrowserError> {
        let value = tokio::select! {biased;_=cancel.cancelled()=>return Err(error("native_cancelled","Browser action interrupted; its result may be unknown")),v=platform::evaluate(view,script,cancel.clone())=>v.map_err(|e|error("native_script_failed",e))?};
        if value.get("ok") == Some(&Value::Bool(false)) {
            return Err(error(
                if value.get("no_effect") == Some(&Value::Bool(true)) {
                    "stale_observation"
                } else {
                    "native_action_unknown"
                },
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Browser observation is stale"),
            ));
        }
        Ok(value)
    }

    fn lookup_control(&self, target: Option<&BrowserTarget>) -> Result<TabState, BrowserError> {
        let target = target.ok_or_else(|| error("invalid_target", "Browser target is required"))?;
        let state = self
            .tabs
            .lock()
            .unwrap()
            .get(&target.tab_id)
            .cloned()
            .ok_or_else(|| error("tab_not_found", "Browser tab no longer exists"))?;
        if !state.ready {
            return Err(error(
                "tab_initializing",
                "Browser initialization has not completed",
            ));
        }
        if state.tab.session_id != target.session_id {
            return Err(error("session_mismatch", "Browser session does not match"));
        }
        Ok(state)
    }
}
