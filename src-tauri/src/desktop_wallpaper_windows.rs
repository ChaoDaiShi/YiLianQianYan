use super::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::Manager;
use windows::{
    core::{w, HSTRING, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND},
        Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
        },
        System::{
            Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER},
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            Shell::{
                DesktopWallpaper, FileOpenDialog, IDesktopWallpaper, IFileOpenDialog,
                DSS_SLIDESHOW, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId},
        },
    },
};

pub(super) async fn status(view: tauri::Webview) -> Result<DesktopWallpapers, String> {
    let running = running_providers()?;
    let providers = running
        .iter()
        .map(|entry| ProviderStatus {
            id: entry.provider,
            running: entry.running,
            command_available: entry.path.is_some(),
            version: entry.path.as_deref().and_then(file_version),
        })
        .collect();
    let monitors = on_main_thread(view, |_| {
        let desktop = wallpaper()?;
        let count = unsafe { desktop.GetMonitorDevicePathCount() }.map_err(stringify)?;
        (0..count)
            .map(|index| {
                let id = unsafe { desktop.GetMonitorDevicePathAt(index) }.map_err(stringify)?;
                let id = webview2_com::take_pwstr(id);
                monitor_wallpaper(&desktop, index, id)
            })
            .collect()
    })
    .await?;
    Ok(DesktopWallpapers {
        monitors,
        providers,
    })
}

pub(super) async fn choose_and_apply(
    view: tauri::Webview,
    monitor: String,
    preset: Option<String>,
) -> Result<Option<MonitorWallpaper>, String> {
    let file = match preset.as_deref() {
        Some("daylight") => Some(built_in(
            &view,
            "daylight",
            include_bytes!("../../frontend/public/backgrounds/daylight.png"),
        )?),
        Some("night") => Some(built_in(
            &view,
            "night",
            include_bytes!("../../frontend/public/backgrounds/night.png"),
        )?),
        Some(_) => return Err("未知的内置壁纸".into()),
        None => None,
    };
    on_main_thread(view, move |owner| {
        let path = match file {
            Some(path) => path,
            None => {
                let Some(path) = select_image(owner)? else {
                    return Ok(None);
                };
                path
            }
        };
        if running_providers()?.iter().any(|entry| entry.running) {
            return Err("请先在外部壁纸软件中结束当前会话，再设置 Windows 壁纸。".into());
        }
        let desktop = wallpaper()?;
        let status = unsafe { desktop.GetStatus() }.map_err(stringify)?;
        if status.0 & DSS_SLIDESHOW.0 != 0 {
            return Err(
                "Windows 当前使用幻灯片放映，请先在 Windows 个性化设置中切换为图片。".into(),
            );
        }
        let count = unsafe { desktop.GetMonitorDevicePathCount() }.map_err(stringify)?;
        let index = (0..count)
            .find(|&index| unsafe {
                desktop
                    .GetMonitorDevicePathAt(index)
                    .ok()
                    .map(|id| webview2_com::take_pwstr(id))
                    .as_deref()
                    == Some(monitor.as_str())
            })
            .ok_or("显示器已断开，请刷新列表。")?;
        let path = path.to_str().ok_or("壁纸路径无法转换为文本")?;
        let previous = monitor_wallpaper(&desktop, index, monitor.clone())?;
        let result = set_wallpaper(&desktop, &monitor, path)
            .and_then(|()| monitor_wallpaper(&desktop, index, monitor.clone()))
            .and_then(|entry| {
                if entry.path.eq_ignore_ascii_case(path) {
                    Ok(entry)
                } else {
                    Err("Windows 未确认新壁纸".into())
                }
            });
        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(error) => {
                let current = monitor_wallpaper(&desktop, index, monitor.clone())?;
                if current.path != previous.path {
                    set_wallpaper(&desktop, &monitor, &previous.path)
                        .map_err(|restore| format!("{error}；恢复原壁纸失败：{restore}"))?;
                }
                Err(error)
            }
        }
    })
    .await
}

fn wallpaper() -> Result<IDesktopWallpaper, String> {
    // COM is initialized on the Tauri UI thread.
    unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_LOCAL_SERVER) }.map_err(stringify)
}

fn monitor_wallpaper(
    desktop: &IDesktopWallpaper,
    index: u32,
    id: String,
) -> Result<MonitorWallpaper, String> {
    let monitor = HSTRING::from(&id);
    let rect = unsafe { desktop.GetMonitorRECT(&monitor) }.map_err(stringify)?;
    let path = unsafe { desktop.GetWallpaper(&monitor) }.map_err(stringify)?;
    Ok(MonitorWallpaper {
        id,
        label: format!(
            "显示器 {} · {}×{}",
            index + 1,
            rect.right - rect.left,
            rect.bottom - rect.top
        ),
        path: webview2_com::take_pwstr(path),
    })
}

