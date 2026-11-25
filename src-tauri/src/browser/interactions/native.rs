use super::*;
use std::{cell::RefCell, collections::HashMap};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::core::Interface;

#[path = "hooks.rs"]
mod hooks;
#[path = "save.rs"]
mod save;

enum PendingKind {
    Popup {
        args: ICoreWebView2NewWindowRequestedEventArgs,
        deferral: ICoreWebView2Deferral,
        environment: ICoreWebView2Environment,
        proxy: Arc<super::super::network::BrowserProxy>,
    },
    Download {
        args: ICoreWebView2DownloadStartingEventArgs,
        deferral: ICoreWebView2Deferral,
        dialog: Option<windows::Win32::UI::Shell::IFileSaveDialog>,
    },
    Permission {
        args: ICoreWebView2PermissionRequestedEventArgs,
        deferral: ICoreWebView2Deferral,
    },
}

struct Pending {
    tab_id: String,
    kind: PendingKind,
}

thread_local! {
    static PENDING: RefCell<HashMap<String, Pending>> = RefCell::new(HashMap::new());
}

async fn on_ui<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(work());
    })
    .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "Browser interaction UI operation was interrupted".to_string())?
}

fn deny_owned(pending: Pending) -> Result<(), String> {
    unsafe {
        let result = match pending.kind {
            PendingKind::Popup { args, deferral, .. } => {
                let set = args.SetHandled(true);
                let completed = deferral.Complete();
                set.and(completed)
            }
            PendingKind::Download {
                args,
                deferral,
                dialog,
            } => {
                if let Some(dialog) = dialog {
                    let _ = dialog.Close(windows::core::HRESULT::from_win32(1223));
                }
                let set = args.SetCancel(true);
                let completed = deferral.Complete();
                set.and(completed)
            }
            PendingKind::Permission { args, deferral } => {
                let set = args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY);
                let completed = deferral.Complete();
                set.and(completed)
            }
        };
        result.map_err(|e| e.to_string())
    }
}

fn cancel_document(tab_id: &str) {
    let pending = PENDING.with(|all| {
        let mut all = all.borrow_mut();
        let ids = all
            .iter()
            .filter(|(_, p)| p.tab_id == tab_id)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        ids.into_iter()
            .filter_map(|id| all.remove(&id))
            .collect::<Vec<_>>()
    });
    for item in pending {
        if let Err(error) = deny_owned(item) {
            tracing::warn!(error=%error,"Browser document request cancellation failed");
        }
    }
}

pub(super) async fn deny(app: &AppHandle, id: String) -> Result<(), String> {
    on_ui(app, move || {
        if let Some(pending) = PENDING.with(|p| p.borrow_mut().remove(&id)) {
            deny_owned(pending)?;
        }
        Ok(())
    })
    .await
}

pub(super) async fn cancel(app: &AppHandle, tab_id: Option<String>) -> Result<(), String> {
    on_ui(app, move || {
        super::commands::forget(tab_id.as_deref());
        let pending = PENDING.with(|p| {
            let mut all = p.borrow_mut();
            let ids = all
                .iter()
                .filter(|(_, item)| tab_id.as_ref().is_none_or(|id| *id == item.tab_id))
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| all.remove(&id))
                .collect::<Vec<_>>()
        });
        let mut failure = None;
        for item in pending {
            if let Err(reason) = deny_owned(item) {
                failure = Some(reason);
            }
        }
        failure.map_or(Ok(()), Err)
    })
    .await
}

pub(crate) fn popup_environment(id: &str) -> Result<ICoreWebView2Environment, String> {
    PENDING.with(|all| match all.borrow().get(id).map(|p| &p.kind) {
        Some(PendingKind::Popup { environment, .. }) => Ok(environment.clone()),
        _ => Err("Popup request was cancelled before its managed tab could be created".into()),
    })
}

