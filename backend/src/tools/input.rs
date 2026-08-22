// ============================================================
// Input tools — mouse & keyboard simulation via enigo
// ============================================================

use async_trait::async_trait;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use super::gui_window::{
    wait_for_visible_foreground_window, window_matches, SystemWindowController, WindowController,
    WindowQuery,
};
use super::trait_def::{RiskLevel, Tool, ToolExecutionContext, ToolResult};

// ============================================================
// Key name → enigo::Key mapping
// ============================================================

static KEY_MAP: LazyLock<HashMap<&'static str, Key>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Modifiers
    m.insert("shift", Key::Shift);
    m.insert("control", Key::Control);
    m.insert("ctrl", Key::Control);
    m.insert("alt", Key::Alt);
    m.insert("meta", Key::Meta);
    m.insert("win", Key::Meta);
    m.insert("command", Key::Meta);
    m.insert("cmd", Key::Meta);

    // Navigation
    m.insert("up", Key::UpArrow);
    m.insert("down", Key::DownArrow);
    m.insert("left", Key::LeftArrow);
    m.insert("right", Key::RightArrow);
    m.insert("up_arrow", Key::UpArrow);
    m.insert("down_arrow", Key::DownArrow);
    m.insert("left_arrow", Key::LeftArrow);
    m.insert("right_arrow", Key::RightArrow);

    // Editing
    m.insert("return", Key::Return);
    m.insert("enter", Key::Return);
    m.insert("tab", Key::Tab);
    m.insert("space", Key::Space);
    m.insert("escape", Key::Escape);
    m.insert("esc", Key::Escape);
    m.insert("backspace", Key::Backspace);
    m.insert("delete", Key::Delete);
    m.insert("insert", Key::Insert);
    m.insert("home", Key::Home);
    m.insert("end", Key::End);
    m.insert("page_up", Key::PageUp);
    m.insert("page_down", Key::PageDown);

    // Lock keys
    m.insert("caps_lock", Key::CapsLock);
    m.insert("num_lock", Key::Numlock);

    // Misc
    m.insert("print_screen", Key::PrintScr);
    m.insert("pause", Key::Pause);

    // Function keys
    m.insert("f1", Key::F1);
    m.insert("f2", Key::F2);
    m.insert("f3", Key::F3);
    m.insert("f4", Key::F4);
    m.insert("f5", Key::F5);
    m.insert("f6", Key::F6);
    m.insert("f7", Key::F7);
    m.insert("f8", Key::F8);
    m.insert("f9", Key::F9);
    m.insert("f10", Key::F10);
    m.insert("f11", Key::F11);
    m.insert("f12", Key::F12);

    m
});

fn resolve_key(name: &str) -> Result<Key, String> {
    // First check the map
    if let Some(key) = KEY_MAP.get(name) {
        return Ok(*key);
    }

    // Single ASCII character → Layout
    let chars: Vec<char> = name.chars().collect();
    if chars.len() == 1 {
        let c = chars[0];
        if c.is_ascii() {
            return Ok(Key::Unicode(c));
        }
    }

    Err(format!(
        "未知按键: {}，请使用标准按键名（如 return、escape、ctrl、a-z、0-9 等）",
        name
    ))
}

fn parse_button(s: &str) -> Result<Button, String> {
    match s {
        "left" => Ok(Button::Left),
        "right" => Ok(Button::Right),
        "middle" => Ok(Button::Middle),
        other => Err(format!("无效的鼠标按钮: {}，支持 left/right/middle", other)),
    }
}

fn make_enigo() -> Result<Enigo, String> {
    Enigo::new(&Settings::default()).map_err(|e| format!("输入系统不可用：{}", e))
}

// ============================================================
// MouseTool
// ============================================================

pub struct MouseTool;

