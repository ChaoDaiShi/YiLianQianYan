use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct CaptureSessions(Mutex<Option<CancellationToken>>);

impl CaptureSessions {
    fn begin(&self) -> Result<CaptureSession<'_>, String> {
        let mut current = self.0.lock().unwrap();
        if current.is_some() {
            return Err("screen_capture_busy".into());
        }
        let token = CancellationToken::new();
        *current = Some(token.clone());
        Ok(CaptureSession {
            sessions: self,
            token,
        })
    }

    pub(crate) fn cancel(&self) {
        if let Some(token) = self.0.lock().unwrap().as_ref() {
            token.cancel();
        }
    }
}

struct CaptureSession<'a> {
    sessions: &'a CaptureSessions,
    token: CancellationToken,
}

impl Drop for CaptureSession<'_> {
    fn drop(&mut self) {
        self.token.cancel();
        self.sessions.0.lock().unwrap().take();
    }
}

pub(crate) fn trusted_main(view: &tauri::Webview) -> Result<(), String> {
    if view.label() != "main" {
        return Err("main_window_required".into());
    }
    crate::browser::trusted_caller(view)
}

#[tauri::command]
pub(super) async fn desktop_capture_screen(
    view: tauri::Webview,
    sessions: tauri::State<'_, CaptureSessions>,
    control: tauri::State<'_, crate::ControlSessionState>,
    backend: tauri::State<'_, crate::DesktopBackendPort>,
) -> Result<tauri::ipc::Response, String> {
    trusted_main(&view)?;
    let session = sessions.begin()?;
    let port = backend.0.ok_or("screen_permission_unavailable")?;
    let permission = || async {
        yilian_backend::api::read_screen_permission(port, &control.0)
            .await
            .map_err(|_| "screen_permission_unavailable".to_owned())
    };
    let before = permission().await?;
    if !before.allowed {
        return Err("screen_permission_denied".into());
    }
    let revoked = async {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if permission().await.as_ref() != Ok(&before) {
                break;
            }
        }
    };
    tokio::pin!(revoked);
    let target = {
        let pick = capture::pick(view, &session.token);
        tokio::pin!(pick);
        tokio::select! {
            target = &mut pick => target?,
            _ = &mut revoked => {
                session.token.cancel();
                let _ = pick.await;
                return Err("screen_permission_revoked".into());
            }
        }
    };
    let Some(target) = target else {
        return Ok(tauri::ipc::Response::new(Vec::<u8>::new()));
    };
    // The consent is scoped to the selected target and the current profile.
    if session.token.is_cancelled() || permission().await? != before {
        return Err("screen_permission_revoked".into());
    }
    let snapshot = capture::snapshot(target, session.token.clone());
    tokio::pin!(snapshot);
    let bytes = tokio::select! {
        bytes = &mut snapshot => bytes?,
        _ = &mut revoked => {
            session.token.cancel();
            // Wait for native capture to release its device before a new session
            // can start, even if authorization vanished while waiting for pixels.
            let _ = snapshot.await;
            return Err("screen_permission_revoked".into());
        }
    };
    if session.token.is_cancelled() || permission().await? != before {
        return Err("screen_permission_revoked".into());
    }
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub(super) fn desktop_cancel_capture(
    view: tauri::Webview,
    sessions: tauri::State<'_, CaptureSessions>,
) -> Result<(), String> {
    trusted_main(&view)?;
    sessions.cancel();
    Ok(())
}

#[cfg(windows)]
#[path = "desktop_perception_windows.rs"]
mod capture;

#[cfg(not(windows))]
mod capture {
    use super::*;

    pub(super) async fn pick(
        _view: tauri::Webview,
        _token: &CancellationToken,
    ) -> Result<Option<()>, String> {
        Err("screen_capture_unsupported".into())
    }

    pub(super) async fn snapshot(
        _target: (),
        _token: CancellationToken,
    ) -> Result<Vec<u8>, String> {
        Err("screen_capture_unsupported".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_is_exclusive_until_the_cancelled_session_releases_resources() {
        let sessions = CaptureSessions::default();
        let first = sessions.begin().unwrap();
        assert!(sessions.begin().is_err());
        sessions.cancel();
        assert!(first.token.is_cancelled());
        assert!(sessions.begin().is_err());
        drop(first);
        assert!(!sessions.begin().unwrap().token.is_cancelled());
    }
}
