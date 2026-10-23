use super::AgentState;
use crate::integrations::llm::types::{ChatMessage, ToolCall};
use std::collections::HashSet;

/// Validate the actual request sequence. This reports corruption without
/// deleting, reordering or fabricating persisted historical messages.
pub(crate) fn validate(messages: &[ChatMessage]) -> Result<(), String> {
    let mut pending = HashSet::new();
    for message in messages {
        if message.role == "tool" {
            let id = message
                .tool_call_id
                .as_deref()
                .ok_or("历史工具结果缺少调用ID，请回退对应用户消息后重发")?;
            if !pending.remove(id) {
                return Err("历史存在孤立或重复工具结果，请回退对应用户消息后重发".into());
            }
            continue;
        }
        if !pending.is_empty() {
            return Err("历史工具调用缺少配对结果，请回退对应用户消息后重发".into());
        }
        if let Some(calls) = &message.tool_calls {
            if message.role != "assistant" && !calls.is_empty() {
                return Err("历史工具调用角色无效".into());
            }
            for call in calls {
                if call.id.is_empty() || !pending.insert(call.id.as_str()) {
                    return Err("历史工具调用ID为空或重复".into());
                }
            }
        }
    }
    if !pending.is_empty() {
        return Err("历史仍有未完成工具调用，请先处理审批或回退对应用户消息".into());
    }
    Ok(())
}

impl AgentState {
    pub(crate) fn mark_tool_started(&mut self, id: &str) {
        self.started_tool_calls.insert(id.into());
    }

    /// A terminal failure must close the current emitted tool-call batch.
    /// Starts are observed at the gateway callback. No tool is executed here.
    pub(crate) fn close_interrupted_tool_batch(&mut self) {
        let mut pending: Vec<ToolCall> = Vec::new();
        for message in &self.messages {
            if message.role == "tool" {
                pending.retain(|call| Some(call.id.as_str()) != message.tool_call_id.as_deref());
            } else {
                // Never silently repair an older corrupt prefix.
                if !pending.is_empty() {
                    return;
                }
                if message.role == "assistant" {
                    pending = message.tool_calls.clone().unwrap_or_default();
                }
            }
        }
        for call in pending {
            let started = self.started_tool_calls.contains(&call.id);
            let content = serde_json::json!({
                "status":if started {"result_unknown"} else {"not_executed"},
                "reason":if started {"run_terminated_after_execution_started"} else {"run_terminated_before_execution_started"},
                "message":if started {"调用已经开始，但终止时未取得确定结果；不得视为成功或自动重复执行"} else {"本次运行已终止，此调用未开始执行"}
            }).to_string();
            self.add_tool_result(call.id, call.function.name, content);
        }
    }
}

#[cfg(test)]
mod tests;