pub(crate) async fn attach_popup(view: &Webview, id: &str) -> Result<(), String> {
    let id = id.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let result = (|| -> Result<(), String> {
            let pending = PENDING
                .with(|all| all.borrow_mut().remove(&id))
                .ok_or("Popup request no longer exists")?;
            match pending.kind {
                PendingKind::Popup {
                    args,
                    deferral,
                    environment,
                    proxy,
                } => unsafe {
                    let result = native
                        .controller()
                        .CoreWebView2()
                        .and_then(|core| {
                            args.SetNewWindow(&core)?;
                            // SetNewWindow replaces the web contents. Register request
                            // interception on the replacement before releasing navigation.
                            super::super::platform::attach_network_boundary(
                                &core,
                                environment,
                                proxy,
                            )
                        })
                        .and_then(|_| args.SetHandled(true));
                    let completed = deferral.Complete();
                    result.and(completed).map_err(|e| e.to_string())
                },
                other => {
                    PENDING.with(|all| {
                        all.borrow_mut().insert(
                            id,
                            Pending {
                                tab_id: pending.tab_id,
                                kind: other,
                            },
                        )
                    });
                    Err("Request is not a popup".into())
                }
            }
        })();
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "Popup attachment was interrupted".to_string())?
}

pub(super) async fn resolve(
    host: &NativeBrowserHost,
    request: &BrowserUiRequest,
    decision: &str,
) -> Result<Value, String> {
    if decision == "deny" {
        deny(&host.app, request.id.clone()).await?;
        return Ok(json!({"denied":true}));
    }
    if decision == "save" {
        return save::download(host, request).await;
    }
    let id = request.id.clone();
    on_ui(&host.app, move || {
        let pending = PENDING
            .with(|all| all.borrow_mut().remove(&id))
            .ok_or("Browser permission request no longer exists")?;
        match pending.kind {
            PendingKind::Permission { args, deferral } => unsafe {
                let result = args
                    .cast::<ICoreWebView2PermissionRequestedEventArgs3>()
                    .and_then(|args| args.SetSavesInProfile(false))
                    .and_then(|_| args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW));
                let completed = deferral.Complete();
                result.and(completed).map_err(|e| e.to_string())?;
                Ok(json!({"allowed":true,"persistent":false}))
            },
            other => {
                let _ = deny_owned(Pending {
                    tab_id: pending.tab_id,
                    kind: other,
                });
                Err("Request is not a permission request".into())
            }
        }
    })
    .await
}

pub(crate) async fn install(
    view: &Webview,
    host: NativeBrowserHost,
    tab_id: String,
) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let result = hooks::install(native, host, tab_id).map_err(|e| e.to_string());
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "Browser native interaction setup was interrupted".to_string())?
}

fn emit_ui(host: &NativeBrowserHost, tab_id: &str, event: &str, payload: Value) {
    if let Some(view) = host.app.get_webview(tab_id) {
        let _ = host.app.emit_to(view.window().label(), event, payload);
    }
}

fn shortcut(key: u32, control: bool, shift: bool, alt: bool) -> Option<&'static str> {
    if !control || alt {
        return None;
    }
    match (key, shift) {
        (0x4c, false) => Some("focus-address"),
        (0x54, false) => Some("new-tab"),
        (0x57, false) => Some("close-tab"),
        (0x09, false) => Some("next-tab"),
        (0x09, true) => Some("previous-tab"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_shell_shortcuts_are_intercepted() {
        assert_eq!(shortcut(0x4c, true, false, false), Some("focus-address"));
        assert_eq!(shortcut(0x09, true, true, false), Some("previous-tab"));
        for key in [0x46, 0x52, 0x6b, 0xbb, 0xbd, 0x74] {
            assert_eq!(shortcut(key, true, false, false), None);
        }
        assert_eq!(shortcut(0x4c, false, false, false), None);
        assert_eq!(shortcut(0x54, true, false, true), None);
    }
}
