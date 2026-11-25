//! Trusted browser interaction requests. Native event objects stay on the UI thread.
use super::*;
use serde::Serialize;
use std::collections::BTreeMap;

#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod native;

#[derive(Clone, Serialize)]
pub(super) struct BrowserUiRequest {
    pub id: String,
    pub kind: String,
    pub tab_id: String,
    pub session_id: String,
    pub navigation_id: u64,
    pub url: Option<String>,
    pub message: String,
    pub suggested_name: Option<String>,
}

impl BrowserUiRequest {
    fn target(&self) -> BrowserTarget {
        BrowserTarget {
            session_id: self.session_id.clone(),
            tab_id: self.tab_id.clone(),
            navigation_id: self.navigation_id,
        }
    }
}

#[derive(Default)]
pub(super) struct BrowserInteractions {
    requests: Mutex<BTreeMap<String, BrowserUiRequest>>,
    zoom: Mutex<BTreeMap<String, f64>>,
}

impl BrowserInteractions {
    async fn cancel_requests(&self, app: &AppHandle, tab_id: &str) -> Result<(), String> {
        self.requests
            .lock()
            .unwrap()
            .retain(|_, r| r.tab_id != tab_id);
        #[cfg(windows)]
        native::cancel(app, Some(tab_id.to_string())).await?;
        Ok(())
    }

    pub async fn cancel_tab(&self, app: &AppHandle, tab_id: &str) -> Result<(), String> {
        self.cancel_requests(app, tab_id).await?;
        self.zoom.lock().unwrap().remove(tab_id);
        Ok(())
    }

    pub async fn cancel_all(&self, app: &AppHandle) -> Result<(), String> {
        self.requests.lock().unwrap().clear();
        self.zoom.lock().unwrap().clear();
        #[cfg(windows)]
        native::cancel(app, None).await?;
        Ok(())
    }
}

impl NativeBrowserHost {
    pub fn ui_state(&self) -> Value {
        json!({
            "requests": self.interactions.requests.lock().unwrap().values().cloned().collect::<Vec<_>>(),
            "zoom": self.interactions.zoom.lock().unwrap().clone(),
        })
    }

    fn interaction_changed(&self, tab_id: &str) {
        let _ = self
            .app
            .emit_to("main", "browser:ui-changed", json!({"tab_id":tab_id}));
        if let Some(view) = self.app.get_webview(tab_id) {
            let window = view.window();
            if window.label() != "main" {
                let _ = self.app.emit_to(
                    window.label(),
                    "browser:ui-changed",
                    json!({"tab_id":tab_id}),
                );
            }
        }
    }

    fn interaction_request(
        &self,
        tab_id: &str,
        kind: &str,
        url: Option<String>,
        message: String,
        suggested_name: Option<String>,
    ) -> Option<BrowserUiRequest> {
        if self.closing.load(Ordering::Acquire) {
            return None;
        }
        let tab = self.tabs.lock().unwrap().get(tab_id)?.tab.clone();
        let request = BrowserUiRequest {
            id: self.id("browser-request"),
            kind: kind.into(),
            tab_id: tab.tab_id,
            session_id: tab.session_id,
            navigation_id: tab.navigation_id,
            url,
            message,
            suggested_name,
        };
        let mut pending = self.interactions.requests.lock().unwrap();
        if pending
            .values()
            .filter(|request| request.tab_id == tab_id)
            .count()
            >= 8
        {
            return None;
        }
        pending.insert(request.id.clone(), request.clone());
        Some(request)
    }

    async fn open_interaction_popup(&self, request: BrowserUiRequest) -> Result<Value, String> {
        let result = self
            .open_popup_request(
                request.id.clone(),
                request.target(),
                request.url.clone().unwrap_or_default(),
            )
            .await;
        self.interactions
            .requests
            .lock()
            .unwrap()
            .remove(&request.id);
        self.interaction_changed(&request.tab_id);
        match result {
            Ok(()) => Ok(json!({"opened":true})),
            Err(reason) => {
                #[cfg(windows)]
                let _ = native::deny(&self.app, request.id).await;
                Err(reason.to_string())
            }
        }
    }

    pub async fn resolve_request(&self, id: String, decision: String) -> Result<Value, String> {
        let request = self
            .interactions
            .requests
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or("Browser request no longer exists")?;
        if !valid_decision(&request.kind, &decision) {
            return Err("Unsupported browser request decision".into());
        }
        // Dismissal remains possible even when the requesting document has changed.
        if decision != "deny" {
            self.lookup(Some(&request.target()))
                .map_err(|e| e.to_string())?;
        }
        if decision == "open" {
            return self.open_interaction_popup(request).await;
        }
        #[cfg(windows)]
        {
            let result = native::resolve(self, &request, &decision).await?;
            if result.get("cancelled").and_then(Value::as_bool) != Some(true) {
                self.interactions.requests.lock().unwrap().remove(&id);
                self.interaction_changed(&request.tab_id);
            }
            Ok(result)
        }
        #[cfg(not(windows))]
        {
            Err("Native browser interactions require Windows".into())
        }
    }

