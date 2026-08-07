// ============================================================
// Agent state — conversation history management
// ============================================================

use crate::llm::types::ChatMessage;

/// Running state for a single agent conversation session
#[derive(Debug, Clone)]
pub struct AgentState {
    /// The system prompt that defines agent behavior
    pub system_prompt: String,
    /// Accumulated conversation messages (including system, user, assistant, tool)
    pub messages: Vec<ChatMessage>,
    /// The current user input being processed
    pub input: String,
    /// The final output (for streaming accumulation)
    pub output: String,
}

impl AgentState {
    /// Create a new agent state with a system prompt
    pub fn new(system_prompt: String) -> Self {
        let mut messages = Vec::new();
        messages.push(ChatMessage {
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
        }
    }

    /// Add a user message to the conversation
    pub fn add_user_message(&mut self, content: String) {
        self.input = content.clone();
        self.messages.push(ChatMessage {
            role: "user".to_string(),
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
        tool_calls: Option<Vec<crate::llm::types::ToolCall>>,
    ) {
        self.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content,
            tool_calls,
            tool_call_id: None,
            name: None,
        });
    }

    /// Add a tool result message to the conversation
    pub fn add_tool_result(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        result: String,
    ) {
        self.messages.push(ChatMessage {
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

        // Re-add system prompt
        self.messages.push(ChatMessage {
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
