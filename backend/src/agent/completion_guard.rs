use crate::integrations::llm::types::ChatMessage;
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
    let instruction = normalized.replace("输入框", "控件");
    let has_text_action = ["输入", "键入", "打字", "写入", "写下", "填写", "填入"]
        .iter()
        .any(|term| instruction.contains(term))
        || ["type", "enter", "write"]
            .iter()
            .any(|term| contains_ascii_word(&normalized, term));

    has_desktop_context && has_text_action
}

pub fn evaluate_completion(messages: &[ChatMessage]) -> CompletionCheck {
    evaluate_turn_completion(messages, None)
}

pub(crate) fn evaluate_turn_completion(
    messages: &[ChatMessage],
    turn_user_start: Option<usize>,
) -> CompletionCheck {
    let Some(start) =
        turn_user_start.or_else(|| messages.iter().rposition(|message| message.role == "user"))
    else {
        return CompletionCheck::Ready;
    };
    let turn = &messages[start..];
    if turn.iter().skip(1).any(|message| {
        message.role == "user"
            && message
                .content
                .as_deref()
                .is_some_and(crate::interaction::chat_understanding::explicit_read_only)
    }) {
        return CompletionCheck::Ready;
    }
    let Some(request_index) = turn.iter().rposition(|message| {
        message.role == "user"
            && message
                .content
                .as_deref()
                .is_some_and(requests_desktop_text_input)
    }) else {
        return CompletionCheck::Ready;
    };

    let mut pending_text_calls = HashSet::new();
    for message in &turn[request_index + 1..] {
        if message.role == "assistant" {
            for tool_call in message.tool_calls.as_deref().unwrap_or_default() {
                let name = tool_call.function.name.as_str();
                if matches!(name, "software_wechat_draft") {
                    pending_text_calls.insert((tool_call.id.as_str(), name));
                    continue;
                }
                if !matches!(name, "keyboard" | "software_control") {
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
                if (name == "keyboard" && is_targeted_text_input)
                    || (name == "software_control" && arguments["action"] == "set_value")
                {
                    pending_text_calls.insert((tool_call.id.as_str(), name));
                }
            }
        } else if let ("tool", Some(id), Some(name), Some(content)) = (
            message.role.as_str(),
            message.tool_call_id.as_deref(),
            message.name.as_deref(),
            message.content.as_deref(),
        ) {
            if pending_text_calls.contains(&(id, name)) {
                let confirmed = match name {
                    "software_wechat_draft" => {
                        content.lines().next() == Some("WECHAT_DRAFT_VERIFIED")
                    }
                    "software_control" => {
                        content.lines().next()
                            == Some(crate::tools::software::VERIFIED_SOFTWARE_TEXT_INPUT_MARKER)
                    }
                    "keyboard" => content.contains(VERIFIED_DESKTOP_TEXT_INPUT_MARKER),
                    _ => false,
                };
                if confirmed {
                    return CompletionCheck::Ready;
                }
            }
        }
    }

    CompletionCheck::MissingDesktopTextInputEvidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::llm::types::{ToolCall, ToolCallFunction};

    fn message(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            reasoning_content: None,
            role: role.to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    fn tool_call(id: &str, name: &str, arguments: serde_json::Value) -> ChatMessage {
        ChatMessage {
            reasoning_content: None,
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
            reasoning_content: None,
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
