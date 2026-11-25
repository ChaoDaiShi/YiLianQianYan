//! BrowserHost dispatch and native document actions.
use super::*;

#[async_trait]
impl BrowserHost for NativeBrowserHost {
    fn tabs(&self) -> Vec<BrowserTab> {
        self.tab_views(true)
    }
    async fn shutdown(&self) -> Result<(), BrowserError> {
        self.shutdown_owned().await
    }
    async fn dispatch(
        &self,
        request: BrowserRequest,
        cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(error(
                "permission_denied",
                "Browser sessions were revoked; restart the application",
            ));
        }
        if matches!(
            request.action,
            BrowserAction::Present | BrowserAction::Stop | BrowserAction::Close
        ) {
            let state = self.lookup_control(request.target.as_ref())?;
            if request.action == BrowserAction::Present {
                return self.present(state, &request.payload).await;
            }
            return self.control(state, &request).await;
        }
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(error("cancelled_before_execution","Browser request cancelled before dispatch")),guard=self.gate.lock()=>guard};
        if cancel.is_cancelled() {
            return Err(error(
                "cancelled_before_execution",
                "Browser request cancelled before dispatch",
            ));
        }
        if request.action == BrowserAction::List {
            return Ok(BrowserReply::verified(json!({"tabs":self.tabs()})));
        }
        if request.action == BrowserAction::Open {
            return self.open_owned(request, cancel).await;
        }
        let mut state = self.lookup(request.target.as_ref())?;
        if request.owner.is_some() || request.payload.get("_source_tool_call_id").is_some() {
            state = self
                .ensure_visible(
                    state,
                    request.target.as_ref().expect("target checked"),
                    &cancel,
                )
                .await?;
        }
        if request.action == BrowserAction::Present {
            return self.present(state, &request.payload).await;
        }
        let view = self.view(&state)?;
        if request.action == BrowserAction::Navigate {
            let raw = request
                .payload
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| error("invalid_url", "URL is required"))?;
            let url =
                tauri::Url::parse(raw).map_err(|_| error("invalid_url", "Invalid web address"))?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(error(
                    "invalid_url",
                    "Only credential-free HTTP(S) addresses are supported",
                ));
            }
            self.update(&state.tab.tab_id, |s| s.observation = None);
            platform::navigation(
                &view,
                "navigate",
                Some(url.to_string()),
                Some(state.proxy.clone()),
                cancel.clone(),
            )
            .await
            .map_err(|e| {
                error(
                    if e == "cancelled_before_execution" {
                        "cancelled_before_execution"
                    } else {
                        "native_navigation_failed"
                    },
                    e,
                )
            })?;
            return Ok(BrowserReply::verified(
                json!({"navigation_requested":true,"tab_id":state.tab.tab_id}),
            ));
        }
        if matches!(
            request.action,
            BrowserAction::Back | BrowserAction::Forward | BrowserAction::Reload
        ) {
            self.update(&state.tab.tab_id, |s| s.observation = None);
            platform::navigation(&view, request.action.name(), None, None, cancel.clone())
                .await
                .map_err(|e| {
                    error(
                        if e == "cancelled_before_execution" {
                            "cancelled_before_execution"
                        } else {
                            "native_navigation_failed"
                        },
                        e,
                    )
                })?;
            return Ok(BrowserReply::verified(json!({"navigation_requested":true})));
        }
        if !state.visible
            || !view.window().is_visible().unwrap_or(false)
            || view.window().is_minimized().unwrap_or(true)
        {
            return Err(error(
                "invalid_visibility",
                "Open the tab in the visible browser before reading or acting",
            ));
        }
        if state.tab.loading {
            return Err(error(
                "stale_navigation",
                "Wait for the current page to finish loading before observing or acting",
            ));
        }
        let actual = view
            .url()
            .map_err(|e| error("native_url_failed", e.to_string()))?;
        if actual.as_str() != state.tab.url || !state.proxy.permits_navigation(actual.as_str()) {
            return Err(error(
                "stale_navigation",
                "The visible document changed; inspect its current state",
            ));
        }
        if request.action == BrowserAction::FillCredential {
            return self
                .fill_credential(view, state.tab.target(), actual, cancel)
                .await;
        }
        if state.credential_filled {
            return Err(error(
                "permission_denied",
                "登录凭据已填入，请在浏览器中完成登录",
            ));
        }
        let reply = match request.action {
            BrowserAction::Observe => {
                let token = self.id("observation");
                let data = self.eval(&view, scripts::observe(&token), &cancel).await?;
                self.lookup(request.target.as_ref())?;
                self.update(&state.tab.tab_id, |s| s.observation = Some(token));
                BrowserReply::verified(
                    json!({"tab":state.tab,"observation":data,"untrusted_content":true}),
                )
            }
            BrowserAction::Click | BrowserAction::Fill => {
                let token = request
                    .payload
                    .get("observation_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("invalid_observation", "Observation ID is required"))?;
                if state.observation.as_deref() != Some(token) {
                    return Err(error(
                        "stale_observation",
                        "Observe the current page before acting",
                    ));
                }
                let element = request
                    .payload
                    .get("element_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("invalid_element", "Element ID is required"))?;
                let script =
                    scripts::action(request.action.name(), token, element, &request.payload)
                        .map_err(|e| error("invalid_action", e))?;
                let data = self.eval(&view, script, &cancel).await?;
                self.update(&state.tab.tab_id, |s| s.observation = None);
                if request.action == BrowserAction::Click {
                    BrowserReply::unknown(&request.operation_id,"Click dispatched to the visible page; its business effect is not known. Observe and explicitly review before another mutation.")
                } else {
                    BrowserReply::verified(
                        json!({"tab":state.tab,"action":data,"business_result_verified":false}),
                    )
                }
            }
            BrowserAction::Scroll => {
                let token = request
                    .payload
                    .get("observation_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("invalid_observation", "Observation ID is required"))?;
                if state.observation.as_deref() != Some(token) {
                    return Err(error(
                        "stale_observation",
                        "Observe the current page before scrolling",
                    ));
                }
                let x = request
                    .payload
                    .get("x")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let y = request
                    .payload
                    .get("delta_y")
                    .or_else(|| request.payload.get("y"))
                    .and_then(Value::as_i64)
                    .unwrap_or(500);
                let data = self
                    .eval(&view, scripts::scroll(token, x, y), &cancel)
                    .await?;
                self.update(&state.tab.tab_id, |s| s.observation = None);
                BrowserReply::verified(json!({"tab":state.tab,"action":data}))
            }
            BrowserAction::Capture => {
                let data = platform::cdp(
                    &view,
                    "Page.captureScreenshot",
                    json!({"format":"png","captureBeyondViewport":false}),
                )
                .await
                .map_err(|e| error("native_capture_failed", e))?;
                let base64 = data.get("data").and_then(Value::as_str).ok_or_else(|| {
                    error(
                        "native_capture_failed",
                        "Native screenshot data is unavailable",
                    )
                })?;
                if base64.len() > 12 * 1024 * 1024 {
                    return Err(error(
                        "native_capture_failed",
                        "Screenshot exceeds the bounded evidence limit",
                    ));
                }
                self.lookup(request.target.as_ref())?;
                BrowserReply::verified(
                    json!({"tab":state.tab,"mime_type":"image/png","data_base64":base64,"untrusted_content":true}),
                )
            }
            _ => return Err(error("unsupported", "Unsupported browser operation")),
        };
        Ok(reply)
    }
}
