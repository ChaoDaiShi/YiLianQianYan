// ============================================================
// JSON-RPC 2.0 codec — typed request/response/notification.
// ============================================================

use serde::{Deserialize, Serialize};

/// A monotonic integer id for JSON-RPC requests.
pub type JsonRpcId = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: JsonRpcId, method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        }
    }

    /// Serialize and check the request does not exceed the size bound.
    pub fn to_bounded_string(
        &self,
        max_bytes: usize,
    ) -> Result<String, crate::mcp_runtime::McpRuntimeError> {
        let text = serde_json::to_string(self)
            .map_err(|e| crate::mcp_runtime::McpRuntimeError::Protocol(e.to_string()))?;
        if text.len() > max_bytes {
            return Err(crate::mcp_runtime::McpRuntimeError::RequestTooLarge);
        }
        Ok(text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcSuccess {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorBody {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub error: JsonRpcErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// A single parsed JSON-RPC message (any of the three kinds).
#[derive(Debug, Clone)]
pub enum JsonRpcMessage {
    Success(JsonRpcSuccess),
    Error(JsonRpcError),
    Notification(JsonRpcNotification),
}

/// Parse an incoming line/value into a JSON-RPC message.
pub fn parse_message(value: &serde_json::Value) -> Result<JsonRpcMessage, String> {
    if value.get("id").is_some() {
        if value.get("error").is_some() {
            serde_json::from_value::<JsonRpcError>(value.clone())
                .map(JsonRpcMessage::Error)
                .map_err(|e| e.to_string())
        } else if value.get("result").is_some() {
            serde_json::from_value::<JsonRpcSuccess>(value.clone())
                .map(JsonRpcMessage::Success)
                .map_err(|e| e.to_string())
        } else {
            Err("JSON-RPC message has an id but neither result nor error".to_string())
        }
    } else if value.get("method").is_some() {
        serde_json::from_value::<JsonRpcNotification>(value.clone())
            .map(JsonRpcMessage::Notification)
            .map_err(|e| e.to_string())
    } else {
        Err("not a JSON-RPC message".to_string())
    }
}
