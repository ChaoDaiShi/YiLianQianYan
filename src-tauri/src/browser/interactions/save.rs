use super::*;
use windows::{
    core::HSTRING,
    Win32::{
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
        UI::Shell::{
            FileSaveDialog, IFileSaveDialog, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT,
            FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
        },
    },
};

pub(super) async fn download(
    host: &NativeBrowserHost,
    request: &BrowserUiRequest,
) -> Result<Value, String> {
    let id = request.id.clone();
    let tab_id = request.tab_id.clone();
    let suggested_name = request
        .suggested_name
        .clone()
        .unwrap_or_else(|| "download".into());
    let app = host.app.clone();
    on_ui(&host.app, move || unsafe {
        // The callback that captured this download has returned. Showing a modal native
        // dialog here therefore does not reenter a WebView2 event callback.
        let args = PENDING.with(|all| match all.borrow().get(&id).map(|p| &p.kind) {
            Some(PendingKind::Download {
                args, dialog: None, ..
            }) => Ok(args.clone()),
            Some(PendingKind::Download { .. }) => {
                Err("A save dialog is already open for this download".to_string())
            }
            _ => Err("Download request no longer exists".to_string()),
        })?;
        let view = app
            .get_webview(&tab_id)
            .ok_or("The download tab was closed")?;
        let owner = view.window().hwnd().map_err(|e| e.to_string())?;
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| e.to_string())?;
        dialog
            .SetOptions(FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_OVERWRITEPROMPT)
            .map_err(|e| e.to_string())?;
        dialog
            .SetTitle(&HSTRING::from("保存下载文件"))
            .map_err(|e| e.to_string())?;
        dialog
            .SetFileName(&HSTRING::from(suggested_name))
            .map_err(|e| e.to_string())?;
        PENDING.with(|all| {
            if let Some(Pending {
                kind: PendingKind::Download { dialog: active, .. },
                ..
            }) = all.borrow_mut().get_mut(&id)
            {
                *active = Some(dialog.clone());
            }
        });
        let shown = dialog.Show(Some(owner));
        PENDING.with(|all| {
            if let Some(Pending {
                kind: PendingKind::Download { dialog: active, .. },
                ..
            }) = all.borrow_mut().get_mut(&id)
            {
                *active = None;
            }
        });
        if !PENDING.with(|all| all.borrow().contains_key(&id)) {
            return Err("The download was cancelled because its tab closed".into());
        }
        if let Err(error) = shown {
            if error.code() == windows::core::HRESULT::from_win32(1223) {
                return Ok(json!({"saved":false,"cancelled":true}));
            }
            return Err(error.to_string());
        }
        let path = webview2_com::take_pwstr(
            dialog
                .GetResult()
                .and_then(|item| item.GetDisplayName(SIGDN_FILESYSPATH))
                .map_err(|e| e.to_string())?,
        );
        // Only the path selected by the native save dialog reaches WebView2.
        args.SetResultFilePath(&HSTRING::from(&path))
            .map_err(|e| e.to_string())?;
        args.SetCancel(false).map_err(|e| e.to_string())?;
        let pending = PENDING
            .with(|all| all.borrow_mut().remove(&id))
            .ok_or("Download request was cancelled")?;
        if let PendingKind::Download { deferral, .. } = pending.kind {
            deferral.Complete().map_err(|e| e.to_string())?;
        }
        Ok(json!({"saved":true,"download_started":true,"path":path}))
    })
    .await
}
