use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::gui_window::{
    matching_window_ids, wait_for_visible_foreground_window, SystemWindowController,
    WindowController, WindowQuery,
};
use super::trait_def::{RiskLevel, Tool, ToolExecutionContext, ToolResult};
use crate::safety::grant::parse_network_target;

const DEFAULT_WINDOW_ATTEMPTS: usize = 40;
const DEFAULT_WINDOW_INTERVAL: Duration = Duration::from_millis(250);

pub trait GuiLauncher: Send + Sync {
    fn launch(&self, target: &str) -> Result<(), String>;
}

struct SystemGuiLauncher;

impl GuiLauncher for SystemGuiLauncher {
    fn launch(&self, target: &str) -> Result<(), String> {
        open::that_detached(target).map_err(|error| format!("系统未能启动目标: {error}"))
    }
}

pub struct GuiLaunchService {
    launcher: Arc<dyn GuiLauncher>,
    controller: Arc<dyn WindowController>,
    attempts: usize,
    interval: Duration,
}

impl GuiLaunchService {
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(SystemGuiLauncher),
            Arc::new(SystemWindowController),
            DEFAULT_WINDOW_ATTEMPTS,
            DEFAULT_WINDOW_INTERVAL,
        )
    }

    fn with_dependencies(
        launcher: Arc<dyn GuiLauncher>,
        controller: Arc<dyn WindowController>,
        attempts: usize,
        interval: Duration,
    ) -> Self {
        Self {
            launcher,
            controller,
            attempts: attempts.max(1),
            interval,
        }
    }

    async fn launch_and_focus(
        &self,
        launch_target: &str,
        query: WindowQuery,
    ) -> Result<super::gui_window::GuiWindow, String> {
        let baseline_controller = Arc::clone(&self.controller);
        let baseline_query = query.clone();
        let baseline = tokio::task::spawn_blocking(move || {
            matching_window_ids(baseline_controller.as_ref(), &baseline_query)
        })
        .await
        .map_err(|error| format!("启动前桌面窗口观察任务失败: {error}"))??;
        self.launcher.launch(launch_target)?;
        if !self.interval.is_zero() {
            tokio::time::sleep(self.interval).await;
        }
        wait_for_visible_foreground_window(
            Arc::clone(&self.controller),
            &query,
            &baseline,
            self.attempts,
            self.interval,
        )
        .await
    }
}

impl Default for GuiLaunchService {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_open_url(raw_url: &str) -> Result<url::Url, String> {
    if raw_url.trim().is_empty() {
        return Err("网址不能为空".to_string());
    }
    parse_network_target(raw_url)?;
    let parsed = url::Url::parse(raw_url).map_err(|_| "网址格式无效".to_string())?;
    if parsed.host_str().is_none() {
        return Err("网址必须包含有效主机名".to_string());
    }
    Ok(parsed)
}

fn safe_url_display(url: &url::Url) -> String {
    let mut display = url.clone();
    display.set_query(None);
    display.set_fragment(None);
    display.to_string()
}

fn normalize_application_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn default_start_menu_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(app_data) = std::env::var_os("APPDATA") {
        roots.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    if let Some(program_data) = std::env::var_os("PROGRAMDATA") {
        roots.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    roots
}

fn collect_application_entries(root: &Path, depth: usize, output: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_application_entries(&path, depth + 1, output);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("");
        if extension.eq_ignore_ascii_case("lnk")
            || extension.eq_ignore_ascii_case("exe")
            || extension.eq_ignore_ascii_case("appref-ms")
        {
            output.push(path);
        }
    }
}

fn resolve_application_target(application: &str, roots: &[PathBuf]) -> Result<String, String> {
    let application = application.trim();
    if application.is_empty() {
        return Err("应用名称不能为空".to_string());
    }
    if application.chars().any(char::is_control) {
        return Err("应用名称包含无效字符".to_string());
    }

    let explicit = Path::new(application);
    if explicit.is_file() {
        return Ok(explicit.to_string_lossy().to_string());
    }
    if explicit.is_absolute() || application.contains(['\\', '/']) {
        return Err(format!("没有找到应用文件：{application}"));
    }
    if explicit
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Ok(application.to_string());
    }

    let query = normalize_application_name(application);
    let mut entries = Vec::new();
    for root in roots {
        collect_application_entries(root, 0, &mut entries);
    }
    entries.sort();

    let mut exact = Vec::new();
    let mut partial = Vec::new();
    for path in entries {
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .map(normalize_application_name)
            .unwrap_or_default();
        if name == query {
            exact.push(path);
        } else if name.contains(&query) || query.contains(&name) {
            partial.push(path);
        }
    }

    if let Some(path) = exact.first() {
        return Ok(path.to_string_lossy().to_string());
    }
    match partial.as_slice() {
        [path] => Ok(path.to_string_lossy().to_string()),
        [] => Err(format!(
            "没有在开始菜单中找到“{application}”；请使用准确应用名称或可执行文件路径"
        )),
        _ => Err(format!(
            "找到多个与“{application}”相近的应用；请提供更准确的名称"
        )),
    }
}