fn set_wallpaper(desktop: &IDesktopWallpaper, monitor: &str, path: &str) -> Result<(), String> {
    unsafe { desktop.SetWallpaper(&HSTRING::from(monitor), &HSTRING::from(path)) }
        .map_err(stringify)
}

fn built_in(view: &tauri::Webview, preset: &str, content: &[u8]) -> Result<PathBuf, String> {
    let base = std::env::var_os("YILIAN_DATA_DIR")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| {
            view.path()
                .app_local_data_dir()
                .map_err(|error| error.to_string())
        })?;
    let directory = base.join("wallpapers");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{preset}.png"));
    if !path.is_file() {
        let pending = path.with_extension("tmp");
        std::fs::write(&pending, content).map_err(|error| error.to_string())?;
        std::fs::rename(pending, &path).map_err(|error| error.to_string())?;
    }
    Ok(path)
}

fn select_image(owner: HWND) -> Result<Option<PathBuf>, String> {
    let Some(path) = select_file(owner, "选择 Windows 壁纸图片")? else {
        return Ok(None);
    };
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "bmp") {
        return Err("请选择 JPG、PNG 或 BMP 图片。".into());
    }
    Ok(Some(path))
}

fn select_file(owner: HWND, title: &str) -> Result<Option<PathBuf>, String> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(stringify)?;
        dialog.SetTitle(&HSTRING::from(title)).map_err(stringify)?;
        dialog
            .SetOptions(FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)
            .map_err(stringify)?;
        match dialog.Show(Some(owner)) {
            Ok(()) => {}
            Err(error) if error.code() == windows::core::HRESULT::from_win32(1223) => {
                return Ok(None)
            }
            Err(error) => return Err(stringify(error)),
        }
        let path = dialog
            .GetResult()
            .and_then(|file| file.GetDisplayName(SIGDN_FILESYSPATH))
            .map_err(stringify)?;
        Ok(Some(PathBuf::from(webview2_com::take_pwstr(path))))
    }
}

async fn on_main_thread<T: Send + 'static>(
    view: tauri::Webview,
    run: impl FnOnce(HWND) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let app = view.app_handle().clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = view
            .window()
            .hwnd()
            .map_err(|error| error.to_string())
            .and_then(run);
        let _ = send.send(result);
    })
    .map_err(|error| error.to_string())?;
    receive.await.map_err(|_| "壁纸操作已中断")?
}

pub(super) async fn playback(provider: Provider, play: bool) -> Result<(), String> {
    let running = running_providers()?;
    if play && running.iter().filter(|entry| entry.running).count() > 1 {
        return Err("检测到两个外部壁纸软件同时运行，请先结束其中一个。".into());
    }
    let path = running
        .into_iter()
        .find(|entry| entry.provider == provider)
        .and_then(|entry| entry.path)
        .ok_or("没有找到正在运行且支持命令行的壁纸提供者")?;
    invoke_cli(
        &path,
        provider
            .play_command(play)
            .into_iter()
            .filter(|arg| !arg.is_empty()),
    )
    .await
}

const ENGINE_PREVIEW_NAME: &str = "YiLianQianYan Wallpaper Preview";

