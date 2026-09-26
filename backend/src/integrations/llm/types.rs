// ============================================================
// LLM types — OpenAI-compatible chat completion API types
// ============================================================

use serde::{Deserialize, Serialize};

// ── Request types ──

#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

/// A single chat message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

// ── Response types (non-streaming) ──

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ── Streaming types ──

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    pub id: Option<String>,
    #[serde(default)]
    pub choices: Vec<StreamChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Delta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaToolCall {
    pub index: u32,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type", default)]
    pub call_type: Option<String>,
    #[serde(default)]
    pub function: Option<DeltaToolCallFunction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaToolCallFunction {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

// ── Accumulated streaming state ──

/// Accumulates streaming deltas into a coherent assistant message
#[derive(Debug, Clone, Default)]
pub struct StreamAccumulator {
    pub content: String,
    pub tool_calls: Vec<AccumulatedToolCall>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Default)]
pub struct AccumulatedToolCall {
    pub index: u32,
    pub id: String,
    pub call_type: String,
    pub function_name: String,
    pub function_arguments: String,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a new delta chunk into the accumulator
    pub fn apply_delta(&mut self, delta: &Delta) {
        if let Some(content) = &delta.content {
            self.content.push_str(content);
        }

        if let Some(tool_calls) = &delta.tool_calls {
            for tc in tool_calls {
                // Find or create the accumulated tool call at this index
                while self.tool_calls.len() <= tc.index as usize {
                    self.tool_calls.push(AccumulatedToolCall::default());
                }
                let acc = &mut self.tool_calls[tc.index as usize];
                acc.index = tc.index;

                if let Some(id) = &tc.id {
                    acc.id = id.clone();
                }
                if let Some(ct) = &tc.call_type {
                    acc.call_type = ct.clone();
                }
                if let Some(func) = &tc.function {
                    if let Some(name) = &func.name {
                        acc.function_name = name.clone();
                    }
                    if let Some(args) = &func.arguments {
                        acc.function_arguments.push_str(args);
                    }
                }
            }
        }
    }

    /// Convert accumulated tool calls into the standard ToolCall format
    pub fn to_tool_calls(&self) -> Option<Vec<ToolCall>> {
        if self.tool_calls.is_empty() {
            return None;
        }
        Some(
            self.tool_calls
                .iter()
                .filter(|tc| !tc.id.is_empty())
                .map(|tc| ToolCall {
                    id: tc.id.clone(),
                    call_type: if tc.call_type.is_empty() {
                        "function".to_string()
                    } else {
                        tc.call_type.clone()
                    },
                    function: ToolCallFunction {
                        name: tc.function_name.clone(),
                        arguments: tc.function_arguments.clone(),
                    },
                })
                .collect(),
        )
    }

    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls.iter().any(|tc| !tc.id.is_empty())
    }
}
