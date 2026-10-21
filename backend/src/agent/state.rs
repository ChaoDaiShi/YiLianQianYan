// ============================================================
// Agent state — conversation history management
// ============================================================

use crate::integrations::llm::types::ChatMessage;

/// Running state for a single agent conversation session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentState {
    /// The system prompt that defines agent behavior
    pub system_prompt: String,
    /// Accumulated conversation messages (including system, user, assistant, tool)
    pub messages: Vec<ChatMessage>,
    /// The current user input being processed
    pub input: String,
    /// The final output (for streaming accumulation)
    pub output: String,
    /// The validated ProofLoop goal bound to this conversation, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_ref: Option<crate::agent::delegation::GoalRef>,
}

impl AgentState {
    /// Create a new agent state with a system prompt
    pub fn new(system_prompt: String) -> Self {
        let mut messages = Vec::new();
        messages.push(ChatMessage {
            reasoning_content: None,
            role: "system".to_string(),
            content: Some(system_prompt.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        Self {
            system_prompt,
            messages,
            input: String::new(),
            output: String::new(),
            goal_ref: None,
        }
    }

    /// Add a user message to the conversation
    pub fn add_user_message(&mut self, content: String) {
        self.input = content.clone();
        self.messages.push(ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    /// Add a transient system instruction used to correct the active run.
    pub fn add_system_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            reasoning_content: None,
            role: "system".to_string(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    /// Add an assistant message to the conversation
    pub fn add_assistant_message(
        &mut self,
        content: Option<String>,
        tool_calls: Option<Vec<crate::integrations::llm::types::ToolCall>>,
    ) {
        self.messages.push(ChatMessage {
            reasoning_content: None,
            role: "assistant".to_string(),
            content,
            tool_calls,
            tool_call_id: None,
            name: None,
        });
    }

    /// Add a tool result message to the conversation
    pub fn add_tool_result(&mut self, tool_call_id: String, tool_name: String, result: String) {
        self.messages.push(ChatMessage {
            reasoning_content: None,
            role: "tool".to_string(),
            content: Some(result),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            name: Some(tool_name),
        });
    }

    /// Estimate the token count of the conversation (rough character-based approximation)
    pub fn estimate_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                let content_len = m.content.as_ref().map(|c| c.len()).unwrap_or(0);
                let tool_calls_len = m
                    .tool_calls
                    .as_ref()
                    .map(|tc| {
                        tc.iter()
                            .map(|t| t.function.arguments.len() + t.function.name.len())
                            .sum::<usize>()
                    })
                    .unwrap_or(0);
                // Rough estimate: 1 token ≈ 4 characters
                (content_len + tool_calls_len) / 4
            })
            .sum()
    }

    /// Reset state for a new conversation
    pub fn reset(&mut self) {
        self.messages.clear();
        self.input.clear();
        self.output.clear();
        self.goal_ref = None;

        // Re-add system prompt
        self.messages.push(ChatMessage {
            reasoning_content: None,
            role: "system".to_string(),
            content: Some(self.system_prompt.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    /// Load messages from persisted history (replaces current messages)
    pub fn load_history(&mut self, messages: Vec<ChatMessage>) {
        // Keep system prompt, replace rest
        let system_msg = self.messages.first().cloned();
        self.messages = messages;
        if let Some(sys) = system_msg {
            if self.messages.first().map(|m| m.role.as_str()) != Some("system") {
                self.messages.insert(0, sys);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AgentState;

    #[test]
    fn older_saved_agent_state_defaults_goal_ref_to_none() {
        let saved = serde_json::json!({
            "system_prompt": "system",
            "messages": [{"role": "system", "content": "system"}],
            "input": "",
            "output": ""
        });

        let state: AgentState = serde_json::from_value(saved).unwrap();
        assert!(state.goal_ref.is_none());
    }
}
