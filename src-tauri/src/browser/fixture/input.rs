//! Real native input only for an owned loopback document in the explicit fixture.
use super::*;

pub(super) async fn input(
    host: &NativeBrowserHost,
    request: BrowserRequest,
    key: Option<String>,
) -> Result<Value, String> {
    let state = host
        .lookup(request.target.as_ref())
        .map_err(|e| e.to_string())?;
    let url = tauri::Url::parse(&state.tab.url).map_err(|e| e.to_string())?;
    if fixture_root().is_none() || !matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"))
    {
        return Err("Native input fixture requires an owned loopback test page".into());
    }
    let chrome = request
        .payload
        .get("chrome")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let inspect = chrome && request.payload.get("inspect").and_then(Value::as_bool) == Some(true);
    let view = if chrome {
        if key.is_none() && !inspect {
            return Err("Chrome fixture input supports only fixed shortcuts".into());
        }
        host.app
            .get_webview(&format!("browser-shell-{}", state.tab.tab_id))
            .ok_or("Owned detached chrome required")?
    } else {
        host.view(&state).map_err(|e| e.to_string())?
    };
    if inspect {
        return platform::cdp(&view,"Runtime.evaluate",json!({"expression":"({url:location.href,ready:document.readyState,focused:document.hasFocus(),body:document.body?.innerText?.slice(0,2000),active:document.activeElement?.getAttribute('aria-label'),tabReady:!!document.querySelector('.browser-page-detached [role=tab][aria-selected=true]'),dialogs:!!document.querySelector('[role=dialog]')})","returnByValue":true})).await;
    }
    if let Some(key) = key {
        let mut result = shortcut(&view, &key).await?;
        result["surface"] = json!(if chrome { "chrome" } else { "content" });
        return Ok(result);
    }
    let selector = request
        .payload
        .get("selector")
        .and_then(Value::as_str)
        .ok_or("Selector required")?;
    let document = platform::cdp(&view, "DOM.getDocument", json!({})).await?;
    let node = platform::cdp(
        &view,
        "DOM.querySelector",
        json!({"nodeId":document["root"]["nodeId"],"selector":selector}),
    )
    .await?;
    let box_model =
        platform::cdp(&view, "DOM.getBoxModel", json!({"nodeId":node["nodeId"]})).await?;
    let quad = box_model["model"]["content"]
        .as_array()
        .filter(|q| q.len() == 8)
        .ok_or("Visible native element required")?;
    let x = (quad[0].as_f64().ok_or("Invalid native geometry")?
        + quad[4].as_f64().ok_or("Invalid native geometry")?)
        / 2.0;
    let y = (quad[1].as_f64().ok_or("Invalid native geometry")?
        + quad[5].as_f64().ok_or("Invalid native geometry")?)
        / 2.0;
    view.window().show().map_err(|e| e.to_string())?;
    view.window().set_focus().map_err(|e| e.to_string())?;
    platform::focus_native(&view).await?;
    platform::cdp(
        &view,
        "Input.dispatchMouseEvent",
        json!({"type":"mousePressed","x":x,"y":y,"button":"left","clickCount":1}),
    )
    .await?;
    platform::cdp(
        &view,
        "Input.dispatchMouseEvent",
        json!({"type":"mouseReleased","x":x,"y":y,"button":"left","clickCount":1}),
    )
    .await?;
    Ok(json!({"clicked":true,"selector":selector}))
}

#[cfg(windows)]
async fn shortcut(view: &Webview, key: &str) -> Result<Value, String> {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
            VK_CONTROL, VK_SHIFT,
        },
        WindowsAndMessaging::GetForegroundWindow,
    };
    let (code, shift) = match key {
        "Ctrl+L" => (0x4c, false),
        "Ctrl+T" => (0x54, false),
        "Ctrl+W" => (0x57, false),
        "Ctrl+Tab" => (0x09, false),
        "Ctrl+Shift+Tab" => (0x09, true),
        _ => return Err("Unsupported native fixture shortcut".into()),
    };
    view.window().show().map_err(|e| e.to_string())?;
    view.window().set_focus().map_err(|e| e.to_string())?;
    platform::focus_native(view).await?;
    let owner = view.window().hwnd().map_err(|e| e.to_string())?;
    unsafe {
        if GetForegroundWindow() != owner {
            return Err("Owned fixture is not the foreground window".into());
        }
        let event = |key, released| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(key),
                    dwFlags: if released {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    ..Default::default()
                },
            },
        };
        let mut input = vec![event(VK_CONTROL.0, false)];
        if shift {
            input.push(event(VK_SHIFT.0, false));
        }
        input.extend([event(code, false), event(code, true)]);
        if shift {
            input.push(event(VK_SHIFT.0, true));
        }
        input.push(event(VK_CONTROL.0, true));
        if SendInput(&input, std::mem::size_of::<INPUT>() as i32) != input.len() as u32 {
            return Err("Native shortcut input was not delivered".into());
        }
    }
    Ok(json!({"sent":key}))
}

#[cfg(not(windows))]
async fn shortcut(_view: &Webview, _key: &str) -> Result<Value, String> {
    Err("Native input fixture requires Windows".into())
}
