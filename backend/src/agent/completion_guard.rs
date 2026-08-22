use crate::llm::types::ChatMessage;
use crate::tools::input::VERIFIED_DESKTOP_TEXT_INPUT_MARKER;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionCheck {
    Ready,
    MissingDesktopTextInputEvidence,
}

fn contains_ascii_word(value: &str, expected: &str) -> bool {
    value
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|word| word.eq_ignore_ascii_case(expected))
}

fn requests_desktop_text_input(content: &str) -> bool {
    let normalized = content.to_lowercase();
    let has_desktop_context = [
        "记事本",
        "窗口",
        "应用",
        "页面",
        "文本框",
        "输入框",
        "写字板",
        "浏览器",
        "qq",
        "微信",
    ]
    .iter()
    .any(|term| normalized.contains(term))
        || [
            "notepad",
            "window",
            "application",
            "app",
            "page",
            "textbox",
            "browser",
            "word",
        ]
        .iter()
        .any(|term| contains_ascii_word(&normalized, term));
    let has_text_action = ["输入", "键入", "打字", "写入", "写下", "填写"]
        .iter()
        .any(|term| normalized.contains(term))
        || ["type", "enter", "write"]
            .iter()
            .any(|term| contains_ascii_word(&normalized, term));

    has_desktop_context && has_text_action
}

pub fn evaluate_completion(messages: &[ChatMessage]) -> CompletionCheck {
    let Some(latest_user_index) = messages.iter().rposition(|message| message.role == "user")
    else {
        return CompletionCheck::Ready;
    };
    let user_content = messages[latest_user_index].content.as_deref().unwrap_or("");
    if !requests_desktop_text_input(user_content) {
        return CompletionCheck::Ready;
    }

    let mut pending_keyboard_calls = HashSet::new();
    for message in &messages[latest_user_index + 1..] {
        if message.role == "assistant" {
            for tool_call in message.tool_calls.as_deref().unwrap_or_default() {
                if tool_call.function.name != "keyboard" {
                    continue;
                }
                let Ok(arguments) =
                    serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
                else {
                    continue;
                };
                let is_targeted_text_input = arguments["action"].as_str() == Some("type")
                    && arguments["target_application"]
                        .as_str()
                        .is_some_and(|target| !target.trim().is_empty());
                if is_targeted_text_input {
                    pending_keyboard_calls.insert(tool_call.id.as_str());
                }
            }
        } else if message.role == "tool"
            && message.name.as_deref() == Some("keyboard")
            && message
                .tool_call_id
                .as_deref()
                .is_some_and(|id| pending_keyboard_calls.contains(id))
            && message
                .content
                .as_deref()
                .is_some_and(|content| content.contains(VERIFIED_DESKTOP_TEXT_INPUT_MARKER))
        {
            return CompletionCheck::Ready;
        }
    }

    CompletionCheck::MissingDesktopTextInputEvidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::{ToolCall, ToolCallFunction};

    fn message(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    fn tool_call(id: &str, name: &str, arguments: serde_json::Value) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: id.to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: name.to_string(),
                    arguments: arguments.to_string(),
                },
            }]),
            tool_call_id: None,
            name: None,
        }
    }

    fn tool_result(id: &str, name: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: Some(id.to_string()),
            name: Some(name.to_string()),
        }
    }

    fn verified_keyboard_result(id: &str) -> ChatMessage {
        tool_result(
            id,
            "keyboard",
            &format!("文本已输入目标应用 Notepad；{VERIFIED_DESKTOP_TEXT_INPUT_MARKER}"),
        )
    }

    #[test]
    fn launch_evidence_alone_does_not_complete_a_desktop_text_task() {
        let messages = vec![
            message("user", "打开记事本，然后在新的页面写入你好世界"),
            tool_call(
                "call-open",
                "open_application",
                serde_json::json!({"application": "Notepad"}),
            ),
            tool_result(
                "call-open",
                "open_application",
                "记事本窗口已显示在桌面前台",
            ),
        ];

        assert_eq!(
            evaluate_completion(&messages),
            CompletionCheck::MissingDesktopTextInputEvidence
        );
    }

    #[test]
    fn verified_keyboard_result_completes_the_latest_desktop_text_task() {
        let messages = vec![
            message("user", "打开记事本，然后在新的页面写入你好世界"),
            tool_call(
                "call-type",
                "keyboard",
                serde_json::json!({
                    "action": "type",
                    "text": "你好世界",
                    "target_application": "Notepad"
                }),
            ),
            verified_keyboard_result("call-type"),
        ];

        assert_eq!(evaluate_completion(&messages), CompletionCheck::Ready);
    }

    #[test]
    fn failed_keyboard_result_is_not_completion_evidence() {
        let messages = vec![
            message("user", "打开记事本并输入你好世界"),
            tool_call(
                "call-type",
                "keyboard",
                serde_json::json!({
                    "action": "type",
                    "text": "你好世界",
                    "target_application": "Notepad"
                }),
            ),
            tool_result("call-type", "keyboard", "文本输入失败：系统拒绝输入"),
        ];

        assert_eq!(
            evaluate_completion(&messages),
            CompletionCheck::MissingDesktopTextInputEvidence
        );
    }

    #[test]
    fn evidence_before_the_latest_user_message_is_ignored() {
        let messages = vec![
            message("user", "在记事本输入旧内容"),
            tool_call(
                "call-old",
                "keyboard",
                serde_json::json!({
                    "action": "type",
                    "text": "旧内容",
                    "target_application": "Notepad"
                }),
            ),
            verified_keyboard_result("call-old"),
            message("user", "打开记事本并输入新内容"),
            tool_call(
                "call-open-new",
                "open_application",
                serde_json::json!({"application": "Notepad"}),
            ),
            tool_result(
                "call-open-new",
                "open_application",
                "记事本窗口已显示在桌面前台",
            ),
        ];

        assert_eq!(
            evaluate_completion(&messages),
            CompletionCheck::MissingDesktopTextInputEvidence
        );
    }

    #[test]
    fn ordinary_file_writes_do_not_require_keyboard_evidence() {
        let messages = vec![message("user", "将你好世界写入 notes.txt 文件")];

        assert_eq!(evaluate_completion(&messages), CompletionCheck::Ready);
    }
}
