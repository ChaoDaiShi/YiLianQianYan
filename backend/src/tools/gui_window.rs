use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiWindow {
    pub id: u32,
    pub app_name: String,
    pub title: String,
    pub pid: u32,
    pub visible: bool,
    pub focused: bool,
    pub minimized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowQuery {
    Browser,
    Application(String),
}

pub trait WindowController: Send + Sync {
    fn observe(&self) -> Result<Vec<GuiWindow>, String>;
    fn restore_and_focus(&self, window_id: u32) -> Result<(), String>;
}

pub struct SystemWindowController;

impl WindowController for SystemWindowController {
    fn observe(&self) -> Result<Vec<GuiWindow>, String> {
        let windows = xcap::Window::all().map_err(|error| format!("无法读取桌面窗口: {error}"))?;
        Ok(windows
            .into_iter()
            .map(|window| GuiWindow {
                id: window.id(),
                app_name: window.app_name().to_string(),
                title: window.title().to_string(),
                pid: window.pid(),
                visible: window.width() > 0 && window.height() > 0 && !window.is_minimized(),
                focused: window.is_focused(),
                minimized: window.is_minimized(),
            })
            .collect())
    }

    #[cfg(windows)]
    fn restore_and_focus(&self, window_id: u32) -> Result<(), String> {
        use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
            ShowWindowAsync, SW_RESTORE,
        };

        let hwnd = window_id as usize as windows_sys::Win32::Foundation::HWND;
        if hwnd.is_null() {
            return Err("窗口句柄无效".to_string());
        }

        // Windows restricts foreground changes. Temporarily attach this
        // thread's input queue to the current foreground thread so a
        // user-approved launch can reliably restore an existing window.
        unsafe {
            let foreground = GetForegroundWindow();
            let current_thread = GetCurrentThreadId();
            let foreground_thread = if foreground.is_null() {
                0
            } else {
                GetWindowThreadProcessId(foreground, std::ptr::null_mut())
            };
            let attached = foreground_thread != 0
                && foreground_thread != current_thread
                && AttachThreadInput(current_thread, foreground_thread, 1) != 0;

            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);

            if attached {
                let _ = AttachThreadInput(current_thread, foreground_thread, 0);
            }
        }

        Ok(())
    }

    #[cfg(not(windows))]
    fn restore_and_focus(&self, _window_id: u32) -> Result<(), String> {
        Err("当前平台暂不支持主动置前桌面窗口".to_string())
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn application_query_name(value: &str) -> String {
    let path = std::path::Path::new(value);
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(normalize)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| normalize(value))
}

pub fn is_recognized_browser_app(app_name: &str) -> bool {
    let normalized = normalize(app_name);
    [
        "chrome",
        "googlechrome",
        "msedge",
        "microsoftedge",
        "firefox",
        "mozillafirefox",
        "brave",
        "bravebrowser",
        "vivaldi",
        "opera",
        "operagx",
        "arc",
        "chromium",
        "360chrome",
        "qqbrowser",
        "sogouexplorer",
        "maxthon",
    ]
    .iter()
    .any(|browser| normalized == *browser || normalized == format!("{browser}exe"))
}

pub fn window_matches(window: &GuiWindow, query: &WindowQuery) -> bool {
    match query {
        WindowQuery::Browser => is_recognized_browser_app(&window.app_name),
        WindowQuery::Application(application) => {
            let query = application_query_name(application);
            if query.is_empty() {
                return false;
            }
            let app = application_query_name(&window.app_name);
            let title = normalize(&window.title);
            app == query
                || title == query
                || app.contains(&query)
                || query.contains(&app)
                || title.contains(&query)
        }
    }
}

fn candidate_score(window: &GuiWindow, query: &WindowQuery, baseline: &HashSet<u32>) -> u8 {
    let mut score = 0;
    if !baseline.contains(&window.id) {
        score += 8;
    }
    if window.visible {
        score += 4;
    }
    if window.focused {
        score += 2;
    }
    if let WindowQuery::Application(application) = query {
        let query = application_query_name(application);
        let app = application_query_name(&window.app_name);
        let title = normalize(&window.title);
        if app == query || title == query {
            score += 16;
        }
    }
    score
}

pub fn matching_window_ids(
    controller: &dyn WindowController,
    query: &WindowQuery,
) -> Result<HashSet<u32>, String> {
    Ok(controller
        .observe()?
        .into_iter()
        .filter(|window| window_matches(window, query))
        .map(|window| window.id)
        .collect())
}

async fn observe_async(controller: &Arc<dyn WindowController>) -> Result<Vec<GuiWindow>, String> {
    let controller = Arc::clone(controller);
    tokio::task::spawn_blocking(move || controller.observe())
        .await
        .map_err(|error| format!("桌面窗口观察任务失败: {error}"))?
}

async fn restore_and_focus_async(
    controller: &Arc<dyn WindowController>,
    window_id: u32,
) -> Result<(), String> {
    let controller = Arc::clone(controller);
    tokio::task::spawn_blocking(move || controller.restore_and_focus(window_id))
        .await
        .map_err(|error| format!("桌面窗口置前任务失败: {error}"))?
}

