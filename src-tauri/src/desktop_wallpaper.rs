use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopWallpapers {
    monitors: Vec<MonitorWallpaper>,
    providers: Vec<ProviderStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MonitorWallpaper {
    id: String,
    label: String,
    path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Provider {
    WallpaperEngine,
    Lively,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderStatus {
    id: Provider,
    running: bool,
    command_available: bool,
    version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopIntegrationStatus {
    rainmeter: ApplicationStatus,
    vtube_studio: ApplicationStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationStatus {
    running: bool,
    command_available: bool,
    version: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RainmeterAction {
    OpenManager,
    RefreshSkin { config: String },
}

impl RainmeterAction {
    fn arguments(&self) -> Result<Vec<&str>, String> {
        match self {
            Self::OpenManager => Ok(vec!["!Manage", "Skins"]),
            Self::RefreshSkin { config } => {
                let valid = !config.is_empty()
                    && config.chars().count() <= 160
                    && config.split('\\').all(|part| {
                        !part.trim().is_empty()
                            && part != "."
                            && part != ".."
                            && part
                                .chars()
                                .all(|c| c.is_alphanumeric() || " _-.".contains(c))
                    });
                if !valid {
                    return Err("请输入 Rainmeter 的皮肤配置名，例如 illustro\\Clock。".into());
                }
                Ok(vec!["!Refresh", config])
            }
        }
    }
}

#[cfg(windows)]
#[path = "desktop_wallpaper_windows.rs"]
mod platform;

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) async fn status(_view: tauri::Webview) -> Result<DesktopWallpapers, String> {
        Err("当前平台不支持 Windows 壁纸".into())
    }

    pub(super) async fn choose_and_apply(
        _view: tauri::Webview,
        _monitor: String,
        _preset: Option<String>,
    ) -> Result<Option<MonitorWallpaper>, String> {
        Err("当前平台不支持 Windows 壁纸".into())
    }

    pub(super) async fn playback(_provider: Provider, _play: bool) -> Result<(), String> {
        Err("当前平台不支持外部壁纸控制".into())
    }
    pub(super) async fn engine_preview(
        _view: tauri::Webview,
        _open: bool,
    ) -> Result<Option<bool>, String> {
        Err("当前平台不支持 Wallpaper Engine 预览".into())
    }
    pub(super) fn integration_status() -> Result<DesktopIntegrationStatus, String> {
        Err("当前平台不支持 Windows 桌面集成".into())
    }
    pub(super) async fn rainmeter(_action: RainmeterAction) -> Result<(), String> {
        Err("当前平台不支持 Rainmeter 控制".into())
    }
}

#[tauri::command]
pub(crate) async fn desktop_wallpaper_status(
    view: tauri::Webview,
) -> Result<DesktopWallpapers, String> {
    crate::desktop_perception::trusted_main(&view)?;
    platform::status(view).await
}

#[tauri::command]
pub(crate) async fn desktop_wallpaper_apply(
    view: tauri::Webview,
    monitor: String,
    preset: Option<String>,
) -> Result<Option<MonitorWallpaper>, String> {
    crate::desktop_perception::trusted_main(&view)?;
    platform::choose_and_apply(view, monitor, preset).await
}

#[tauri::command]
pub(crate) async fn desktop_wallpaper_playback(
    view: tauri::Webview,
    provider: Provider,
    play: bool,
) -> Result<(), String> {
    crate::desktop_perception::trusted_main(&view)?;
    platform::playback(provider, play).await
}

#[tauri::command]
pub(crate) async fn desktop_wallpaper_engine_preview(
    view: tauri::Webview,
    open: bool,
) -> Result<Option<bool>, String> {
    crate::desktop_perception::trusted_main(&view)?;
    platform::engine_preview(view, open).await
}

#[tauri::command]
pub(crate) fn desktop_integration_status(
    view: tauri::Webview,
) -> Result<DesktopIntegrationStatus, String> {
    crate::desktop_perception::trusted_main(&view)?;
    platform::integration_status()
}

#[tauri::command]
pub(crate) async fn desktop_rainmeter_invoke(
    view: tauri::Webview,
    action: RainmeterAction,
) -> Result<(), String> {
    crate::desktop_perception::trusted_main(&view)?;
    platform::rainmeter(action).await
}

impl Provider {
    fn play_command(self, play: bool) -> [&'static str; 3] {
        match self {
            Self::WallpaperEngine => ["-control", if play { "play" } else { "pause" }, ""],
            Self::Lively => ["app", "--play", if play { "true" } else { "false" }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Provider, RainmeterAction};

    #[test]
    fn provider_playback_uses_the_public_command_for_each_engine() {
        assert_eq!(
            Provider::WallpaperEngine.play_command(false),
            ["-control", "pause", ""]
        );
        assert_eq!(
            Provider::WallpaperEngine.play_command(true),
            ["-control", "play", ""]
        );
        assert_eq!(
            Provider::Lively.play_command(false),
            ["app", "--play", "false"]
        );
        assert_eq!(
            Provider::Lively.play_command(true),
            ["app", "--play", "true"]
        );
    }

    #[test]
    fn rainmeter_refresh_is_scoped_to_one_named_skin() {
        let refresh = |config: &str| RainmeterAction::RefreshSkin {
            config: config.into(),
        };
        assert_eq!(
            refresh("illustro\\Clock").arguments().unwrap(),
            ["!Refresh", "illustro\\Clock"]
        );
        assert!(refresh("*").arguments().is_err());
        assert!(refresh("時計][!Quit]").arguments().is_err());
    }
}