#[async_trait]
impl Tool for MouseTool {
    fn name(&self) -> &str {
        "mouse"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    fn description(&self) -> &str {
        "模拟鼠标操作。支持：move(移动)、click(单击)、double_click(双击)、drag(拖拽)、scroll(滚轮)。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["move", "click", "double_click", "drag", "scroll"],
                    "description": "操作类型"
                },
                "x": {
                    "type": "integer",
                    "description": "目标X坐标（move/click/double_click时使用）"
                },
                "y": {
                    "type": "integer",
                    "description": "目标Y坐标（move/click/double_click时使用）"
                },
                "from_x": {
                    "type": "integer",
                    "description": "拖拽起始X（drag时使用）"
                },
                "from_y": {
                    "type": "integer",
                    "description": "拖拽起始Y（drag时使用）"
                },
                "to_x": {
                    "type": "integer",
                    "description": "拖拽目标X（drag时使用）"
                },
                "to_y": {
                    "type": "integer",
                    "description": "拖拽目标Y（drag时使用）"
                },
                "button": {
                    "type": "string",
                    "enum": ["left", "right", "middle"],
                    "description": "鼠标按钮，默认left"
                },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "left", "right"],
                    "description": "滚轮方向（scroll时使用）"
                },
                "amount": {
                    "type": "integer",
                    "description": "滚动行数，默认3"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args["action"].as_str().unwrap_or("");
        let mut enigo = match make_enigo() {
            Ok(e) => e,
            Err(e) => return ToolResult::error(e),
        };

        match action {
            "move" => {
                let x = args["x"].as_i64().unwrap_or(0) as i32;
                let y = args["y"].as_i64().unwrap_or(0) as i32;
                if let Err(e) = enigo.move_mouse(x, y, Coordinate::Abs) {
                    return ToolResult::error(format!("鼠标移动失败：{}", e));
                }
                ToolResult::success(format!("鼠标已移动到 ({}, {})", x, y))
            }
            "click" | "double_click" => {
                let x = args["x"].as_i64().unwrap_or(0) as i32;
                let y = args["y"].as_i64().unwrap_or(0) as i32;
                let button = parse_button(args["button"].as_str().unwrap_or("left"));
                let button = match button {
                    Ok(b) => b,
                    Err(e) => return ToolResult::error(e),
                };

                // Move to position first
                if let Err(e) = enigo.move_mouse(x, y, Coordinate::Abs) {
                    return ToolResult::error(format!("鼠标移动失败：{}", e));
                }
                // Click
                if let Err(e) = enigo.button(button, Direction::Click) {
                    return ToolResult::error(format!("鼠标点击失败：{}", e));
                }
                if action == "double_click" {
                    // Small delay between clicks
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    if let Err(e) = enigo.button(button, Direction::Click) {
                        return ToolResult::error(format!("鼠标双击失败：{}", e));
                    }
                }

                let btn_name = args["button"].as_str().unwrap_or("left");
                let desc = if action == "double_click" {
                    format!("已在 ({}, {}) 执行 {} 双击", x, y, btn_name)
                } else {
                    format!("已在 ({}, {}) 执行 {} 单击", x, y, btn_name)
                };
                ToolResult::success(desc)
            }
            "drag" => {
                let from_x = args["from_x"].as_i64().unwrap_or(0) as i32;
                let from_y = args["from_y"].as_i64().unwrap_or(0) as i32;
                let to_x = args["to_x"].as_i64().unwrap_or(0) as i32;
                let to_y = args["to_y"].as_i64().unwrap_or(0) as i32;
                let button = parse_button(args["button"].as_str().unwrap_or("left"));
                let button = match button {
                    Ok(b) => b,
                    Err(e) => return ToolResult::error(e),
                };

                // Move to start, press, move to end, release
                if let Err(e) = enigo.move_mouse(from_x, from_y, Coordinate::Abs) {
                    return ToolResult::error(format!("鼠标移动失败：{}", e));
                }
                if let Err(e) = enigo.button(button, Direction::Press) {
                    return ToolResult::error(format!("鼠标按下失败：{}", e));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
                if let Err(e) = enigo.move_mouse(to_x, to_y, Coordinate::Abs) {
                    return ToolResult::error(format!("鼠标拖拽移动失败：{}", e));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
                if let Err(e) = enigo.button(button, Direction::Release) {
                    return ToolResult::error(format!("鼠标释放失败：{}", e));
                }

                ToolResult::success(format!(
                    "已从 ({}, {}) 拖拽到 ({}, {})",
                    from_x, from_y, to_x, to_y
                ))
            }
            "scroll" => {
                let dir = args["direction"].as_str().unwrap_or("down");
                let amount = args["amount"].as_i64().unwrap_or(3) as i32;

                let (axis, length) = match dir {
                    "up" => (Axis::Vertical, -amount),
                    "down" => (Axis::Vertical, amount),
                    "left" => (Axis::Horizontal, -amount),
                    "right" => (Axis::Horizontal, amount),
                    other => {
                        return ToolResult::error(format!(
                            "无效的滚轮方向: {}，支持 up/down/left/right",
                            other
                        ))
                    }
                };

                if let Err(e) = enigo.scroll(length, axis) {
                    return ToolResult::error(format!("滚轮操作失败：{}", e));
                }

                ToolResult::success(format!("滚轮向{}滚动 {} 行", dir, amount.abs()))
            }
            other => ToolResult::error(format!(
                "未知操作: {}，支持 move/click/double_click/drag/scroll",
                other
            )),
        }
    }
}

// ============================================================
// KeyboardTool
// ============================================================

const DEFAULT_KEYBOARD_WINDOW_ATTEMPTS: usize = 10;
const DEFAULT_KEYBOARD_WINDOW_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) const VERIFIED_DESKTOP_TEXT_INPUT_MARKER: &str = "目标窗口验证：通过";

