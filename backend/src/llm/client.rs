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
    #[error("embedding model is not configured")]
    EmbeddingNotConfigured,
    #[error("invalid embedding response: {0}")]
    InvalidEmbeddingResponse(String),
    #[error("embedding API key not configured")]
    EmbeddingNoApiKey,
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
    // Embedding
    // ============================================================

    /// Call the OpenAI-compatible `/embeddings` endpoint and return the
    /// first embedding vector.
    ///
    /// Uses `embedding_model` and `embedding_base_url` from the config —
    /// these are independent of the chat model and must be explicitly set.
    pub async fn embed(&self, input: &str) -> Result<Vec<f32>, LlmError> {
        if !self.config.has_embedding() {
            return Err(LlmError::EmbeddingNotConfigured);
        }

        let api_key = self
            .config
            .resolve_embedding_api_key()
            .ok_or(LlmError::EmbeddingNoApiKey)?;

        let url = format!(
            "{}/embeddings",
            self.config.embedding_base_url.trim_end_matches('/')
        );

        let request = serde_json::json!({
            "model": self.config.embedding_model,
            "input": input,
        });

        tracing::debug!(
            model = %self.config.embedding_model,
            input_len = input.chars().count(),
            "calling embedding API"
        );

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!(
                "embedding HTTP {}: {}",
                status, body
            )));
        }

        let raw: serde_json::Value = response.json().await?;

        let data = raw["data"]
            .as_array()
            .ok_or_else(|| {
                LlmError::InvalidEmbeddingResponse("missing or invalid 'data' array".to_string())
            })?
            .first()
            .ok_or_else(|| {
                LlmError::InvalidEmbeddingResponse("'data' array is empty".to_string())
            })?;

        let embedding = data["embedding"].as_array().ok_or_else(|| {
            LlmError::InvalidEmbeddingResponse("missing or invalid 'embedding' array".to_string())
        })?;

        if embedding.is_empty() {
            return Err(LlmError::InvalidEmbeddingResponse(
                "'embedding' array is empty".to_string(),
            ));
        }

        let vec: Vec<f32> = embedding
            .iter()
            .map(|v| {
                let f = v.as_f64().unwrap_or(f64::NAN) as f32;
                if f.is_finite() {
                    Ok(f)
                } else {
                    Err(LlmError::InvalidEmbeddingResponse(format!(
                        "non-finite embedding value: {v}"
                    )))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        tracing::debug!(dim = vec.len(), "embedding returned successfully");

        Ok(vec)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::ModelConfig;

    fn config_for_mock(base_url: &str) -> ModelConfig {
        ModelConfig {
            embedding_model: "test-embed-model".to_string(),
            embedding_base_url: base_url.to_string(),
            embedding_api_key: "test-key".to_string(),
            invoke_timeout_ms: 10_000,
            ..Default::default()
        }
    }

    fn unconfigured_config() -> ModelConfig {
        ModelConfig {
            embedding_model: String::new(),
            embedding_base_url: String::new(),
            ..Default::default()
        }
    }

    // ── Unit tests (no HTTP) ──

    #[test]
    fn embed_errors_when_not_configured() {
        let client = LlmClient::new(&unconfigured_config());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(client.embed("test input"));
        assert!(matches!(result, Err(LlmError::EmbeddingNotConfigured)));
    }

    // ── Integration tests (mock HTTP server) ──

    use axum::{extract::State, routing::post, Json, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct EmbedMockState {
        responses: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
        counter: Arc<AtomicUsize>,
    }

    async fn start_mock_embed_server(
        responses: Vec<serde_json::Value>,
    ) -> (String, EmbedMockState, tokio::task::JoinHandle<()>) {
        let state = EmbedMockState {
            responses: Arc::new(std::sync::Mutex::new(responses)),
            counter: Arc::new(AtomicUsize::new(0)),
        };
        let s = state.clone();
        let app = Router::new()
            .route("/embeddings", post(embed_mock_handler))
            .with_state(s);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (addr, state, handle)
    }

    async fn embed_mock_handler(
        State(state): State<EmbedMockState>,
        Json(body): Json<serde_json::Value>,
    ) -> (axum::http::StatusCode, Json<serde_json::Value>) {
        let idx = state.counter.fetch_add(1, Ordering::SeqCst);
        let body_model = body["model"].as_str().unwrap_or("");
        let body_input = body["input"].as_str().unwrap_or("");
        if body_model.is_empty() || body_input.is_empty() {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing model or input"})),
            );
        }
        let responses = state.responses.lock().unwrap();
        if idx >= responses.len() {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "no more responses"})),
            );
        }
        (axum::http::StatusCode::OK, Json(responses[idx].clone()))
    }

    #[tokio::test]
    async fn embed_returns_vector_on_success() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": [0.1, 0.2, 0.3]}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr));
        let result = client.embed("test input").await.unwrap();
        assert_eq!(result, vec![0.1, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn embed_errors_on_empty_data_array() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": []
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr));
        let result = client.embed("test").await;
        assert!(matches!(result, Err(LlmError::InvalidEmbeddingResponse(_))));
    }

    #[tokio::test]
    async fn embed_errors_on_empty_embedding_array() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": []}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr));
        let result = client.embed("test").await;
        assert!(matches!(result, Err(LlmError::InvalidEmbeddingResponse(_))));
    }

    #[tokio::test]
    async fn embed_errors_on_non_finite_values() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": [0.1, "NaN", null]}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr));
        let result = client.embed("test").await;
        assert!(matches!(result, Err(LlmError::InvalidEmbeddingResponse(_))));
    }

    #[tokio::test]
    async fn embed_handles_utf8_input() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": [0.5]}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr));
        let result = client.embed("用户偏好最小范围代码修改").await.unwrap();
        assert_eq!(result, vec![0.5]);
    }

    #[tokio::test]
    async fn embed_sends_correct_model_and_input() {
        use std::sync::Mutex;
        let last_body: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));

        #[derive(Clone)]
        struct CaptureState {
            last_body: Arc<Mutex<Option<serde_json::Value>>>,
        }

        let state = CaptureState {
            last_body: Arc::clone(&last_body),
        };

        let app = Router::new()
            .route("/embeddings", post(capture_handler))
            .with_state(state);

        async fn capture_handler(
            State(state): State<CaptureState>,
            Json(body): Json<serde_json::Value>,
        ) -> (axum::http::StatusCode, Json<serde_json::Value>) {
            *state.last_body.lock().unwrap() = Some(body);
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "data": [{"embedding": [0.1]}]
                })),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let _handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = LlmClient::new(&config_for_mock(&addr));
        client.embed("hello world").await.unwrap();

        let body = last_body.lock().unwrap().take().unwrap();
        assert_eq!(body["model"], "test-embed-model");
        assert_eq!(body["input"], "hello world");
    }
}
