//! Metadata-only inspection of the controlled loopback fixture cookie.
use super::*;

#[cfg(windows)]
pub(super) async fn metadata(
    host: &NativeBrowserHost,
    request: BrowserRequest,
) -> Result<Value, String> {
    use webview2_com::{
        GetCookiesCompletedHandler, Microsoft::Web::WebView2::Win32::ICoreWebView2_2,
    };
    use windows::core::{Interface, HSTRING};

    let state = host
        .lookup(request.target.as_ref())
        .map_err(|e| e.to_string())?;
    let url = tauri::Url::parse(&state.tab.url).map_err(|e| e.to_string())?;
    if fixture_root().is_none() || !matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"))
    {
        return Err("Cookie metadata fixture requires an owned loopback test page".into());
    }
    let view = host.view(&state).map_err(|e| e.to_string())?;
    let actual_url = url.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(tx)));
    view.with_webview(move |native| {
        let callback_sender = sender.clone();
        let registered = (|| -> windows::core::Result<()> {
            unsafe {
                let core = native.controller().CoreWebView2()?;
                let manager = core.cast::<ICoreWebView2_2>()?.CookieManager()?;
                manager.GetCookies(
                    &HSTRING::from(actual_url),
                    &GetCookiesCompletedHandler::create(Box::new(move |status, cookies| {
                        let result = (|| -> windows::core::Result<Value> {
                            status?;
                            let mut metadata = Vec::new();
                            if let Some(cookies) = cookies {
                                let mut count = 0;
                                cookies.Count(&mut count)?;
                                for index in 0..count {
                                    let cookie = cookies.GetValueAtIndex(index)?;
                                    let mut name = windows::core::PWSTR::null();
                                    cookie.Name(&mut name)?;
                                    let name = webview2_com::take_pwstr(name);
                                    if name != "remember" {
                                        continue;
                                    }
                                    let mut domain = windows::core::PWSTR::null();
                                    cookie.Domain(&mut domain)?;
                                    let domain = webview2_com::take_pwstr(domain);
                                    let mut path = windows::core::PWSTR::null();
                                    cookie.Path(&mut path)?;
                                    let path = webview2_com::take_pwstr(path);
                                    let mut is_session = windows::core::BOOL::default();
                                    cookie.IsSession(&mut is_session)?;
                                    let mut expires = 0.0;
                                    cookie.Expires(&mut expires)?;
                                    metadata.push(json!({
                                        "name": name,
                                        "domain": domain,
                                        "path": path,
                                        "is_session": is_session.as_bool(),
                                        "expires": expires
                                    }));
                                }
                            }
                            Ok(json!({"cookies": metadata}))
                        })();
                        if let Some(tx) = callback_sender.lock().unwrap().take() {
                            let _ = tx.send(result.map_err(|error| error.to_string()));
                        }
                        Ok(())
                    })),
                )
            }
        })();
        if let Err(error) = registered {
            if let Some(tx) = sender.lock().unwrap().take() {
                let _ = tx.send(Err(error.to_string()));
            }
        }
    })
    .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(3), rx)
        .await
        .map_err(|_| "Cookie metadata snapshot timeout".to_string())?
        .map_err(|_| "Cookie metadata snapshot closed".to_string())?
}

#[cfg(not(windows))]
pub(super) async fn metadata(
    _host: &NativeBrowserHost,
    _request: BrowserRequest,
) -> Result<Value, String> {
    Err("Cookie metadata fixture requires Windows".into())
}
