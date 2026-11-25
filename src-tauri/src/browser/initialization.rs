//! Owned initialization remains alive until its real native creation settles.
use super::*;

type OpenResult =
    Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<BrowserReply, BrowserError>>>>>;

impl NativeBrowserHost {
    pub(super) async fn open_owned(
        &self,
        request: BrowserRequest,
        cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        let cancel = cancel.child_token();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let result = Arc::new(Mutex::new(Some(tx)));
        let host = self.clone();
        self.pending_workers.fetch_add(1, Ordering::AcqRel);
        let guard = NativeWorkGuard(self.pending_workers.clone());
        tauri::async_runtime::spawn(async move {
            let _guard = guard;
            let reply = host.open(&request, &cancel, result.clone()).await;
            if let Some(tx) = result.lock().unwrap().take() {
                let _ = tx.send(reply);
            };
        });
        rx.await.map_err(|_| {
            error(
                "native_closed",
                "Browser initialization task was interrupted",
            )
        })?
    }

    async fn create_owned<T: Send + 'static>(
        &self,
        cancel: &CancellationToken,
        result: &OpenResult,
        on_ui: bool,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, BrowserError> {
        let app = self.app.clone();
        let mut worker = tokio::task::spawn_blocking(move || {
            if on_ui {
                // The popup's COM environment is owned by the UI thread.
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                app.run_on_main_thread(move || {
                    let _ = tx.send(work());
                })
                .map_err(|e| e.to_string())?;
                rx.recv()
                    .map_err(|_| "Popup creation UI acknowledgement was interrupted".to_string())?
            } else {
                work()
            }
        });
        let joined = tokio::select! {
            joined = &mut worker => joined,
            _ = tokio::time::sleep(Duration::from_secs(15)) => {
                // Notify the caller on time, but keep owning this non-cancellable native worker.
                cancel.cancel();
                if let Some(tx) = result.lock().unwrap().take() {
                    let _ = tx.send(Err(error("native_timeout", "Native browser UI operation timed out")));
                }
                worker.await
            }
        };
        joined
            .map_err(|_| error("native_closed", "Native browser worker was interrupted"))?
            .map_err(|e| error("native_operation_failed", e))
    }