    pub async fn browser_command(
        &self,
        target: BrowserTarget,
        command: String,
        payload: Value,
    ) -> Result<Value, String> {
        if command == "clear_data" {
            let _guard = self.gate.lock().await;
            let state = self.lookup(Some(&target)).map_err(|e| e.to_string())?;
            let view = self.view(&state).map_err(|e| e.to_string())?;
            return self.clear_profile_data(state, &view).await;
        }
        let state = self.lookup(Some(&target)).map_err(|e| e.to_string())?;
        if command == "new_tab" {
            return self.new_tab_in_main(&state.tab.tab_id).await;
        }
        let view = self.view(&state).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            let result = commands::execute(&view, &command, payload).await?;
            if let Some(factor) = result.get("factor").and_then(Value::as_f64) {
                self.interactions
                    .zoom
                    .lock()
                    .unwrap()
                    .insert(target.tab_id.clone(), factor);
                self.interaction_changed(&target.tab_id);
            }
            Ok(result)
        }
        #[cfg(not(windows))]
        {
            Err("Native browser interactions require Windows".into())
        }
    }

    pub(super) async fn new_tab_in_main(&self, source_tab: &str) -> Result<Value, String> {
        let window = self
            .app
            .get_window("main")
            .ok_or("Main browser window is unavailable")?;
        let chrome = self
            .app
            .get_webview("main")
            .ok_or("Main browser toolbar is unavailable")?;
        window
            .show()
            .and_then(|_| window.unminimize())
            .and_then(|_| window.set_focus())
            .map_err(|e| e.to_string())?;
        platform::focus_native(&chrome).await?;
        self.app
            .emit_to("main", "browser:new-tab", json!({"tab_id":source_tab}))
            .map_err(|e| e.to_string())?;
        Ok(json!({"new_tab_requested":true,"window":"main"}))
    }

    async fn clear_profile_data(&self, state: TabState, view: &Webview) -> Result<Value, String> {
        #[cfg(not(windows))]
        {
            let _ = (state, view);
            return Err("Native browser interactions require Windows".into());
        }
        #[cfg(windows)]
        {
            let siblings = self
                .tabs
                .lock()
                .unwrap()
                .values()
                .filter(|other| other.profile == state.profile)
                .cloned()
                .collect::<Vec<_>>();
            // Discard the live documents before clearing their shared profile. Otherwise
            // a sibling's still-running script could immediately recreate local storage.
            for tab in &siblings {
                self.interactions
                    .cancel_requests(&self.app, &tab.tab.tab_id)
                    .await?;
                let sibling_view = self.view(tab).map_err(|e| e.to_string())?;
                platform::navigation(
                    &sibling_view,
                    "navigate",
                    Some("about:blank".into()),
                    None,
                    CancellationToken::new(),
                )
                .await?;
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                loop {
                    let frame =
                        platform::cdp(&sibling_view, "Page.getFrameTree", json!({})).await?;
                    if frame
                        .pointer("/frameTree/frame/url")
                        .and_then(Value::as_str)
                        == Some("about:blank")
                    {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(
                            "A related browser document is still active; data was not cleared"
                                .into(),
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                self.update(&tab.tab.tab_id, |s| s.observation = None);
            }
            let result = commands::execute(view, "clear_data", json!({})).await?;
            let mut reload_errors = Vec::new();
            for tab in &siblings {
                if tab.tab.url == "about:blank" {
                    continue;
                }
                let reloaded = match self.view(tab) {
                    Ok(view) => {
                        platform::navigation(
                            &view,
                            "navigate",
                            Some(tab.tab.url.clone()),
                            Some(tab.proxy.clone()),
                            CancellationToken::new(),
                        )
                        .await
                    }
                    Err(reason) => Err(reason.to_string()),
                };
                if let Err(reason) = reloaded {
                    reload_errors.push(json!({"tab_id":tab.tab.tab_id,"error":reason}));
                }
            }
            Ok(
                json!({"cleared":result.get("cleared").and_then(Value::as_bool)==Some(true),"affected_tabs":siblings.iter().map(|s|s.tab.tab_id.clone()).collect::<Vec<_>>(),"reload_errors":reload_errors}),
            )
        }
    }
}

fn valid_decision(kind: &str, decision: &str) -> bool {
    decision == "deny"
        || matches!(
            (kind, decision),
            ("popup", "open") | ("download", "save") | ("permission", "allow")
        )
}

#[cfg(windows)]
pub(super) use native::{attach_popup, install, popup_environment};

#[cfg(not(windows))]
pub(super) async fn install(
    _view: &Webview,
    _host: NativeBrowserHost,
    _tab_id: String,
) -> Result<(), String> {
    Err("Native browser interactions require Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_decisions_are_specific_to_the_native_operation() {
        for kind in ["popup", "download", "permission"] {
            assert!(valid_decision(kind, "deny"));
        }
        assert!(valid_decision("popup", "open"));
        assert!(valid_decision("download", "save"));
        assert!(valid_decision("permission", "allow"));
        assert!(!valid_decision("download", "allow"));
        assert!(!valid_decision("permission", "open"));
        assert!(!valid_decision("popup", "save"));
    }
}
