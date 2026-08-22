// ============================================================
// Input tools — mouse & keyboard simulation via enigo
// ============================================================

use async_trait::async_trait;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::collections::HashMap;
use std::sync::LazyLock;

use super::trait_def::{RiskLevel, Tool, ToolResult};

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

pub struct KeyboardTool;

#[async_trait]
impl Tool for KeyboardTool {
    fn name(&self) -> &str {
        "keyboard"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
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
        let action = args["action"].as_str().unwrap_or("");
        let mut enigo = match make_enigo() {
            Ok(e) => e,
            Err(e) => return ToolResult::error(e),
        };

        match action {
            "type" => {
                let text = args["text"].as_str().unwrap_or("");
                if text.is_empty() {
                    return ToolResult::error("type 操作需要 text 参数");
                }
                if let Err(e) = enigo.text(text) {
                    return ToolResult::error(format!("文本输入失败：{}", e));
                }
                ToolResult::success(format!("已输入文本：{}", text))
            }
            "press" => {
                let key_name = args["key"].as_str().unwrap_or("");
                if key_name.is_empty() {
                    return ToolResult::error("press 操作需要 key 参数");
                }
                let key = match resolve_key(key_name) {
                    Ok(k) => k,
                    Err(e) => return ToolResult::error(e),
                };
                if let Err(e) = enigo.key(key, Direction::Click) {
                    return ToolResult::error(format!("按键失败：{}", e));
                }
                ToolResult::success(format!("已按下：{}", key_name))
            }
            "combo" => {
                let keys: Vec<String> = args["keys"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                if keys.is_empty() {
                    return ToolResult::error("combo 操作需要 keys 数组");
                }

                let resolved: Vec<Key> = match keys
                    .iter()
                    .map(|k| resolve_key(k))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(v) => v,
                    Err(e) => return ToolResult::error(e),
                };

                // Press all keys in order
                for &k in &resolved {
                    if let Err(e) = enigo.key(k, Direction::Press) {
                        return ToolResult::error(format!("组合键按下失败：{}", e));
                    }
                }
                // Small pause for OS to register
                std::thread::sleep(std::time::Duration::from_millis(30));
                // Release in reverse order
                for &k in resolved.iter().rev() {
                    if let Err(e) = enigo.key(k, Direction::Release) {
                        return ToolResult::error(format!("组合键释放失败：{}", e));
                    }
                }

                ToolResult::success(format!("已按下组合键：{}", keys.join("+")))
            }
            "key_down" => {
                let key_name = args["key"].as_str().unwrap_or("");
                if key_name.is_empty() {
                    return ToolResult::error("key_down 操作需要 key 参数");
                }
                let key = match resolve_key(key_name) {
                    Ok(k) => k,
                    Err(e) => return ToolResult::error(e),
                };
                if let Err(e) = enigo.key(key, Direction::Press) {
                    return ToolResult::error(format!("按住按键失败：{}", e));
                }
                ToolResult::success(format!("已按住：{}", key_name))
            }
            "key_up" => {
                let key_name = args["key"].as_str().unwrap_or("");
                if key_name.is_empty() {
                    return ToolResult::error("key_up 操作需要 key 参数");
                }
                let key = match resolve_key(key_name) {
                    Ok(k) => k,
                    Err(e) => return ToolResult::error(e),
                };
                if let Err(e) = enigo.key(key, Direction::Release) {
                    return ToolResult::error(format!("释放按键失败：{}", e));
                }
                ToolResult::success(format!("已释放：{}", key_name))
            }
            _ => ToolResult::error(format!(
                "未知操作: {}，支持 type/press/combo/key_down/key_up",
                action
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_type_schema_exposes_a_target_application() {
        let schema = KeyboardTool.parameters();

        assert_eq!(
            schema["properties"]["target_application"]["type"],
            "string"
        );
        assert!(schema["properties"]["target_application"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("目标应用")));
    }
}
