#[cfg(windows)]
#[tauri::command]
pub(crate) async fn pick_workspace_folder(view: tauri::Webview) -> Result<Option<String>, String> {
    use tauri::Manager;

    crate::browser::trusted_caller(&view)?;
    let app = view.app_handle().clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = view
            .window()
            .hwnd()
            .map_err(|error| error.to_string())
            .and_then(select_folder);
        let _ = send.send(result);
    })
    .map_err(|error| error.to_string())?;
    receive.await.map_err(|_| "文件夹选择已中断".to_string())?
}

#[cfg(windows)]
fn select_folder(owner: windows::Win32::Foundation::HWND) -> Result<Option<String>, String> {
    use windows::{
        core::{HRESULT, HSTRING},
        Win32::{
            System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            UI::Shell::{
                FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
                FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
            },
        },
    };

    // The desktop UI thread already owns the COM apartment for the webview.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| error.to_string())?;
        dialog
            .SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .map_err(|error| error.to_string())?;
        dialog
            .SetTitle(&HSTRING::from("选择工作空间文件夹"))
            .map_err(|error| error.to_string())?;
        match dialog.Show(Some(owner)) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(1223) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        }
        let path = dialog
            .GetResult()
            .and_then(|folder| folder.GetDisplayName(SIGDN_FILESYSPATH))
            .map_err(|error| error.to_string())?;
        Ok(Some(webview2_com::take_pwstr(path)))
    }
}

#[cfg(not(windows))]
#[tauri::command]
pub(crate) async fn pick_workspace_folder() -> Result<Option<String>, String> {
    Err("当前平台请手动输入文件夹路径".into())
}