trait TextInjector: Send + Sync {
    fn type_text(&self, text: &str) -> Result<(), String>;
}

struct EnigoTextInjector;

impl TextInjector for EnigoTextInjector {
    fn type_text(&self, text: &str) -> Result<(), String> {
        let mut enigo = make_enigo()?;
        enigo
            .text(text)
            .map_err(|error| format!("文本输入失败：{error}"))
    }
}

pub struct KeyboardTool {
    controller: Arc<dyn WindowController>,
    text_injector: Arc<dyn TextInjector>,
    window_attempts: usize,
    window_interval: Duration,
}

impl KeyboardTool {
    pub fn new() -> Self {
        Self {
            controller: Arc::new(SystemWindowController),
            text_injector: Arc::new(EnigoTextInjector),
            window_attempts: DEFAULT_KEYBOARD_WINDOW_ATTEMPTS,
            window_interval: DEFAULT_KEYBOARD_WINDOW_INTERVAL,
        }
    }

    #[cfg(test)]
    fn with_dependencies(
        controller: Arc<dyn WindowController>,
        text_injector: Arc<dyn TextInjector>,
        window_attempts: usize,
        window_interval: Duration,
    ) -> Self {
        Self {
            controller,
            text_injector,
            window_attempts: window_attempts.max(1),
            window_interval,
        }
    }

    fn execute_non_text_action(&self, args: serde_json::Value) -> ToolResult {
        let action = args["action"].as_str().unwrap_or("");
        let mut enigo = match make_enigo() {
            Ok(enigo) => enigo,
            Err(error) => return ToolResult::error(error),
        };

        match action {
            "press" => {
                let key_name = args["key"].as_str().unwrap_or("");
                if key_name.is_empty() {
                    return ToolResult::error("press 操作需要 key 参数");
                }
                let key = match resolve_key(key_name) {
                    Ok(key) => key,
                    Err(error) => return ToolResult::error(error),
                };
                if let Err(error) = enigo.key(key, Direction::Click) {
                    return ToolResult::error(format!("按键失败：{error}"));
                }
                ToolResult::success(format!("已按下：{key_name}"))
            }
            "combo" => {
                let keys: Vec<String> = args["keys"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|value| value.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                if keys.is_empty() {
                    return ToolResult::error("combo 操作需要 keys 数组");
                }

                let resolved: Vec<Key> = match keys
                    .iter()
                    .map(|key| resolve_key(key))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(keys) => keys,
                    Err(error) => return ToolResult::error(error),
                };

                for &key in &resolved {
                    if let Err(error) = enigo.key(key, Direction::Press) {
                        return ToolResult::error(format!("组合键按下失败：{error}"));
                    }
                }
                std::thread::sleep(Duration::from_millis(30));
                for &key in resolved.iter().rev() {
                    if let Err(error) = enigo.key(key, Direction::Release) {
                        return ToolResult::error(format!("组合键释放失败：{error}"));
                    }
                }

                ToolResult::success(format!("已按下组合键：{}", keys.join("+")))
            }
            "key_down" | "key_up" => {
                let key_name = args["key"].as_str().unwrap_or("");
                if key_name.is_empty() {
                    return ToolResult::error(format!("{action} 操作需要 key 参数"));
                }
                let key = match resolve_key(key_name) {
                    Ok(key) => key,
                    Err(error) => return ToolResult::error(error),
                };
                let direction = if action == "key_down" {
                    Direction::Press
                } else {
                    Direction::Release
                };
                if let Err(error) = enigo.key(key, direction) {
                    let operation = if action == "key_down" {
                        "按住"
                    } else {
                        "释放"
                    };
                    return ToolResult::error(format!("{operation}按键失败：{error}"));
                }
                let operation = if action == "key_down" {
                    "已按住"
                } else {
                    "已释放"
                };
                ToolResult::success(format!("{operation}：{key_name}"))
            }
            "type" => ToolResult::error("type 操作必须通过安全网关并指定 target_application"),
            _ => ToolResult::error(format!(
                "未知操作: {action}，支持 type/press/combo/key_down/key_up"
            )),
        }
    }
}