pub async fn wait_for_visible_foreground_window(
    controller: Arc<dyn WindowController>,
    query: &WindowQuery,
    baseline: &HashSet<u32>,
    attempts: usize,
    interval: Duration,
) -> Result<GuiWindow, String> {
    let attempts = attempts.max(1);
    let mut last_error = "未检测到与请求匹配的桌面窗口".to_string();

    for attempt in 0..attempts {
        let mut candidates = observe_async(&controller)
            .await?
            .into_iter()
            .filter(|window| window_matches(window, query))
            .collect::<Vec<_>>();
        candidates
            .sort_by_key(|window| std::cmp::Reverse(candidate_score(window, query, baseline)));

        if let Some(candidate) = candidates.first() {
            if candidate.visible && candidate.focused {
                return Ok(candidate.clone());
            }

            match restore_and_focus_async(&controller, candidate.id).await {
                Ok(()) => {
                    let confirmed = observe_async(&controller)
                        .await?
                        .into_iter()
                        .find(|window| {
                            window.id == candidate.id && window.visible && window.focused
                        });
                    if let Some(window) = confirmed {
                        return Ok(window);
                    }
                    last_error = "已找到目标窗口，但未能确认它已显示在桌面前台".to_string();
                }
                Err(error) => last_error = error,
            }
        }

        if attempt + 1 < attempts {
            tokio::time::sleep(interval).await;
        }
    }

    Err(format!("{last_error}；不会把后台进程误报为已打开"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeWindowController {
        observations: Mutex<Vec<Vec<GuiWindow>>>,
        focused: Mutex<Vec<u32>>,
    }

    impl FakeWindowController {
        fn new(observations: Vec<Vec<GuiWindow>>) -> Self {
            Self {
                observations: Mutex::new(observations),
                focused: Mutex::new(Vec::new()),
            }
        }
    }

    impl WindowController for FakeWindowController {
        fn observe(&self) -> Result<Vec<GuiWindow>, String> {
            let mut observations = self.observations.lock().unwrap();
            if observations.len() > 1 {
                Ok(observations.remove(0))
            } else {
                Ok(observations.first().cloned().unwrap_or_default())
            }
        }

        fn restore_and_focus(&self, window_id: u32) -> Result<(), String> {
            self.focused.lock().unwrap().push(window_id);
            Ok(())
        }
    }

    fn window(id: u32, app: &str, title: &str, visible: bool, focused: bool) -> GuiWindow {
        GuiWindow {
            id,
            app_name: app.to_string(),
            title: title.to_string(),
            pid: id + 100,
            visible,
            focused,
            minimized: !visible,
        }
    }

    #[tokio::test]
    async fn restores_an_existing_minimized_matching_application() {
        let controller = Arc::new(FakeWindowController::new(vec![
            vec![window(7, "QQ.exe", "QQ", false, false)],
            vec![window(7, "QQ.exe", "QQ", true, true)],
        ]));

        let result = wait_for_visible_foreground_window(
            controller.clone(),
            &WindowQuery::Application("QQ".to_string()),
            &HashSet::from([7]),
            1,
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(result.id, 7);
        assert_eq!(*controller.focused.lock().unwrap(), vec![7]);
    }

    #[tokio::test]
    async fn prefers_a_new_matching_window_and_confirms_focus_after_activation() {
        let controller = Arc::new(FakeWindowController::new(vec![
            vec![
                window(1, "QQ.exe", "旧窗口", true, false),
                window(2, "QQ.exe", "QQ", true, false),
            ],
            vec![window(2, "QQ.exe", "QQ", true, true)],
        ]));

        let result = wait_for_visible_foreground_window(
            controller.clone(),
            &WindowQuery::Application("QQ".to_string()),
            &HashSet::from([1]),
            1,
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(result.id, 2);
        assert_eq!(*controller.focused.lock().unwrap(), vec![2]);
    }

    #[tokio::test]
    async fn refuses_success_when_focus_cannot_be_confirmed() {
        let controller = Arc::new(FakeWindowController::new(vec![
            vec![window(9, "QQ.exe", "QQ", true, false)],
            vec![window(9, "QQ.exe", "QQ", true, false)],
        ]));

        let error = wait_for_visible_foreground_window(
            controller,
            &WindowQuery::Application("QQ".to_string()),
            &HashSet::new(),
            1,
            Duration::ZERO,
        )
        .await
        .unwrap_err();

        assert!(error.contains("前台"));
    }

    #[test]
    fn browser_query_rejects_unrelated_foreground_windows() {
        assert!(!window_matches(
            &window(3, "QQ.exe", "QQ", true, true),
            &WindowQuery::Browser,
        ));
        assert!(window_matches(
            &window(4, "msedge.exe", "哔哩哔哩", true, true),
            &WindowQuery::Browser,
        ));
    }
}