pub(super) async fn engine_preview(
    view: tauri::Webview,
    open: bool,
) -> Result<Option<bool>, String> {
    let [engine, _] = running_providers()?;
    let (Some(path), Some(pid)) = (engine.path, engine.pid) else {
        return Err("Wallpaper Engine 没有运行或命令行不可访问。".into());
    };
    if open {
        let Some(media) =
            on_main_thread(view, |owner| select_file(owner, "选择要预览的 MP4 视频")).await?
        else {
            return Ok(None);
        };
        if !media
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
        {
            return Err("请选择 MP4 视频。".into());
        }
        let media = media.to_str().ok_or("视频路径无法转换为文本")?;
        invoke_cli(
            &path,
            [
                "-control",
                "openWallpaper",
                "-file",
                media,
                "-playInWindow",
                ENGINE_PREVIEW_NAME,
                "-width",
                "480",
                "-height",
                "270",
            ],
        )
        .await?;
    } else {
        invoke_cli(
            &path,
            [
                "-control",
                "closeWallpaper",
                "-location",
                ENGINE_PREVIEW_NAME,
            ],
        )
        .await?;
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while engine_preview_open(pid) != open {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| "Wallpaper Engine 未确认预览窗口状态。")?;
    Ok(Some(open))
}

fn engine_preview_open(pid: u32) -> bool {
    unsafe {
        FindWindowW(None, &HSTRING::from(ENGINE_PREVIEW_NAME)).is_ok_and(|window| {
            let mut owner = 0;
            GetWindowThreadProcessId(window, Some(&mut owner));
            owner == pid
        })
    }
}

pub(super) fn integration_status() -> Result<DesktopIntegrationStatus, String> {
    let [rainmeter, studio] = find_processes([&["rainmeter.exe"], &["vtube studio.exe"]])?;
    let status = |process: ProcessState, cli: bool| ApplicationStatus {
        running: process.running,
        command_available: cli && process.path.is_some(),
        version: process.path.as_deref().and_then(file_version),
    };
    Ok(DesktopIntegrationStatus {
        rainmeter: status(rainmeter, true),
        // Version detection uses the executable. API availability is negotiated
        // separately against the port configured by the user.
        vtube_studio: status(studio, false),
    })
}

pub(super) async fn rainmeter(action: RainmeterAction) -> Result<(), String> {
    let args = action.arguments()?;
    let [process] = find_processes([&["rainmeter.exe"]])?;
    let path = process.path.ok_or("Rainmeter 没有运行或命令行不可访问。")?;
    invoke_cli(&path, args).await
}

async fn invoke_cli<'a>(
    path: &Path,
    args: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let mut command = tokio::process::Command::new(path);
    command
        .args(args)
        .current_dir(path.parent().unwrap_or(Path::new(".")))
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .kill_on_drop(true);
    let exit = tokio::time::timeout(Duration::from_secs(5), command.status())
        .await
        .map_err(|_| "桌面应用命令响应超时")?
        .map_err(|error| error.to_string())?;
    if exit.success() {
        Ok(())
    } else {
        Err(format!("桌面应用命令返回 {exit}"))
    }
}

struct RunningProvider {
    provider: Provider,
    running: bool,
    path: Option<PathBuf>,
    pid: Option<u32>,
}

fn running_providers() -> Result<[RunningProvider; 2], String> {
    let [engine, lively] = find_processes([
        &["wallpaper32.exe", "wallpaper64.exe"],
        &["lively.exe", "livelywpf.exe"],
    ])?;
    Ok([
        (Provider::WallpaperEngine, engine),
        (Provider::Lively, lively),
    ]
    .map(|(provider, state)| RunningProvider {
        provider,
        running: state.running,
        path: state.path,
        pid: state.pid,
    }))
}

#[derive(Default)]
struct ProcessState {
    running: bool,
    path: Option<PathBuf>,
    pid: Option<u32>,
}

fn find_processes<const N: usize>(exe_names: [&[&str]; N]) -> Result<[ProcessState; N], String> {
    let mut entries = std::array::from_fn(|_| ProcessState::default());
    // ToolHelp enumerates names without reading external program settings.
    unsafe {
        let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(stringify)?);
        let mut process = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        Process32FirstW(snapshot.0, &mut process).map_err(stringify)?;
        loop {
            let name = String::from_utf16_lossy(&process.szExeFile)
                .trim_end_matches('\0')
                .to_ascii_lowercase();
            if let Some(index) = exe_names
                .iter()
                .position(|options| options.contains(&name.as_str()))
            {
                let entry = &mut entries[index];
                entry.running = true;
                if entry.path.is_none() {
                    entry.path = process_path(process.th32ProcessID);
                    entry.pid = entry.path.as_ref().map(|_| process.th32ProcessID);
                }
            }
            if Process32NextW(snapshot.0, &mut process).is_err() {
                break;
            }
        }
    }
    Ok(entries)
}

fn file_version(path: &Path) -> Option<String> {
    let name = HSTRING::from(path.as_os_str());
    unsafe {
        let size = GetFileVersionInfoSizeW(&name, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&name, None, size, data.as_mut_ptr().cast()).ok()?;
        let mut value = std::ptr::null_mut();
        let mut length = 0;
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\"), &mut value, &mut length).as_bool()
            || length < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
        {
            return None;
        }
        let info = value.cast::<VS_FIXEDFILEINFO>().read_unaligned();
        Some(format!(
            "{}.{}.{}.{}",
            info.dwFileVersionMS >> 16,
            info.dwFileVersionMS & 0xffff,
            info.dwFileVersionLS >> 16,
            info.dwFileVersionLS & 0xffff
        ))
    }
}

fn process_path(id: u32) -> Option<PathBuf> {
    unsafe {
        let process = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, id).ok()?);
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .ok()?;
        use std::os::windows::ffi::OsStringExt;
        Some(std::ffi::OsString::from_wide(&path[..length as usize]).into())
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn stringify(error: windows::core::Error) -> String {
    error.to_string()
}
