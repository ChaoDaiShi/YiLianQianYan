use super::*;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, DownloadStartingEventHandler,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler,
    PermissionRequestedEventHandler, StateChangedEventHandler, WindowCloseRequestedEventHandler,
    ZoomFactorChangedEventHandler,
};
use windows::{
    core::{BOOL, PWSTR},
    Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
};

pub(super) fn install(
    native: tauri::webview::PlatformWebview,
    host: NativeBrowserHost,
    tab_id: String,
) -> windows::core::Result<()> {
    unsafe {
        let controller = native.controller();
        let core = controller.CoreWebView2()?;
        let popup_environment = native.environment();
        let close_host = host.clone();
        let close_tab = tab_id.clone();
        let mut close_token = 0;
        core.add_WindowCloseRequested(&WindowCloseRequestedEventHandler::create(Box::new(move |_,_| {
            // Wry destroys its child HWND for window.close(); retire the same managed
            // tab after callback return so its controller and context lease also close.
            let host = close_host.clone();
            let tab_id = close_tab.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(reason) = host.retire_and_dispose(&tab_id).await {
                    tracing::error!(tab_id=%tab_id,error=%reason,"Browser window.close disposal failed");
                }
                let _ = host.app.emit_to("main","browser:closed",json!({"tab_id":tab_id}));
            });
            Ok(())
        })), &mut close_token)?;
        let navigation_host = host.clone();
        let navigation_tab = tab_id.clone();
        let mut navigation_token = 0;
        core.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, _| {
                cancel_document(&navigation_tab);
                navigation_host
                    .interactions
                    .requests
                    .lock()
                    .unwrap()
                    .retain(|_, r| r.tab_id != navigation_tab);
                navigation_host.interaction_changed(&navigation_tab);
                Ok(())
            })),
            &mut navigation_token,
        )?;
        let popup_host = host.clone();
        let popup_tab = tab_id.clone();
        let mut token = 0;
        core.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                tracing::info!(tab_id=%popup_tab,"native browser NewWindowRequested callback entered");
                let Some(args) = args else {
                    return Ok(());
                };
                args.SetHandled(true)?;
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let url = webview2_com::take_pwstr(uri);
                let allowed = {
                    let tabs = popup_host.tabs.lock().unwrap();
                    tabs.get(&popup_tab).is_some_and(|state| {
                        url == "about:blank"
                            || tauri::Url::parse(&url).is_ok_and(|u| {
                                matches!(u.scheme(), "http" | "https")
                                    && u.username().is_empty()
                                    && u.password().is_none()
                                    && state.proxy.permits_navigation(&url)
                            })
                    })
                };
                if !allowed {
                    popup_host.update(&popup_tab, |state| {
                        state.tab.error =
                            Some("Popup address is blocked by the browser network boundary".into())
                    });
                    return Ok(());
                }
                let Some(proxy) = popup_host
                    .tabs
                    .lock()
                    .unwrap()
                    .get(&popup_tab)
                    .map(|state| state.proxy.clone())
                else {
                    return Ok(());
                };
                let mut human = BOOL::default();
                args.IsUserInitiated(&mut human)?;
                let Some(request) = popup_host.interaction_request(
                    &popup_tab,
                    "popup",
                    Some(url),
                    "此页面请求打开新标签页".into(),
                    None,
                ) else {
                    return Ok(());
                };
                let deferral = args.GetDeferral()?;
                PENDING.with(|all| {
                    all.borrow_mut().insert(
                        request.id.clone(),
                        Pending {
                            tab_id: popup_tab.clone(),
                            kind: PendingKind::Popup {
                                args,
                                deferral,
                                environment: popup_environment.clone(),
                                proxy,
                            },
                        },
                    )
                });
                tracing::info!(tab_id=%popup_tab, request_id=%request.id, is_user_initiated=human.as_bool(), "native browser popup request registered");
                popup_host.interaction_changed(&popup_tab);
                if human.as_bool() {
                    // No WebView creation in this COM callback; the owned host operation runs after return.
                    let host = popup_host.clone();
                    let tab_id = popup_tab.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(reason) = host.open_interaction_popup(request).await {
                            host.update(&tab_id, |state| {
                                state.tab.error = Some(format!("无法打开新标签页：{reason}"))
                            });
                            tracing::warn!(error=%reason, "Managed browser popup could not open");
                        }
                    });
                }
                Ok(())
            })),
            &mut token,
        )?;
        tracing::info!(tab_id=%tab_id, event_token=token,"native browser NewWindowRequested handler installed");

        let download_host = host.clone();
        let download_tab = tab_id.clone();
        let core4 = core.cast::<ICoreWebView2_4>()?;
        core4.add_DownloadStarting(&DownloadStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()); };
            args.SetHandled(true)?;
            let operation = args.DownloadOperation()?;
            let mut uri = PWSTR::null();
            operation.Uri(&mut uri)?;
            let url = webview2_com::take_pwstr(uri);
            let mut path = PWSTR::null();
            args.ResultFilePath(&mut path)?;
            let suggested_name = suggested_filename(&webview2_com::take_pwstr(path));
            let Some(request) = download_host.interaction_request(&download_tab, "download", Some(url.clone()), "选择保存位置后才会开始下载".into(), Some(suggested_name)) else {
                args.SetCancel(true)?;
                return Ok(());
            };
            let deferral = args.GetDeferral()?;
            PENDING.with(|all| all.borrow_mut().insert(request.id.clone(), Pending { tab_id:download_tab.clone(), kind:PendingKind::Download { args, deferral, dialog:None } }));
            let finished_host = download_host.clone();
            let finished_tab = download_tab.clone();
            let mut state_token = 0;
            operation.add_StateChanged(&StateChangedEventHandler::create(Box::new(move |operation, _| {
                let Some(operation) = operation else { return Ok(()); };
                let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
                operation.State(&mut state)?;
                if state != COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS {
                    let mut path = PWSTR::null();
                    operation.ResultFilePath(&mut path)?;
                    let path = webview2_com::take_pwstr(path);
                    let success = state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED;
                    emit_ui(&finished_host, &finished_tab, "browser:download", json!({"tab_id":finished_tab,"url":url,"completed":success,"path":if success {Some(path)} else {None}}));
                    let mut interruption = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NONE;
                    operation.InterruptReason(&mut interruption)?;
                    if !success && interruption != COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED {
                        finished_host.update(&finished_tab, |s| s.tab.error=Some("下载已取消或中断；可从原页面重新下载".into()));
                    }
                }
                Ok(())
            })), &mut state_token)?;
            download_host.update(&download_tab, |state| state.tab.loading = false);
            download_host.interaction_changed(&download_tab);
            Ok(())
        })), &mut token)?;

        let permission_host = host.clone();
        let permission_tab = tab_id.clone();
        core.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                args.cast::<ICoreWebView2PermissionRequestedEventArgs3>()?
                    .SetSavesInProfile(false)?;
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let url = webview2_com::take_pwstr(uri);
                let message = permission_message(kind);
                let Some(request) = permission_host.interaction_request(
                    &permission_tab,
                    "permission",
                    Some(url),
                    message,
                    None,
                ) else {
                    return Ok(());
                };
                let deferral = args.GetDeferral()?;
                PENDING.with(|all| {
                    all.borrow_mut().insert(
                        request.id.clone(),
                        Pending {
                            tab_id: permission_tab.clone(),
                            kind: PendingKind::Permission { args, deferral },
                        },
                    )
                });
                permission_host.interaction_changed(&permission_tab);
                Ok(())
            })),
            &mut token,
        )?;

        let shortcut_host = host.clone();
        let shortcut_tab = tab_id.clone();
        controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    return Ok(());
                }
                let mut key = 0;
                args.VirtualKey(&mut key)?;
                let control = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let alt = GetKeyState(VK_MENU.0 as i32) < 0;
                if let Some(action) = shortcut(key, control, shift, alt) {
                    let detached = shortcut_host
                        .app
                        .get_webview(&shortcut_tab)
                        .is_some_and(|view| view.window().label() != "main");
                    if detached && matches!(action, "next-tab" | "previous-tab") {
                        return Ok(());
                    }
                    args.SetHandled(true)?;
                    let host = shortcut_host.clone();
                    let tab_id = shortcut_tab.clone();
                    tauri::async_runtime::spawn(async move {
                        if detached && action == "new-tab" {
                            if let Err(reason) = host.new_tab_in_main(&tab_id).await {
                                host.update(&tab_id, |s| {
                                    s.tab.error =
                                        Some(format!("无法打开主窗口的新标签页：{reason}"))
                                });
                                return;
                            }
                            return;
                        }
                        let Some(content) = host.app.get_webview(&tab_id) else {
                            return;
                        };
                        let owner = content.window().label().to_string();
                        let Some(chrome) = host.app.get_webview(&owner) else {
                            return;
                        };
                        if let Err(reason) =
                            super::super::super::platform::focus_native(&chrome).await
                        {
                            host.update(&tab_id, |s| {
                                s.tab.error =
                                    Some(format!("无法将键盘焦点移回浏览器工具栏：{reason}"))
                            });
                            return;
                        }
                        if content.window().label() == owner {
                            let _ = host.app.emit_to(
                                &owner,
                                "browser:shortcut",
                                json!({"action":action,"tab_id":tab_id}),
                            );
                        }
                    });
                }
                Ok(())
            })),
            &mut token,
        )?;

        let zoom_host = host.clone();
        let zoom_tab = tab_id.clone();
        controller.add_ZoomFactorChanged(
            &ZoomFactorChangedEventHandler::create(Box::new(move |controller, _| {
                if let Some(controller) = controller {
                    let mut factor = 1.0;
                    controller.ZoomFactor(&mut factor)?;
                    zoom_host
                        .interactions
                        .zoom
                        .lock()
                        .unwrap()
                        .insert(zoom_tab.clone(), factor);
                    zoom_host.interaction_changed(&zoom_tab);
                }
                Ok(())
            })),
            &mut token,
        )?;
        let mut factor = 1.0;
        controller.ZoomFactor(&mut factor)?;
        host.interactions
            .zoom
            .lock()
            .unwrap()
            .insert(tab_id, factor);
    }
    Ok(())
}

fn suggested_filename(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("download")
        .to_string()
}

fn permission_message(kind: COREWEBVIEW2_PERMISSION_KIND) -> String {
    let resource = match kind {
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => "麦克风",
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => "摄像头",
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => "位置信息",
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => "通知",
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => "读取剪贴板",
        _ => return format!("此网站请求浏览器权限（类型 {}）；仅本次允许", kind.0),
    };
    format!("此网站请求使用{resource}；仅本次允许")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_suggestion_is_a_filename_not_a_destination() {
        assert_eq!(
            suggested_filename("C:\\Users\\one\\Downloads\\a.txt"),
            "a.txt"
        );
        assert_eq!(suggested_filename("/tmp/a.txt"), "a.txt");
        assert_eq!(suggested_filename(""), "download");
    }
}
