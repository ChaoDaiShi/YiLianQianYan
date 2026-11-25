//! Per-tab presentation ordering, independent of document actions.
use super::*;

impl NativeBrowserHost {
    pub(super) async fn present(
        &self,
        state: TabState,
        payload: &Value,
    ) -> Result<BrowserReply, BrowserError> {
        let mode = payload
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or(&state.tab.mode)
            .to_string();
        if !matches!(mode.as_str(), "embedded" | "detached" | "hidden") {
            return Err(error("invalid_mode", "Unknown browser display mode"));
        }
        let view = self.view(&state)?;
        let x = payload
            .get("x")
            .and_then(Value::as_f64)
            .unwrap_or(if mode == "detached" { 0.0 } else { 90.0 })
            .clamp(0.0, 10000.0);
        let y = payload
            .get("y")
            .and_then(Value::as_f64)
            .unwrap_or(130.0)
            .clamp(0.0, 10000.0);
        let width = payload
            .get("width")
            .and_then(Value::as_f64)
            .unwrap_or(900.0)
            .clamp(64.0, 10000.0);
        let height = payload
            .get("height")
            .and_then(Value::as_f64)
            .unwrap_or(600.0)
            .clamp(64.0, 10000.0);
        let app = self.app.clone();
        let tab_id = state.tab.tab_id.clone();
        let close_host = self.clone();
        let close_id = tab_id.clone();
        let dest_mode = mode.clone();
        let should_focus = !state.visible || state.tab.mode != mode;
        let present_closing = self.closing.clone();
        let chrome_profile = state.chrome_profile.clone();
        let presenter_window = payload
            .get("_presenter_window")
            .and_then(Value::as_str)
            .map(str::to_string);
        let intent = {
            // Ownership changes when an intent is accepted, before a detached shell exists.
            let mut owner = state.presentation_owner.lock().unwrap();
            if mode == "hidden"
                && presenter_window
                    .as_deref()
                    .is_some_and(|caller| caller != owner.as_str())
            {
                return Ok(BrowserReply::verified(
                    json!({"tab":state.tab,"ignored_stale_presenter":true}),
                ));
            }
            if mode != "hidden" {
                *owner = if mode == "detached" {
                    format!("browser-shell-{tab_id}")
                } else {
                    "main".into()
                };
            }
            state.presentation_sequence.fetch_add(1, Ordering::AcqRel) + 1
        };
        let sequence = state.presentation_sequence.clone();
        let presentation_gate = state.presentation_gate.clone();
        let apply_host = self.clone();
        tracing::info!(tab_id=%state.tab.tab_id,mode=%mode,"native browser presentation queued");
        let applied=self.on_worker(move || {
            if dest_mode == "hidden" {
                let _presentation = presentation_gate.lock().unwrap();
                if sequence.load(Ordering::Acquire) != intent || !apply_host.tabs.lock().unwrap().contains_key(&tab_id) { return Ok(false); }
                view.hide().map_err(|e|e.to_string())?;
                apply_host.update(&tab_id, |s|s.visible=false);
                return Ok(true);
            }
            let destination = if dest_mode == "detached" {
                let label = format!("browser-shell-{tab_id}");
                if let Some(window) = app.get_window(&label) {
                    window
                } else {
                    let builder = tauri::WebviewWindowBuilder::new(
                        &app,
                        &label,
                        WebviewUrl::App(format!("browser/window?tab={tab_id}").into()),
                    )
                    .title("忆涟千言 · 浏览器")
                    .visible(false)
                    .inner_size(1100.0, 800.0)
                    .min_inner_size(640.0, 480.0);
                    let builder = builder.data_directory(chrome_profile);
                    tracing::info!(tab_id=%tab_id,"native browser detached shell build begin");
                    #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
                    fixture::pause("shell_creating", &tab_id, &tab_id);
                    let shell = builder.build().map_err(|e| e.to_string())?;
                    tracing::info!(tab_id=%tab_id,"native browser detached shell build returned");
                    let window = shell.as_ref().window();
                    let event_app = app.clone();
                    let event_window = window.clone();
                    window.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let app = event_app.clone();
                            let id = close_id.clone();
                            let host = close_host.clone();
                            let shell = event_window.clone();
                            tauri::async_runtime::spawn(async move {
                                if let (Some(view), Some(main)) =
                                    (app.get_webview(&id), app.get_window("main"))
                                {
                                    // A delayed close of an old shell must not hide a tab already moved elsewhere.
                                    if view.window().label()==shell.label(){
                                        let _=view.hide();host.update(&id,|s|s.visible=false);
                                        let moved=if view.window().label()==main.label(){true}else{let _=view.bounds();view.reparent(&main).is_ok()};
                                        host.update(&id,|s|{if moved{s.tab.mode="embedded".into()}else{s.tab.error=Some("The window could not be reattached. Its existing page was retained; inspect the session before continuing.".into());}});
                                    }
                                }
                                let _ = shell.hide();
                            });
                        }
                    });
                    window
                }
            } else {
                app.get_window("main").ok_or("Main window is unavailable")?
            };
            if present_closing.load(Ordering::Acquire) || !apply_host.tabs.lock().unwrap().contains_key(&tab_id) {
                tokio::runtime::Handle::current().block_on(apply_host.dispose_views(&tab_id)).map_err(|e|e.to_string())?;
                return Err("Browser session revoked before presentation".into());
            }
            #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
            fixture::pause("presentation_show", &tab_id, &tab_id);
            let _presentation = presentation_gate.lock().unwrap();
            if sequence.load(Ordering::Acquire) != intent || !apply_host.tabs.lock().unwrap().contains_key(&tab_id) { return Ok(false); }
            if view.window().label() != destination.label() {
                tracing::info!(tab_id=%tab_id,"native browser different-parent hide/defocus begin");
                view.hide().map_err(|e| e.to_string())?;
                let _ = view.bounds().map_err(|e| e.to_string())?;
                destination.show().map_err(|e|e.to_string())?;
                let chrome = app.get_webview(destination.label()).ok_or("Trusted destination chrome is unavailable")?;
                tracing::info!(tab_id=%tab_id,"native browser trusted chrome focus acknowledgement begin");
                tokio::runtime::Handle::current().block_on(platform::focus_native(&chrome))?;
                tracing::info!(tab_id=%tab_id,"native browser trusted chrome focus acknowledged");
                tracing::info!(tab_id=%tab_id,"native browser different-parent reparent begin");
                view.reparent(&destination).map_err(|e| e.to_string())?;
                tracing::info!(tab_id=%tab_id,"native browser different-parent reparent returned");
            }
            tracing::info!(tab_id=%tab_id,"native browser bounds update begin");
            view.set_position(LogicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
            view.set_size(LogicalSize::new(width, height))
                .map_err(|e| e.to_string())?;
            tracing::info!(tab_id=%tab_id,"native browser bounds updated; parent show begin");
            destination.show().map_err(|e| e.to_string())?;
            tracing::info!(tab_id=%tab_id,"native browser parent shown");
            if should_focus {
                tracing::info!(tab_id=%tab_id,"native browser focus begin");
                destination.unminimize().map_err(|e| e.to_string())?;
                destination.set_focus().map_err(|e| e.to_string())?;
                tracing::info!(tab_id=%tab_id,"native browser focus returned");
            }
            if present_closing.load(Ordering::Acquire) {
                let _ = view.hide();
                return Err("Browser session revoked before presentation".into());
            }
            tracing::info!(tab_id=%tab_id,"native browser content show begin");
            view.show().map_err(|e| e.to_string())?;
            tracing::info!(tab_id=%tab_id,"native browser content shown");
            if dest_mode == "embedded" {
                if let Some(shell) = app.get_window(&format!("browser-shell-{tab_id}")) {
                    let _ = shell.hide();
                }
            }
            apply_host.update(&tab_id, |s| { s.visible=true; s.tab.mode=dest_mode; });
            Ok(true)
        })
        .await?;
        if !applied {
            return Ok(BrowserReply::verified(
                json!({"tab":self.tabs.lock().unwrap().get(&state.tab.tab_id).map(|s|s.tab.clone()),"ignored_stale_presenter":true}),
            ));
        }
        tracing::info!(tab_id=%state.tab.tab_id,mode=%mode,"native browser presentation completed");
        Ok(BrowserReply::verified(
            json!({"tab":self.tabs.lock().unwrap().get(&state.tab.tab_id).map(|s|s.tab.clone())}),
        ))
    }
}
