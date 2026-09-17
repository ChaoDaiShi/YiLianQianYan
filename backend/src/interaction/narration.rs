//! Deterministic product-language narration for Task status projections.

use serde::{Deserialize, Serialize};

use crate::task::{TaskExecutionControlState, TaskStatusProjection};

const MAX_NARRATION_CHARS: usize = 1_024;
const MAX_NARRATION_LABEL_CHARS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrationRequest {
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TaskNarrator;

impl TaskNarrator {
    pub fn new() -> Self {
        Self
    }

    pub fn narrate(projection: &TaskStatusProjection) -> NarrationRequest {
        let title = bounded_label(&projection.title, "当前任务");
        let mut text = match projection.overall_status.as_str() {
            "completed" => format!("{title}已完成，共完成 {} 项。", projection.completed_count),
            "paused" => format!("{title}已暂停。"),
            "cancelled" => format!("{title}已取消。"),
            "failed" => format!("{title}遇到问题。"),
            "working" => format!("{title}正在进行中。"),
            "runnable" => format!("{title}可以继续处理。"),
            "pending" => format!("{title}等待开始。"),
            _ => format!("{title}有新的进展。"),
        };

        // Completion has a deliberately concise canonical sentence.  Other
        // states append only bounded semantic counts and labels.
        if projection.overall_status != "completed" {
            let mut details = Vec::new();
            if projection.completed_count > 0 {
                details.push(format!("已完成 {} 项", projection.completed_count));
            }
            if projection.failed_count > 0 {
                details.push(format!("{} 项遇到问题", projection.failed_count));
            }
            if projection.running_count > 0 {
                let activity = projection
                    .current_node_summary
                    .as_deref()
                    .map(|value| bounded_label(value, "当前步骤"));
                details.push(match activity {
                    Some(activity) => format!("正在处理：{activity}"),
                    None => format!("正在处理 {} 项", projection.running_count),
                });
            }
            if projection.waiting_count > 0 {
                details.push(format!("有 {} 项等待处理", projection.waiting_count));
            }
            if projection.attention_required {
                details.push("需要你的确认".to_string());
            }
            if !details.is_empty() {
                text = format!(
                    "{}{}。",
                    text.trim_end_matches('。'),
                    format!("，{}", details.join("，"))
                );
            }
        }
        if matches!(projection.control_state, TaskExecutionControlState::Paused)
            && !text.contains("需要你的确认")
        {
            text = format!("{}需要你的确认。", text.trim_end_matches('。'));
        }
        NarrationRequest {
            text: truncate(&text, MAX_NARRATION_CHARS),
        }
    }
}

fn bounded_label(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        truncate(trimmed, MAX_NARRATION_LABEL_CHARS)
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
