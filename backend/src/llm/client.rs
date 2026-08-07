// ============================================================
// LLM Client — OpenAI-compatible chat completions with SSE streaming
// ============================================================

use reqwest::Client as HttpClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing;

use super::types::*;
use crate::config::types::ModelConfig;

/// Errors that can occur during LLM calls
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Stream error: {0}")]
    Stream(String),
    #[error("Timeout")]
    Timeout,
    #[error("No API key configured")]
    NoApiKey,
}

/// LLM client for OpenAI-compatible chat completion APIs
pub struct LlmClient {
    http: HttpClient,
    config: ModelConfig,
    /// Concurrency limiter (max 3 concurrent LLM calls)
    limiter: Arc<Mutex<()>>,
}

impl LlmClient {
    /// Create a new LLM client from configuration
    pub fn new(config: &ModelConfig) -> Self {
        let http = HttpClient::builder()
            .timeout(Duration::from_millis(config.invoke_timeout_ms))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http,
            config: config.clone(),
            limiter: Arc::new(Mutex::new(())),
        }
    }

    /// Build the full API endpoint URL
    fn api_url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }

    /// Get the resolved API key
    fn api_key(&self) -> Result<String, LlmError> {
        self.config.resolve_api_key().ok_or(LlmError::NoApiKey)
    }

    // ============================================================
    // Non-streaming invoke
    // ============================================================

    /// Send a non-streaming chat completion request
    pub async fn invoke(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
    ) -> Result<ChatCompletionResponse, LlmError> {
        let _guard = self.limiter.lock().await;

        let request = ChatCompletionRequest {
            model: self.config.name.clone(),
            messages: messages.to_vec(),
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
            tool_choice: None,
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
            stream: false,
        };

        let api_key = self.api_key()?;

        tracing::debug!(
            "LLM invoke: {} messages, {} tools",
            messages.len(),
            tools.len()
        );

        let response = self
            .http
            .post(&self.api_url())
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("HTTP {}: {}", status, body)));
        }

        let completion: ChatCompletionResponse = response.json().await?;
        Ok(completion)
    }

    // ============================================================
    // Streaming invoke
    // ============================================================

    /// Send a streaming chat completion request.
    /// Returns a receiver that yields StreamChunk items as they arrive.
    pub async fn invoke_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
    ) -> Result<tokio::sync::mpsc::Receiver<Result<StreamChunk, LlmError>>, LlmError> {
        let _guard = self.limiter.lock().await;

        let request = ChatCompletionRequest {
            model: self.config.name.clone(),
            messages: messages.to_vec(),
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
            tool_choice: None,
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
            stream: true,
        };

        let api_key = self.api_key()?;

        tracing::debug!(
            "LLM stream: {} messages, {} tools",
            messages.len(),
            tools.len()
        );

        let response = self
            .http
            .post(&self.api_url())
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("HTTP {}: {}", status, body)));
        }

        let (tx, rx) = tokio::sync::mpsc::channel(256);

        // Spawn a task to read the SSE stream
        tokio::spawn(async move {
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            loop {
                match tokio::time::timeout(
                    Duration::from_secs(60),
                    futures::TryStreamExt::try_next(&mut byte_stream),
                )
                .await
                {
                    Ok(Ok(Some(bytes))) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete SSE lines from buffer
                        while let Some(line_end) = buffer.find('\n') {
                            let line = buffer[..line_end].trim().to_string();
                            buffer = buffer[line_end + 1..].to_string();

                            if line.is_empty() {
                                continue;
                            }

                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    // Stream complete
                                    return;
                                }

                                match serde_json::from_str::<StreamChunk>(data) {
                                    Ok(chunk) => {
                                        if tx.send(Ok(chunk)).await.is_err() {
                                            // Receiver dropped
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("SSE parse error: {} in line: {}", e, data);
                                        // Continue — some providers have non-standard fields
                                    }
                                }
                            }
                        }
                    }
                    Ok(Ok(None)) => {
                        // EOF — stream ended
                        return;
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(Err(LlmError::Stream(e.to_string()))).await;
                        return;
                    }
                    Err(_) => {
                        let _ = tx.send(Err(LlmError::Timeout)).await;
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }

    // ============================================================
    // Convenience: streaming with full accumulator
    // ============================================================

    /// Stream a chat completion, accumulating content and tool calls.
    /// Calls `on_token` for each content delta and `on_tool_call` for completed tool calls.
    pub async fn stream_with_callbacks<F>(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        mut on_token: F,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        let mut rx = self.invoke_stream(messages, tools).await?;
        let mut accumulator = StreamAccumulator::new();

        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    for choice in &chunk.choices {
                        accumulator.apply_delta(&choice.delta);

                        // Emit content tokens as they arrive
                        if let Some(ref content) = choice.delta.content {
                            if !content.is_empty() {
                                on_token(content);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Stream error: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(accumulator)
    }
}
