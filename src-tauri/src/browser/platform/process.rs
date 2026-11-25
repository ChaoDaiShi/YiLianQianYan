//! Observe the actual browser-process exit before releasing a persistent context.
use std::{cell::RefCell, collections::HashMap, sync::Arc};
use tauri::Webview;
use webview2_com::{BrowserProcessExitedEventHandler, Microsoft::Web::WebView2::Win32::*};
use windows::core::Interface;

use super::super::profiles::BrowserProcessLifetime;

struct ProcessEventSource {
    environment: ICoreWebView2Environment5,
    token: i64,
}

thread_local! {
    // WebView disposal releases Wry's environment references. The event source
    // must remain alive until it can report that its browser process has exited.
    static EVENT_SOURCES: RefCell<HashMap<u32,ProcessEventSource>> = RefCell::new(HashMap::new());
}

pub(in crate::browser) async fn track_process(
    view: &Webview,
    lifetime: Arc<BrowserProcessLifetime>,
) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                let core = native.controller().CoreWebView2()?;
                let mut pid = 0;
                core.BrowserProcessId(&mut pid)?;
                let started = lifetime.started(pid);
                if !started {
                    return Ok(());
                }
                let listener_lifetime = lifetime.clone();
                let registered = (|| -> windows::core::Result<()> {
                    let environment = native.environment().cast::<ICoreWebView2Environment5>()?;
                    let mut token = 0;
                    environment.add_BrowserProcessExited(
                        &BrowserProcessExitedEventHandler::create(Box::new(move |_, args| {
                            if let Some(args) = args {
                                let mut exited_pid = 0;
                                args.BrowserProcessId(&mut exited_pid)?;
                                let mut exit_kind=COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND::default();
                                args.BrowserProcessExitKind(&mut exit_kind)?;
                                if exited_pid == pid {
                                    // Remove ownership before calling COM; an event
                                    // callback must never reenter a live RefCell borrow.
                                    let source = EVENT_SOURCES.with(|sources| sources.borrow_mut().remove(&pid));
                                    if let Some(source) = source {
                                        if let Err(error) = source.environment.remove_BrowserProcessExited(source.token) {
                                            tracing::warn!(browser_process_id=pid,error=%error,"persistent browser exit handler removal failed");
                                        }
                                        drop(source);
                                    }
                                    listener_lifetime.exited(pid);
                                    tracing::info!(
                                        browser_process_id = pid,
                                        exit_kind = exit_kind.0,
                                        "persistent browser process exit observed"
                                    );
                                }
                            }
                            Ok(())
                        })),
                        &mut token,
                    )?;
                    EVENT_SOURCES.with(|sources| sources.borrow_mut().insert(pid, ProcessEventSource {environment,token}));
                    tracing::info!(
                        browser_process_id = pid,
                        event_token = token,
                        "persistent browser process exit handler installed"
                    );
                    Ok(())
                })();
                if registered.is_err() && started {
                    lifetime.exited(pid);
                }
                registered
            }
        })()
        .map_err(|e| e.to_string());
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "Browser process tracking acknowledgement was interrupted".to_string())?
}
