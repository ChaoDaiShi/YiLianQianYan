//! Native-only hardening and screenshot access. Browser content never gets host objects or IPC.
use serde_json::Value;
use tauri::Webview;

#[cfg(windows)]
mod process;
#[cfg(windows)]
pub(super) use process::track_process;

/// Acknowledge trusted UI focus transfer; the runtime patch owns reparent lock safety.
#[cfg(windows)]
pub async fn focus_native(view: &Webview) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC;
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let result = unsafe {
            native
                .controller()
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
        }
        .map_err(|e| e.to_string());
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .map_err(|_| "Trusted browser chrome focus timed out")?
        .map_err(|_| "Trusted browser chrome focus acknowledgement was lost")?
}

#[cfg(not(windows))]
pub async fn focus_native(_view: &Webview) -> Result<(), String> {
    Err("Native browser focus is currently supported on Windows only".into())
}

/// Controller.Close executes on its owning UI thread; its reply is native disposal evidence.
#[cfg(windows)]
pub async fn close_native(view: &Webview) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
    let label = view.label().to_string();
    view.with_webview(move |native| {
        let result = unsafe { native.controller().Close() }.map_err(|e| e.to_string());
        #[cfg(all(debug_assertions, feature = "native-browser-fixture"))]
        if result.is_ok() {
            super::fixture::record_native_closed(&label);
        }
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .map_err(|_| "Native controller disposal timed out")?
        .map_err(|_| "Native controller disposal acknowledgement was lost")??;
    // Remove Tauri bookkeeping only after the controller has actually closed.
    view.close().map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub async fn close_native(_view: &Webview) -> Result<(), String> {
    Err("Native browser disposal is currently supported on Windows only".into())
}

#[cfg(all(windows, debug_assertions, feature = "native-browser-fixture"))]
pub fn fixture_diagnostics(view: &Webview) {
    use webview2_com::{
        BrowserProcessExitedEventHandler, Microsoft::Web::WebView2::Win32::*,
        ProcessFailedEventHandler,
    };
    use windows::core::Interface;
    if let Err(error) = view.with_webview(move |native| {
        tracing::info!("native fixture controller ready for diagnostics");
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                let core = native.controller().CoreWebView2()?;
                let mut token = 0;
                core.add_ProcessFailed(
                    &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                        if let Some(args) = args {
                            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                            args.ProcessFailedKind(&mut kind)?;
                            if let Ok(detail) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>()
                            {
                                let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
                                let mut code = 0;
                                detail.Reason(&mut reason)?;
                                detail.ExitCode(&mut code)?;
                                tracing::error!(
                                    kind = kind.0,
                                    reason = reason.0,
                                    exit_code = code,
                                    "native fixture WebView2 process failed"
                                );
                            } else {
                                tracing::error!(
                                    kind = kind.0,
                                    "native fixture WebView2 process failed"
                                );
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )?;
                let environment = native.environment().cast::<ICoreWebView2Environment5>()?;
                let mut exit_token = 0;
                environment.add_BrowserProcessExited(
                    &BrowserProcessExitedEventHandler::create(Box::new(move |_, args| {
                        if let Some(args) = args {
                            let mut kind = COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND::default();
                            let mut process = 0;
                            args.BrowserProcessExitKind(&mut kind)?;
                            args.BrowserProcessId(&mut process)?;
                            tracing::info!(
                                kind = kind.0,
                                process_id = process,
                                "native fixture browser process exited"
                            );
                        }
                        Ok(())
                    })),
                    &mut exit_token,
                )?;
                Ok(())
            }
        })();
        if let Err(error) = result {
            tracing::error!(error=%error,"native fixture process diagnostics unavailable");
        }
    }) {
        tracing::error!(error=%error,"native fixture controller diagnostics dispatch failed");
    }
}
#[cfg(not(all(windows, debug_assertions, feature = "native-browser-fixture")))]
pub fn fixture_diagnostics(_view: &Webview) {}

pub async fn evaluate(
    view: &Webview,
    expression: String,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Value, String> {
    let tree = cdp_cancellable(
        view,
        "Page.getFrameTree",
        serde_json::json!({}),
        Some(cancel.clone()),
    )
    .await?;
    let frame = tree
        .pointer("/frameTree/frame/id")
        .and_then(Value::as_str)
        .ok_or("Browser document frame is unavailable")?;
    let world = cdp_cancellable(
        view,
        "Page.createIsolatedWorld",
        serde_json::json!({"frameId":frame,"worldName":"yilian-native-automation"}),
        Some(cancel.clone()),
    )
    .await?;
    let id = world
        .get("executionContextId")
        .and_then(Value::as_u64)
        .ok_or("Isolated browser context is unavailable")?;
    let value = cdp_cancellable(
        view,
        "Runtime.evaluate",
        serde_json::json!({"contextId":id,"expression":expression,"returnByValue":true}),
        Some(cancel),
    )
    .await?;
    if value.get("exceptionDetails").is_some() {
        return Err("Browser document changed or rejected the operation".into());
    }
    value
        .pointer("/result/value")
        .cloned()
        .ok_or_else(|| "Browser operation did not return a structured result".into())
}

pub struct NativeStatus {
    pub url: String,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub completed: Option<Result<(), String>>,
}
#[cfg(windows)]
fn report(
    core: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    callback: &std::sync::Arc<dyn Fn(NativeStatus) + Send + Sync>,
    completed: Option<Result<(), String>>,
) -> windows::core::Result<()> {
    use windows::core::{BOOL, PWSTR};
    unsafe {
        let mut source = PWSTR::null();
        core.Source(&mut source)?;
        let url = webview2_com::take_pwstr(source);
        let mut back = BOOL::default();
        let mut forward = BOOL::default();
        core.CanGoBack(&mut back)?;
        core.CanGoForward(&mut forward)?;
        callback(NativeStatus {
            url,
            can_go_back: back.as_bool(),
            can_go_forward: forward.as_bool(),
            completed,
        });
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn attach_network_boundary(
    core: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    environment: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment,
    proxy: std::sync::Arc<super::network::BrowserProxy>,
) -> windows::core::Result<()> {
    use webview2_com::{
        take_pwstr, Microsoft::Web::WebView2::Win32::*, WebResourceRequestedEventHandler,
    };
    use windows::core::{HSTRING, PWSTR};
    unsafe {
        core.AddWebResourceRequestedFilter(
            &HSTRING::from("*"),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        )?;
        let mut token = 0;
        core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let request = args.Request()?;
                    let mut uri = PWSTR::null();
                    request.Uri(&mut uri)?;
                    if !proxy.allows(&take_pwstr(uri)) {
                        let response = environment.CreateWebResourceResponse(
                            None,
                            403,
                            &HSTRING::from("Blocked by browser boundary"),
                            &HSTRING::from("Content-Type: text/plain\r\nCache-Control: no-store"),
                        )?;
                        args.SetResponse(&response)?;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}

#[cfg(windows)]
pub async fn harden(
    view: &Webview,
    proxy: std::sync::Arc<super::network::BrowserProxy>,
    callback: std::sync::Arc<dyn Fn(NativeStatus) + Send + Sync>,
    host: super::NativeBrowserHost,
    target: super::BrowserTarget,
) -> Result<(), String> {
    use webview2_com::{
        HistoryChangedEventHandler, Microsoft::Web::WebView2::Win32::*,
        NavigationCompletedEventHandler, SourceChangedEventHandler,
    };
    use windows::core::Interface;
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                let core = native.controller().CoreWebView2()?;
                let environment = native.environment();
                attach_network_boundary(&core, environment, proxy)?;
                let source_callback = callback.clone();
                let mut source_token = 0;
                core.add_SourceChanged(
                    &SourceChangedEventHandler::create(Box::new(move |core, _| {
                        if let Some(core) = core {
                            report(&core, &source_callback, None)?;
                        }
                        Ok(())
                    })),
                    &mut source_token,
                )?;
                let history_callback = callback.clone();
                let mut history_token = 0;
                core.add_HistoryChanged(
                    &HistoryChangedEventHandler::create(Box::new(move |core, _| {
                        if let Some(core) = core {
                            report(&core, &history_callback, None)?;
                        }
                        Ok(())
                    })),
                    &mut history_token,
                )?;
                let mut completed_token = 0;
                core.add_NavigationCompleted(
                    &NavigationCompletedEventHandler::create(Box::new(move |core, args| {
                        if let (Some(core), Some(args)) = (core, args) {
                            let mut success = windows::core::BOOL::default();
                            args.IsSuccess(&mut success)?;
                            let completed = if success.as_bool() {
                                Ok(())
                            } else {
                                Err("Navigation failed or was cancelled by the browser boundary"
                                    .to_string())
                            };
                            report(&core, &callback, Some(completed))?;
                        }
                        Ok(())
                    })),
                    &mut completed_token,
                )?;
                let settings = core.Settings()?;
                settings.SetIsWebMessageEnabled(false)?;
                settings.SetAreHostObjectsAllowed(false)?;
                settings.SetAreDefaultScriptDialogsEnabled(true)?;
                settings.SetAreDevToolsEnabled(false)?;
                settings.SetAreDefaultContextMenusEnabled(true)?;
                settings.SetIsZoomControlEnabled(true)?;
                if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
                    settings3.SetAreBrowserAcceleratorKeysEnabled(true)?;
                }
                let settings4 = settings.cast::<ICoreWebView2Settings4>()?;
                settings4.SetIsPasswordAutosaveEnabled(false)?;
                settings4.SetIsGeneralAutofillEnabled(false)?;
                Ok(())
            }
        })();
        let _ = tx.send(result.map_err(|e| e.to_string()));
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(8), rx)
        .await
        .map_err(|_| "Browser hardening timed out")?
        .map_err(|_| "Browser hardening cancelled")??;
    super::interactions::install(view, host, target.tab_id).await?;
    Ok(())
}

pub async fn cdp(view: &Webview, method: &str, params: Value) -> Result<Value, String> {
    cdp_cancellable(view, method, params, None).await
}

#[cfg(windows)]
pub async fn cdp_cancellable(
    view: &Webview,
    method: &str,
    params: Value,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Result<Value, String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;
    let method = method.to_string();
    let params = params.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        if cancel.as_ref().is_some_and(|token| token.is_cancelled()) {
            let _ = tx.send(Err("cancelled_before_execution".into()));
            return;
        }
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        let fail_tx = tx.clone();
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                let core = native.controller().CoreWebView2()?;
                core.CallDevToolsProtocolMethod(
                    &HSTRING::from(method),
                    &HSTRING::from(params),
                    &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                        move |error, json| {
                            let result = if error.is_ok() {
                                serde_json::from_str::<Value>(&json.to_string())
                                    .map_err(|e| e.to_string())
                            } else {
                                Err(format!("Native browser method failed: {error:?}"))
                            };
                            if let Some(tx) = tx.lock().unwrap().take() {
                                let _ = tx.send(result);
                            }
                            Ok(())
                        },
                    )),
                )?;
                Ok(())
            }
        })();
        if let Err(error) = result {
            if let Some(tx) = fail_tx.lock().unwrap().take() {
                let _ = tx.send(Err(error.to_string()));
            }
        }
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(12), rx)
        .await
        .map_err(|_| "Native browser method timed out")?
        .map_err(|_| "Native browser method cancelled")?
}

