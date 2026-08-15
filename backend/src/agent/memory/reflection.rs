// ============================================================
// Memory reflection — turn a completed task into memory candidates.
//
// Reflection is WHAT-to-remember, not a write. It never touches storage and
// never executes a tool; the write pipeline validates and persists separately.
// ============================================================

use async_trait::async_trait;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::candidate::{MemoryCandidate, MemoryCategory};
use crate::config::types::ModelConfig;
use crate::llm::client::LlmClient;
use crate::llm::types::ChatMessage;
use crate::task::model::TaskId;

/// Inputs to reflection. Contains no secrets.
#[derive(Debug, Clone)]
pub struct ReflectionInput {
    pub task_id: TaskId,
    pub task_title: String,
    pub task_description: String,
    pub agent_name: String,
    /// Bounded result summaries of the task's agent executions.
    pub agent_result_summaries: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ReflectorError {
    #[error("reflection produced no usable candidates: {0}")]
    InvalidOutput(String),
    #[error("reflection LLM call failed: {0}")]
    Llm(String),
}

#[async_trait]
pub trait MemoryReflector: Send + Sync {
    async fn reflect(
        &self,
        input: ReflectionInput,
        cancel: &CancellationToken,
    ) -> Result<Vec<MemoryCandidate>, ReflectorError>;
}

const REFLECTION_SYSTEM_PROMPT: &str = r#"你是记忆沉淀器。根据任务执行结果，提取值得长期保存的经验。

输出严格 JSON 数组，每项：
{
  "category": "fact" | "preference" | "knowledge" | "note",
  "content": "简洁、可复用的经验内容",
  "confidence": 0.0 到 1.0 之间的数字
}

约束：
- 只提取真正有长期价值的内容（用户偏好、通用经验、项目知识、失败教训）
- 不要保存 API key、token、密码、secret、base64、原始代码大段
- 内容用中文或原文语言，简洁
- 最多 10 条
- 只输出 JSON 数组，不要其他文字"#;

pub struct LlmMemoryReflector {
    llm: LlmClient,
}

impl LlmMemoryReflector {
    pub fn new(config: &ModelConfig) -> Self {
        Self {
            llm: LlmClient::new(config),
        }
    }
}

#[async_trait]
impl MemoryReflector for LlmMemoryReflector {
    async fn reflect(
        &self,
        input: ReflectionInput,
        cancel: &CancellationToken,
    ) -> Result<Vec<MemoryCandidate>, ReflectorError> {
        let user_prompt = serde_json::json!({
            "task_title": input.task_title,
            "task_description": input.task_description,
            "agent_name": input.agent_name,
            "results": input.agent_result_summaries,
        })
        .to_string();

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(REFLECTION_SYSTEM_PROMPT.to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(user_prompt),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];

        if cancel.is_cancelled() {
            return Err(ReflectorError::InvalidOutput("cancelled".to_string()));
        }

        let response = self
            .llm
            .invoke(&messages, &[])
            .await
            .map_err(|error| ReflectorError::Llm(error.to_string()))?;
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ReflectorError::InvalidOutput("empty LLM response".to_string()))?;
        if choice
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|c| !c.is_empty())
        {
            return Err(ReflectorError::InvalidOutput(
                "reflector returned unsupported tool calls".to_string(),
            ));
        }
        let content = choice.message.content.unwrap_or_default();
        let json_text = strip_code_fence(content.trim());
        let value: serde_json::Value = serde_json::from_str(json_text)
            .map_err(|error| ReflectorError::InvalidOutput(error.to_string()))?;

        parse_candidates(value, input.task_id.clone(), input.agent_name.clone())
            .map_err(|error| ReflectorError::InvalidOutput(error))
    }
}

fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        rest.trim()
            .strip_suffix("```")
            .unwrap_or(rest.trim())
            .trim()
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest.trim()
            .strip_suffix("```")
            .unwrap_or(rest.trim())
            .trim()
    } else {
        trimmed
    }
}

/// Parse a reflection JSON array into candidates, attaching task provenance.
fn parse_candidates(
    value: serde_json::Value,
    task_id: TaskId,
    agent_name: String,
) -> Result<Vec<MemoryCandidate>, String> {
    let array = value
        .as_array()
        .ok_or_else(|| "reflection output must be a JSON array".to_string())?;
    let mut candidates = Vec::with_capacity(array.len());
    for item in array {
        let category: MemoryCategory = item
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .parse()
            .map_err(|e: String| e)?;
        let content = item
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if content.is_empty() {
            continue;
        }
        let confidence = item
            .get("confidence")
            .and_then(|v| v.as_f64())
            .map(|c| c.clamp(0.0, 1.0) as f32)
            .unwrap_or(0.0);
        candidates.push(MemoryCandidate::new(
            category,
            content,
            Some(task_id.clone()),
            agent_name.clone(),
            confidence,
        ));
    }
    Ok(candidates)
}