impl Default for KeyboardTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for KeyboardTool {
    fn name(&self) -> &str {
        "keyboard"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    fn description(&self) -> &str {
        "模拟键盘操作。支持：type(输入文本)、press(单键)、combo(组合键如ctrl+c)、key_down(按住)、key_up(释放)。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["type", "press", "combo", "key_down", "key_up"],
                    "description": "操作类型"
                },
                "text": {
                    "type": "string",
                    "description": "要输入的文本（action=type时使用）"
                },
                "target_application": {
                    "type": "string",
                    "description": "接收文本的目标应用名称（action=type时必须提供）"
                },
                "key": {
                    "type": "string",
                    "description": "按键名（action=press/key_down/key_up时使用），如 return、escape、tab、f1 等"
                },
                "keys": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "组合键数组（action=combo时使用），如 [\"ctrl\", \"c\"]"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        self.execute_non_text_action(args)
    }

    async fn execute_with_context(
        &self,
        args: serde_json::Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        if args["action"].as_str().unwrap_or("") != "type" {
            return self.execute_non_text_action(args);
        }

        let text = args["text"].as_str().unwrap_or("");
        if text.is_empty() {
            return ToolResult::error("type 操作需要 text 参数");
        }
        let target = args["target_application"].as_str().unwrap_or("").trim();
        if target.is_empty() {
            return ToolResult::error("type 操作需要 target_application 参数");
        }
        if !context.authorized_desktop("keyboard_type", Some(target)) {
            return ToolResult::error("文本输入缺少安全网关创建的目标应用授权证据");
        }

        let query = WindowQuery::Application(target.to_string());
        let target_window = match wait_for_visible_foreground_window(
            Arc::clone(&self.controller),
            &query,
            &HashSet::new(),
            self.window_attempts,
            self.window_interval,
        )
        .await
        {
            Ok(window) => window,
            Err(error) => {
                return ToolResult::error(format!(
                    "无法确认目标应用 {target} 已显示在桌面前台：{error}"
                ))
            }
        };

        let injector = Arc::clone(&self.text_injector);
        let text = text.to_string();
        match tokio::task::spawn_blocking(move || injector.type_text(&text)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return ToolResult::error(error),
            Err(error) => {
                return ToolResult::error(format!("文本输入任务执行失败：{error}"));
            }
        }

        let controller = Arc::clone(&self.controller);
        let windows = match tokio::task::spawn_blocking(move || controller.observe()).await {
            Ok(Ok(windows)) => windows,
            Ok(Err(error)) => {
                return ToolResult::error(format!("输入后无法读取目标窗口状态：{error}"));
            }
            Err(error) => {
                return ToolResult::error(format!("输入后窗口验证任务失败：{error}"));
            }
        };
        let still_foreground = windows.iter().any(|window| {
            window.id == target_window.id
                && window.visible
                && window.focused
                && window_matches(window, &query)
        });
        if !still_foreground {
            return ToolResult::error(format!(
                "文本已发送，但输入后无法确认目标应用 {target} 仍在桌面前台；任务未验证完成"
            ));
        }

        ToolResult::success(format!(
            "文本已输入目标应用 {target}；{VERIFIED_DESKTOP_TEXT_INPUT_MARKER}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::grant::AuthorizedResource;
    use crate::tools::gui_window::{GuiWindow, WindowController};
    use crate::tools::trait_def::ToolExecutionContext;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeWindowController {
        observations: Mutex<Vec<Vec<GuiWindow>>>,
    }

    impl FakeWindowController {
        fn new(observations: Vec<Vec<GuiWindow>>) -> Self {
            Self {
                observations: Mutex::new(observations),
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

        fn restore_and_focus(&self, _window_id: u32) -> Result<(), String> {
            Ok(())
        }
    }

    struct FakeTextInjector {
        result: Result<(), String>,
        texts: Mutex<Vec<String>>,
    }

    impl FakeTextInjector {
        fn succeeding() -> Self {
            Self {
                result: Ok(()),
                texts: Mutex::new(Vec::new()),
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                result: Err(message.to_string()),
                texts: Mutex::new(Vec::new()),
            }
        }
    }

    impl TextInjector for FakeTextInjector {
        fn type_text(&self, text: &str) -> Result<(), String> {
            self.texts.lock().unwrap().push(text.to_string());
            self.result.clone()
        }
    }

    fn notepad_window(focused: bool) -> GuiWindow {
        GuiWindow {
            id: 42,
            app_name: "notepad.exe".to_string(),
            title: "无标题 - 记事本".to_string(),
            pid: 420,
            visible: true,
            focused,
            minimized: false,
        }
    }

    fn context(target: &str) -> ToolExecutionContext {
        ToolExecutionContext::new_with_resources(
            Arc::new(crate::isolation::ManagedProcessRegistry::new()),
            "call-keyboard-type",
            vec![AuthorizedResource::Desktop {
                action: "keyboard_type".to_string(),
                target: Some(target.to_string()),
                one_shot_approval: true,
            }],
        )
    }

    fn type_args(target: &str) -> serde_json::Value {
        serde_json::json!({
            "action": "type",
            "text": "你好世界",
            "target_application": target
        })
    }

    fn keyboard_tool(
        observations: Vec<Vec<GuiWindow>>,
        injector: Arc<dyn TextInjector>,
    ) -> KeyboardTool {
        KeyboardTool::with_dependencies(
            Arc::new(FakeWindowController::new(observations)),
            injector,
            1,
            Duration::ZERO,
        )
    }

    #[test]
    fn keyboard_type_schema_exposes_a_target_application() {
        let schema = KeyboardTool::new().parameters();

        assert_eq!(schema["properties"]["target_application"]["type"], "string");
        assert!(schema["properties"]["target_application"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("目标应用")));
    }

    #[test]
    fn input_tool_risk_metadata_keeps_mouse_stable_and_marks_keyboard_high() {
        assert_eq!(MouseTool.risk_level(), RiskLevel::Medium);
        assert_eq!(KeyboardTool::new().risk_level(), RiskLevel::High);
        assert!(KeyboardTool::new().requires_approval());
    }

    #[tokio::test]
    async fn keyboard_type_rejects_a_mismatched_target_authorization() {
        let injector = Arc::new(FakeTextInjector::succeeding());
        let tool = keyboard_tool(
            vec![vec![notepad_window(true)], vec![notepad_window(true)]],
            injector.clone(),
        );

        let result = tool
            .execute_with_context(type_args("Notepad"), &context("WeChat"))
            .await;

        assert!(!result.ok);
        assert!(result.content.contains("授权"));
        assert!(injector.texts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn keyboard_type_refuses_to_inject_without_a_visible_target_window() {
        let injector = Arc::new(FakeTextInjector::succeeding());
        let tool = keyboard_tool(vec![vec![]], injector.clone());

        let result = tool
            .execute_with_context(type_args("Notepad"), &context("Notepad"))
            .await;

        assert!(!result.ok);
        assert!(result.content.contains("未检测到"));
        assert!(injector.texts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn keyboard_type_reports_success_only_after_target_verification() {
        let injector = Arc::new(FakeTextInjector::succeeding());
        let tool = keyboard_tool(
            vec![vec![notepad_window(true)], vec![notepad_window(true)]],
            injector.clone(),
        );

        let result = tool
            .execute_with_context(type_args("Notepad"), &context("Notepad"))
            .await;

        assert!(result.ok, "{}", result.content);
        assert!(result.content.contains(VERIFIED_DESKTOP_TEXT_INPUT_MARKER));
        assert!(!result.content.contains("你好世界"));
        assert_eq!(*injector.texts.lock().unwrap(), vec!["你好世界"]);
    }

    #[tokio::test]
    async fn keyboard_type_propagates_injection_failure_without_verification() {
        let injector = Arc::new(FakeTextInjector::failing("模拟输入被系统拒绝"));
        let tool = keyboard_tool(vec![vec![notepad_window(true)]], injector);

        let result = tool
            .execute_with_context(type_args("Notepad"), &context("Notepad"))
            .await;

        assert!(!result.ok);
        assert!(result.content.contains("模拟输入被系统拒绝"));
        assert!(!result.content.contains(VERIFIED_DESKTOP_TEXT_INPUT_MARKER));
    }

    #[tokio::test]
    async fn keyboard_type_fails_if_the_target_loses_foreground_after_injection() {
        let injector = Arc::new(FakeTextInjector::succeeding());
        let tool = keyboard_tool(
            vec![vec![notepad_window(true)], vec![notepad_window(false)]],
            injector,
        );

        let result = tool
            .execute_with_context(type_args("Notepad"), &context("Notepad"))
            .await;

        assert!(!result.ok);
        assert!(result.content.contains("输入后"));
        assert!(!result.content.contains(VERIFIED_DESKTOP_TEXT_INPUT_MARKER));
    }
}