#[cfg(not(windows))]
pub async fn harden(
    _view: &Webview,
    _proxy: std::sync::Arc<super::network::BrowserProxy>,
    _callback: std::sync::Arc<dyn Fn(NativeStatus) + Send + Sync>,
    _host: super::NativeBrowserHost,
    _target: super::BrowserTarget,
) -> Result<(), String> {
    Err("The isolated native browser is currently supported on Windows only".into())
}
#[cfg(not(windows))]
pub async fn cdp_cancellable(
    _view: &Webview,
    _method: &str,
    _params: Value,
    _cancel: Option<tokio_util::sync::CancellationToken>,
) -> Result<Value, String> {
    Err("Native browser capture is currently supported on Windows only".into())
}

#[cfg(windows)]
pub async fn navigation(
    view: &Webview,
    action: &str,
    url: Option<String>,
    proxy: Option<std::sync::Arc<super::network::BrowserProxy>>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    use windows::core::HSTRING;
    let action = action.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        if cancel.is_cancelled() {
            let _ = tx.send(Err("cancelled_before_execution".into()));
            return;
        }
        let result = (|| -> Result<(), String> {
            unsafe {
                let core = native
                    .controller()
                    .CoreWebView2()
                    .map_err(|e| e.to_string())?;
                match action.as_str() {
                    "navigate" => {
                        let raw = url.as_deref().ok_or("Navigation address is missing")?;
                        let parsed =
                            tauri::Url::parse(raw).map_err(|_| "Invalid browser address")?;
                        if let Some(proxy) = proxy {
                            if parsed.host_str().is_some_and(|h| {
                                h == "localhost" || h.parse::<std::net::IpAddr>().is_ok()
                            }) {
                                proxy.authorize_local_origin(raw)?;
                            }
                            if !proxy.permits_navigation(raw) {
                                return Err("Browser address is blocked".into());
                            }
                        }
                        if cancel.is_cancelled() {
                            return Err("cancelled_before_execution".into());
                        }
                        core.Navigate(&HSTRING::from(raw))
                            .map_err(|e| e.to_string())?;
                    }
                    "back" => core.GoBack().map_err(|e| e.to_string())?,
                    "forward" => core.GoForward().map_err(|e| e.to_string())?,
                    "reload" => core.Reload().map_err(|e| e.to_string())?,
                    "stop" => core.Stop().map_err(|e| e.to_string())?,
                    _ => return Err("Unsupported native navigation".into()),
                }
                Ok(())
            }
        })();
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(10), rx)
        .await
        .map_err(|_| "Native navigation timed out")?
        .map_err(|_| "Native navigation callback was interrupted")?
}
#[cfg(not(windows))]
pub async fn navigation(
    _view: &Webview,
    _action: &str,
    _url: Option<String>,
    _proxy: Option<std::sync::Arc<super::network::BrowserProxy>>,
    _cancel: tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    Err("Native browsing is currently available on Windows only".into())
}
