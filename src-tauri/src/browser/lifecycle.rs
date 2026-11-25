//! Native disposal and shutdown ownership.
use super::*;

impl NativeBrowserHost {
    pub(super) async fn control(
        &self,
        state: TabState,
        request: &BrowserRequest,
    ) -> Result<BrowserReply, BrowserError> {
        if request.action == BrowserAction::Close {
            self.retire_and_dispose(&state.tab.tab_id).await?;
            return Ok(BrowserReply::verified(
                json!({"closed":true,"tab_id":state.tab.tab_id}),
            ));
        }
        let view = self.view(&state)?;
        if request.action == BrowserAction::Stop {
            platform::cdp(&view, "Page.stopLoading", json!({}))
                .await
                .map_err(|e| error("native_stop_failed", e))?;
            self.update(&state.tab.tab_id, |s| {
                s.tab.loading = false;
                s.tab.error = Some("Navigation stopped by user".into());
                s.observation = None;
            });
            return Ok(BrowserReply::verified(json!({"stopped":true})));
        }
        Err(error("unsupported", "Unknown browser control"))
    }

    pub(super) async fn retire_and_dispose(&self, id: &str) -> Result<(), BrowserError> {
        {
            let mut tabs = self.tabs.lock().unwrap();
            if let Some(state) = tabs.remove(id) {
                state.presentation_sequence.fetch_add(1, Ordering::AcqRel);
                self.retiring.lock().unwrap().insert(id.into(), state);
            }
        }
        let state = self.retiring.lock().unwrap().get(id).cloned();
        let Some(state) = state else { return Ok(()) };
        let _disposal = state.disposal_gate.lock().await;
        if !self.retiring.lock().unwrap().contains_key(id) {
            return Ok(());
        }
        self.dispose_views(id).await?;
        self.retiring.lock().unwrap().remove(id);
        let profiles = [state.chrome_profile];
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            for profile in profiles {
                let _ = remove_temporary_profile(&profile);
            }
        });
        Ok(())
    }

    pub(super) async fn dispose_views(&self, id: &str) -> Result<(), BrowserError> {
        self.interactions
            .cancel_tab(&self.app, id)
            .await
            .map_err(|reason| error("native_shutdown_failed", reason))?;
        if let Some(view) = self.app.get_webview(id) {
            platform::close_native(&view)
                .await
                .map_err(|e| error("native_shutdown_failed", e))?;
        }
        let shell_id = format!("browser-shell-{id}");
        if let Some(chrome) = self.app.get_webview(&shell_id) {
            platform::close_native(&chrome)
                .await
                .map_err(|e| error("native_shutdown_failed", e))?;
        }
        let app = self.app.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        // UI-thread destruction completes synchronously in the installed Tauri runtime.
        self.app
            .run_on_main_thread(move || {
                let result = app
                    .get_window(&shell_id)
                    .map_or(Ok(()), |shell| shell.destroy().map_err(|e| e.to_string()));
                let _ = tx.send(result);
            })
            .map_err(|e| error("native_shutdown_failed", e.to_string()))?;
        tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .map_err(|_| {
                error(
                    "native_shutdown_timeout",
                    "Native disposal UI acknowledgement timed out",
                )
            })?
            .map_err(|_| {
                error(
                    "native_shutdown_failed",
                    "Native disposal acknowledgement was lost",
                )
            })?
            .map_err(|e| error("native_shutdown_failed", e))
    }

    pub(super) async fn shutdown_owned(&self) -> Result<(), BrowserError> {
        self.closing.store(true, Ordering::Release);
        self.shutdown_persistent_contexts().await;
        {
            let mut tabs = self.tabs.lock().unwrap();
            let mut retiring = self.retiring.lock().unwrap();
            for (id, state) in tabs.drain() {
                state.proxy.shutdown();
                state.presentation_sequence.fetch_add(1, Ordering::AcqRel);
                retiring.insert(id, state);
            }
            for state in retiring.values() {
                state.proxy.shutdown();
            }
        }
        // A late creation/presentation still owns its controller/shell until its worker settles.
        self.interactions
            .cancel_all(&self.app)
            .await
            .map_err(|reason| error("native_shutdown_failed", reason))?;
        let deadline = std::time::Instant::now() + Duration::from_millis(700);
        while self.pending_workers.load(Ordering::Acquire) > 0 {
            if std::time::Instant::now() > deadline {
                return Err(error(
                    "native_shutdown_timeout",
                    "Pending browser creation/presentation has not settled",
                ));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let ids: Vec<_> = self.retiring.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.retire_and_dispose(&id).await?;
        }
        self.wait_persistent_processes().await?;
        Ok(())
    }
}