    async fn open(
        &self,
        request: &BrowserRequest,
        cancel: &CancellationToken,
        result: OpenResult,
    ) -> Result<BrowserReply, BrowserError> {
        if !cfg!(windows) {
            return Err(error(
                "unsupported",
                "The isolated visible browser is currently available on Windows only",
            ));
        }
        let url = request
            .payload
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| error("invalid_url", "URL is required"))?;
        let parsed =
            tauri::Url::parse(url).map_err(|_| error("invalid_url", "Invalid web address"))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(error(
                "invalid_url",
                "Only credential-free HTTP(S) addresses are supported",
            ));
        }
        if self.tabs.lock().unwrap().len() >= 12 {
            return Err(error(
                "invalid_capacity",
                "Close a browser tab before opening another (maximum 12)",
            ));
        }
        let mut forbidden = vec![1420, 9420];
        if let Ok(addr) = std::env::var("YILIAN_HOST") {
            if let Some(port) = addr.rsplit(':').next().and_then(|p| p.parse().ok()) {
                forbidden.push(port)
            }
        }
        if let Some(dev) = self.app.config().build.dev_url.as_ref() {
            if let Some(port) = dev.port_or_known_default() {
                forbidden.push(port)
            }
        }
        if let Ok(args) = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
            for item in args.split_whitespace() {
                if let Some(port) = item
                    .strip_prefix("--remote-debugging-port=")
                    .and_then(|p| p.parse::<u16>().ok())
                {
                    forbidden.push(port)
                }
            }
        }
        let tab_id = self.id("browser-content");
        let session_id = self.id("browser-session");
        let popup_id = request
            .payload
            .get("_native_popup_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let lease = if let Some(parent_id) = request
            .payload
            .get("_native_parent_tab_id")
            .and_then(Value::as_str)
        {
            let parent = self.lookup(request.target.as_ref())?;
            if popup_id.is_none() || parent.tab.tab_id != parent_id {
                return Err(error(
                    "invalid_popup",
                    "The popup opener does not match its native request",
                ));
            }
            parent.context
        } else {
            self.acquire_browser_profile(request, &parsed, forbidden, &session_id)
                .await?
        };
        let proxy = lease.proxy.clone();
        #[cfg(windows)]
        let process = lease.process.clone();
        let profile = lease.profile.clone();
        let ephemeral = lease.ephemeral;
        if !proxy.allows(url) {
            return Err(error(
                "blocked_origin",
                "The address is reserved for the assistant or is outside this browser session",
            ));
        }
        if cancel.is_cancelled() {
            return Err(error(
                "cancelled_before_execution",
                "Browser opening cancelled",
            ));
        }
        tracing::info!(ephemeral, "native browser proxy ready; preparing tab");
        let chrome_profile =
            std::env::temp_dir().join(format!("yilian-browser-trusted-chrome-{tab_id}"));
        std::fs::create_dir(&chrome_profile).map_err(|_| {
            error(
                "native_profile_failed",
                "Temporary browser chrome profile could not be created",
            )
        })?;
        let tab = BrowserTab {
            session_id,
            tab_id: tab_id.clone(),
            workspace_id: request
                .payload
                .get("workspace_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            site_id: request
                .payload
                .get("site_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            url: "about:blank".into(),
            title: "New browser tab".into(),
            navigation_id: 0,
            mode: "embedded".into(),
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            error: None,
            ephemeral,
        };
        {
            let mut opening_tabs = self.tabs.lock().unwrap();
            if self.closing.load(Ordering::Acquire) {
                drop(opening_tabs);
                let _ = remove_temporary_profile(&chrome_profile);
                return Err(error(
                    "permission_denied",
                    "Browser sessions were revoked; restart the application",
                ));
            }
            opening_tabs.insert(
                tab_id.clone(),
                TabState {
                    entry_url: parsed.to_string(),
                    tab: tab.clone(),
                    context: lease,
                    proxy: proxy.clone(),
                    profile: profile.clone(),
                    chrome_profile: chrome_profile.clone(),
                    observation: None,
                    credential_filled: false,
                    visible: false,
                    ready: false,
                    presentation_sequence: Arc::new(AtomicU64::new(0)),
                    presentation_owner: Arc::new(Mutex::new("main".into())),
                    presentation_gate: Arc::new(Mutex::new(())),
                    disposal_gate: Arc::new(tokio::sync::Mutex::new(())),
                },
            );
        }
        let app = self.app.clone();
        let native = self.clone();
        let label = tab_id.clone();
        let args = proxy.browser_args();
        let nav_proxy = proxy.clone();
        let create_closing = self.closing.clone();
        #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
        let fixture_operation = request.operation_id.clone();
        #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
        let fixture_tab = tab_id.clone();
        let title_host = self.clone();
        let title_label = tab_id.clone();
        let load_host = self.clone();
        let load_label = tab_id.clone();
        let create_popup_id = popup_id.clone();
        tracing::info!(tab_id=%tab_id,"native browser child creation queued");
        let view = self
            .create_owned(cancel, &result, popup_id.is_some(), move || {
                let window = app
                    .get_window("main")
                    .ok_or("Main application window is unavailable")?;
                let builder = WebviewBuilder::new(
                    &label,
                    WebviewUrl::External(tauri::Url::parse("about:blank").unwrap()),
                )
                .data_directory(profiles::webview_directory(profile))
                .incognito(ephemeral)
                .additional_browser_args(&args)
                .devtools(false)
                .zoom_hotkeys_enabled(true);
                #[cfg(windows)]
                let builder = if let Some(id) = create_popup_id {
                    builder.with_environment(interactions::popup_environment(&id)?)
                } else {
                    builder
                };
                let view = window
                    .add_child(
                        builder
                            .on_navigation(move |url| {
                                if url.as_str() == "about:blank" {
                                    return true;
                                }
                                let allowed = nav_proxy.permits_navigation(url.as_str());
                                native.update(&label, |state| {
                                    state.observation = None;
                                    state.tab.loading = allowed;
                                    if allowed {
                                        state.tab.navigation_id =
                                            state.tab.navigation_id.saturating_add(1);
                                        state.tab.url = url.to_string();
                                        state.tab.error = None
                                    } else {
                                        state.tab.error = Some(
                                            "Navigation blocked by browser network boundary".into(),
                                        )
                                    }
                                });
                                allowed
                            })
                            .on_document_title_changed(move |_, title| {
                                title_host.update(&title_label, |state| {
                                    state.tab.title = title.chars().take(240).collect()
                                })
                            })
                            .on_page_load(move |_view, payload| {
                                if payload.event() == tauri::webview::PageLoadEvent::Finished
                                    && payload.url().as_str() != "about:blank"
                                {
                                    load_host.update(&load_label, |state| {
                                        state.tab.loading = false;
                                        state.tab.url = payload.url().to_string();
                                        state.credential_filled = false;
                                    });
                                }
                            }),
                        LogicalPosition::new(90.0, 130.0),
                        LogicalSize::new(900.0, 600.0),
                    )
                    .map_err(|e| e.to_string())?;
                view.hide().map_err(|e| e.to_string())?;
                #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
                fixture::pause("creation_registered", &fixture_operation, &fixture_tab);
                Ok(view)
            })
            .await;
        tracing::info!(tab_id=%tab_id,"native browser child creation returned");
        let view = match view {
            Ok(view) => view,
            Err(e) => {
                self.retire_and_dispose(&tab_id).await?;
                return Err(e);
            }
        };
        if cancel.is_cancelled() || create_closing.load(Ordering::Acquire) {
            self.retire_and_dispose(&tab_id).await?;
            return Err(error(
                "cancelled_before_execution",
                "Browser opening cancelled",
            ));
        }
        tracing::info!(tab_id=%tab_id,"native browser hardening begin");
        #[cfg(windows)]
        if !ephemeral {
            if let Err(reason) = platform::track_process(&view, process).await {
                self.retire_and_dispose(&tab_id).await?;
                return Err(error("native_process_tracking_failed", reason));
            }
        }
        let status_host = self.clone();
        let status_id = tab_id.clone();
        let status = Arc::new(move |event: platform::NativeStatus| {
            status_host.update(&status_id, |s| {
                if event.url != "about:blank" && s.tab.url != event.url {
                    s.tab.url = event.url;
                    s.tab.navigation_id = s.tab.navigation_id.saturating_add(1);
                    s.observation = None;
                }
                s.tab.can_go_back = event.can_go_back;
                s.tab.can_go_forward = event.can_go_forward;
                if let Some(completed) = event.completed {
                    s.tab.loading = false;
                    if let Err(reason) = completed {
                        s.tab.error = Some(reason);
                    }
                }
            });
        });
        let target = self
            .tabs
            .lock()
            .unwrap()
            .get(&tab_id)
            .expect("owned tab is registered")
            .tab
            .target();
        if let Err(reason) = platform::harden(&view, proxy, status, self.clone(), target).await {
            self.retire_and_dispose(&tab_id).await?;
            return Err(error("native_hardening_failed", reason));
        }
        if cancel.is_cancelled() || self.closing.load(Ordering::Acquire) {
            self.retire_and_dispose(&tab_id).await?;
            return Err(error(
                "cancelled_before_execution",
                "Browser opening cancelled",
            ));
        }
        #[cfg(windows)]
        if let Some(id) = popup_id.as_deref() {
            if let Err(reason) = interactions::attach_popup(&view, id).await {
                self.retire_and_dispose(&tab_id).await?;
                return Err(error("native_popup_failed", reason));
            }
        }
        self.update(&tab_id, |state| state.ready = true);
        if popup_id.is_some()
            && request
                .payload
                .get("_native_popup_blank")
                .and_then(Value::as_bool)
                == Some(true)
        {
            self.update(&tab_id, |state| {
                state.tab.loading = false;
                state.tab.navigation_id = state.tab.navigation_id.max(1);
            });
        }
        tracing::info!(tab_id=%tab_id,"native browser hardening complete; requesting trusted presentation");
        self.app
            .emit_to("main", "browser:opened", json!({"tab_id":tab_id}))
            .map_err(|e| error("native_dispatch_failed", e.to_string()))?;
        tracing::info!(tab_id=%tab_id,"native browser document navigation requested");
        if popup_id.is_none() {
            platform::navigation(
                &view,
                "navigate",
                Some(parsed.to_string()),
                None,
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
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            if cancel.is_cancelled() || self.closing.load(Ordering::Acquire) {
                return Ok(BrowserReply::unknown(
                    &request.operation_id,
                    "Browser opening was interrupted; inspect the existing tab before retrying",
                ));
            }
            let current = self.tabs.lock().unwrap().get(&tab_id).cloned();
            if current.is_none()
                || current
                    .as_ref()
                    .is_some_and(|s| s.tab.error.as_deref() == Some("Navigation stopped by user"))
            {
                return Ok(BrowserReply::unknown(&request.operation_id,"Browser opening was stopped or closed by the user; no automatic replay is allowed"));
            }
            if current.as_ref().is_some_and(|s| {
                s.tab
                    .error
                    .as_deref()
                    .is_some_and(|e| e.starts_with("Navigation failed"))
            }) {
                return Ok(BrowserReply::unknown(&request.operation_id,"Browser navigation failed after dispatch; inspect the existing tab before retrying"));
            }
            if current.as_ref().is_some_and(|s| {
                s.visible
                    && !s.tab.loading
                    && (popup_id.is_some() || s.tab.navigation_id > 0 && s.tab.url != "about:blank")
                    && view.window().is_visible().unwrap_or(false)
                    && !view.window().is_minimized().unwrap_or(true)
            }) {
                break;
            }
            if std::time::Instant::now() > deadline {
                return Ok(BrowserReply::unknown(&request.operation_id,"Browser tab exists but visible document readiness was not confirmed; inspect the existing tab before retrying"));
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        Ok(BrowserReply::verified(
            json!({"tab":self.tabs.lock().unwrap().get(&tab_id).map(|t|t.tab.clone())}),
        ))
    }
}