pub struct OpenUrlTool {
    service: GuiLaunchService,
}

impl OpenUrlTool {
    pub fn new() -> Self {
        Self {
            service: GuiLaunchService::new(),
        }
    }

    #[cfg(test)]
    fn with_service(service: GuiLaunchService) -> Self {
        Self { service }
    }
}

impl Default for OpenUrlTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for OpenUrlTool {
    fn name(&self) -> &str {
        "open_url"
    }

    fn description(&self) -> &str {
        "使用系统默认浏览器打开HTTP或HTTPS网页，并在确认浏览器窗口已恢复、可见且位于桌面前台后返回成功。打开网站必须使用此工具，不要使用bash。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "要显示在默认浏览器中的HTTP或HTTPS网址"
                }
            },
            "required": ["url"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("open_url requires SecurityExecutionGateway trusted context")
    }

    async fn execute_with_context(
        &self,
        args: serde_json::Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        let raw_url = args
            .get("url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let url = match validate_open_url(raw_url) {
            Ok(url) => url,
            Err(error) => return ToolResult::error(error),
        };
        let target = match parse_network_target(raw_url) {
            Ok(target) => target,
            Err(error) => return ToolResult::error(error),
        };
        let port = target
            .port
            .unwrap_or(if target.scheme == "http" { 80 } else { 443 });
        if context
            .authorized_network(&target.scheme, &target.host, port, "GET")
            .is_none()
            || !context.authorized_desktop("open_url", Some(raw_url))
        {
            return ToolResult::error("打开网页缺少安全网关创建的网络或桌面授权证据");
        }

        match self
            .service
            .launch_and_focus(url.as_str(), WindowQuery::Browser)
            .await
        {
            Ok(window) => ToolResult::success(format!(
                "已在 {} 中打开 {}，窗口已显示在桌面前台",
                window.app_name,
                safe_url_display(&url)
            )),
            Err(error) => ToolResult::error(format!("网页未能显示在桌面前台：{error}")),
        }
    }
}

pub struct OpenApplicationTool {
    service: GuiLaunchService,
    start_menu_roots: Vec<PathBuf>,
}

impl OpenApplicationTool {
    pub fn new() -> Self {
        Self {
            service: GuiLaunchService::new(),
            start_menu_roots: default_start_menu_roots(),
        }
    }

    #[cfg(test)]
    fn with_service_and_roots(service: GuiLaunchService, start_menu_roots: Vec<PathBuf>) -> Self {
        Self {
            service,
            start_menu_roots,
        }
    }
}

impl Default for OpenApplicationTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for OpenApplicationTool {
    fn name(&self) -> &str {
        "open_application"
    }

