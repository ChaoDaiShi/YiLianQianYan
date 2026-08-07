// ============================================================
// Tool trait definition — the contract all tools must implement
// ============================================================

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of executing a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ok: true,
            error: None,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            ok: false,
            error: Some(content.clone()),
            content,
        }
    }
}

/// Information about a tool (for display / listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Core trait that every tool must implement
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name (used as the function name in LLM tool calls)
    fn name(&self) -> &str;

    /// Human-readable description for the LLM
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters
    fn parameters(&self) -> serde_json::Value;

    /// Whether this tool requires user approval before execution
    fn requires_approval(&self) -> bool {
        false
    }

    /// Execute the tool with the given arguments.
    /// `args` is the parsed JSON value of the tool call's function.arguments.
    async fn execute(&self, args: serde_json::Value) -> ToolResult;

    /// Convert to an OpenAI-compatible tool definition
    fn to_openai_tool(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters(),
            }
        })
    }

    /// Convert to a ToolInfo for frontend display
    fn to_info(&self) -> ToolInfo {
        ToolInfo {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }
}