    fn description(&self) -> &str {
        "打开Windows桌面应用（例如QQ），并在确认应用窗口已恢复、可见且位于桌面前台后返回成功。打开GUI应用必须使用此工具，不要只检查进程。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "application": {
                    "type": "string",
                    "description": "开始菜单中的应用名称、可执行文件名或完整路径，例如QQ"
                }
            },
            "required": ["application"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("open_application requires SecurityExecutionGateway trusted context")
    }

    async fn execute_with_context(
        &self,
        args: serde_json::Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        let raw_application = args
            .get("application")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !context.authorized_desktop("open_application", Some(raw_application)) {
            return ToolResult::error("打开应用缺少安全网关创建的桌面授权证据");
        }
        let application = raw_application.trim();
        let launch_target = match resolve_application_target(application, &self.start_menu_roots) {
            Ok(target) => target,
            Err(error) => return ToolResult::error(error),
        };

        match self
            .service
            .launch_and_focus(
                &launch_target,
                WindowQuery::Application(application.to_string()),
            )
            .await
        {
            Ok(window) => ToolResult::success(format!(
                "已打开 {}，{} 窗口已显示在桌面前台",
                application, window.app_name
            )),
            Err(error) => {
                ToolResult::error(format!("{} 未能显示在桌面前台：{}", application, error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::grant::{AuthorizedResource, NetworkZone};
    use crate::tools::gui_window::{GuiWindow, WindowController};
    use crate::tools::Tool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeLauncher {
        targets: Mutex<Vec<String>>,
    }

    impl GuiLauncher for FakeLauncher {
        fn launch(&self, target: &str) -> Result<(), String> {
            self.targets.lock().unwrap().push(target.to_string());
            Ok(())
        }
    }

    struct FakeWindowController {
        observations: Mutex<Vec<Vec<GuiWindow>>>,
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

        fn restore_and_focus(&self, _window_id: u32) -> Result<(), String> {
            Ok(())
        }
    }

    fn window(id: u32, app: &str, visible: bool, focused: bool) -> GuiWindow {
        GuiWindow {
            id,
            app_name: app.to_string(),
            title: app.trim_end_matches(".exe").to_string(),
            pid: id + 10,
            visible,
            focused,
            minimized: !visible,
        }
    }

    fn service(observations: Vec<Vec<GuiWindow>>) -> GuiLaunchService {
        GuiLaunchService::with_dependencies(
            Arc::new(FakeLauncher {
                targets: Mutex::new(Vec::new()),
            }),
            Arc::new(FakeWindowController {
                observations: Mutex::new(observations),
            }),
            1,
            Duration::ZERO,
        )
    }

    fn context(resources: Vec<AuthorizedResource>) -> ToolExecutionContext {
        ToolExecutionContext::new_with_resources(
            Arc::new(crate::isolation::ManagedProcessRegistry::new()),
            "call-open-gui",
            resources,
        )
    }

    #[tokio::test]
    async fn open_url_fails_when_launcher_does_not_produce_a_foreground_browser() {
        let tool = OpenUrlTool::with_service(service(vec![vec![], vec![]]));
        let context = context(vec![
            AuthorizedResource::Network {
                scheme: "https".to_string(),
                host: "www.bilibili.com".to_string(),
                port: 443,
                method: "GET".to_string(),
                zone: NetworkZone::Public,
                grant_id: None,
                one_shot_approval: true,
            },
            AuthorizedResource::Desktop {
                action: "open_url".to_string(),
                target: Some("https://www.bilibili.com/".to_string()),
                one_shot_approval: true,
            },
        ]);

        let result = tool
            .execute_with_context(
                serde_json::json!({"url": "https://www.bilibili.com/"}),
                &context,
            )
            .await;

        assert!(!result.ok);
        assert!(result.content.contains("不会把后台进程误报"));
    }

    #[tokio::test]
    async fn open_url_succeeds_only_with_an_authorized_visible_foreground_browser() {
        let tool = OpenUrlTool::with_service(service(vec![
            vec![],
            vec![window(12, "msedge.exe", true, true)],
        ]));
        let context = context(vec![
            AuthorizedResource::Network {
                scheme: "https".to_string(),
                host: "www.bilibili.com".to_string(),
                port: 443,
                method: "GET".to_string(),
                zone: NetworkZone::Public,
                grant_id: None,
                one_shot_approval: true,
            },
            AuthorizedResource::Desktop {
                action: "open_url".to_string(),
                target: Some("https://www.bilibili.com/".to_string()),
                one_shot_approval: true,
            },
        ]);

        let result = tool
            .execute_with_context(
                serde_json::json!({"url": "https://www.bilibili.com/"}),
                &context,
            )
            .await;

        assert!(result.ok, "{}", result.content);
        assert!(result.content.contains("窗口已显示在桌面前台"));
    }

    #[tokio::test]
    async fn open_application_rejects_mismatched_desktop_authorization() {
        let tool = OpenApplicationTool::with_service_and_roots(service(vec![]), vec![]);
        let context = context(vec![AuthorizedResource::Desktop {
            action: "open_application".to_string(),
            target: Some("WeChat".to_string()),
            one_shot_approval: true,
        }]);

        let result = tool
            .execute_with_context(serde_json::json!({"application": "QQ"}), &context)
            .await;

        assert!(!result.ok);
        assert!(result.content.contains("授权证据"));
    }

    #[tokio::test]
    async fn open_application_succeeds_only_after_the_matching_window_is_focused() {
        let root = std::env::temp_dir().join(format!("yilian-start-menu-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let shortcut = root.join("QQ.lnk");
        std::fs::write(&shortcut, []).unwrap();
        let tool = OpenApplicationTool::with_service_and_roots(
            service(vec![
                vec![],
                vec![window(8, "QQ.exe", true, false)],
                vec![window(8, "QQ.exe", true, true)],
            ]),
            vec![root.clone()],
        );
        let context = context(vec![AuthorizedResource::Desktop {
            action: "open_application".to_string(),
            target: Some("QQ".to_string()),
            one_shot_approval: true,
        }]);

        let result = tool
            .execute_with_context(serde_json::json!({"application": "QQ"}), &context)
            .await;

        assert!(result.ok, "{}", result.content);
        assert!(result.content.contains("已显示在桌面前台"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn application_resolution_rejects_ambiguous_partial_matches() {
        let root = std::env::temp_dir().join(format!("yilian-start-menu-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("QQ音乐.lnk"), []).unwrap();
        std::fs::write(root.join("QQ浏览器.lnk"), []).unwrap();

        let error = resolve_application_target("QQ", &[root.clone()]).unwrap_err();

        assert!(error.contains("多个"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn url_validation_rejects_non_web_schemes_and_credentials() {
        assert!(validate_open_url("file:///C:/Windows/System32").is_err());
        assert!(validate_open_url("https://user:pass@example.com").is_err());
        assert!(validate_open_url("https://www.bilibili.com/").is_ok());
    }
}
