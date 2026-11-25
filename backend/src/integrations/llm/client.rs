// ============================================================
// LLM Client — OpenAI-compatible chat completions with SSE streaming
// ============================================================

use reqwest::Client as HttpClient;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing;

use super::types::*;
use crate::config::types::ModelConfig;
use crate::integrations::secret::SecretResolver;

mod response_format;

pub trait UsageRecorder: Send + Sync {
    fn record(&self, usage: &Usage);
    fn record_stream_usage(&self, _usage: &Usage) {}
    fn request_started(&self) {}
    fn begin_attempt(&self, _request: &str, _attempt: &str, _tier: usize) {}
    fn finish_attempt(&self, _failure: Option<&str>) {}
}

/// Errors that can occur during LLM calls
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Provider request failed (HTTP {status}, code {code:?})")]
    Provider { status: u16, code: Option<String> },
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
    #[error("secret store unavailable")]
    SecretUnavailable,
    #[error("embedding model is not configured")]
    EmbeddingNotConfigured,
    #[error("invalid embedding response: {0}")]
    InvalidEmbeddingResponse(String),
    #[error("embedding API key not configured")]
    EmbeddingNoApiKey,
    #[error("当前资料档案已关闭外部模型调用")]
    ExternalModelDisabled,
    #[error("模型服务重定向超出已配置的来源或次数上限，请检查模型 API 地址")]
    Redirect,
}

pub(crate) fn model_redirect_policy() -> reqwest::redirect::Policy {
    let limit = reqwest::redirect::Policy::limited(10);
    reqwest::redirect::Policy::custom(move |attempt| {
        let next = attempt.url();
        if next.username().is_empty()
            && next.password().is_none()
            && attempt
                .previous()
                .first()
                .is_some_and(|original| original.origin() == next.origin())
        {
            limit.redirect(attempt)
        } else {
            attempt.error(LlmError::Redirect)
        }
    })
}

impl LlmError {
    pub(crate) fn from_model_request(error: reqwest::Error) -> Self {
        if error.is_redirect() {
            Self::Redirect
        } else {
            Self::Http(error)
        }
    }
}

/// LLM client for OpenAI-compatible chat completion APIs
pub struct LlmClient {
    json_object_output: bool,
    thinking_mode: super::thinking::ThinkingMode,
    generation: super::generation::GenerationOverrides,
    pool: Option<super::model_pool::ModelPoolRuntime>,
    http: HttpClient,
    config: ModelConfig,
    resolver: Arc<SecretResolver>,
    profile_permissions: Option<crate::db::Database>,
    /// Concurrency limiter (max 3 concurrent LLM calls)
    limiter: Arc<Mutex<()>>,
    usage_recorder: Option<Arc<dyn UsageRecorder>>,
}

impl LlmClient {
    /// Create a new LLM client from configuration + a secret resolver.
    pub fn new(config: &ModelConfig, resolver: Arc<SecretResolver>) -> Self {
        Self::new_with_usage_recorder(config, resolver, None)
    }

    pub fn new_with_usage_recorder(
        config: &ModelConfig,
        resolver: Arc<SecretResolver>,
        usage_recorder: Option<Arc<dyn UsageRecorder>>,
    ) -> Self {
        let mut http = HttpClient::builder()
            .timeout(Duration::from_millis(config.invoke_timeout_ms))
            .redirect(model_redirect_policy());
        if config.uses_local_chat() {
            http = http.no_proxy();
        }
        let http = http.build().expect("Failed to create HTTP client");

        Self {
            json_object_output: false,
            thinking_mode: super::thinking::ThinkingMode::Standard,
            generation: super::generation::GenerationOverrides::default(),
            pool: None,
            http,
            config: config.clone(),
            resolver,
            profile_permissions: None,
            limiter: Arc::new(Mutex::new(())),
            usage_recorder,
        }
    }

    pub(crate) fn with_profile_permissions(mut self, db: crate::db::Database) -> Self {
        self.profile_permissions = Some(db);
        self
    }

    fn check_external_model_permission(&self, local: bool) -> Result<(), LlmError> {
        if !local
            && self
                .profile_permissions
                .as_ref()
                .is_some_and(|db| !crate::modules::onboarding::allowed(db, "external_model"))
        {
            return Err(LlmError::ExternalModelDisabled);
        }
        Ok(())
    }

    pub fn with_model_pool(mut self, mut pool: super::model_pool::ModelPoolRuntime) -> Self {
        if self.json_object_output {
            for candidate in &mut pool.candidates {
                candidate.client.set_json_object_output();
            }
        }
        pool.set_generation_overrides(self.generation);
        self.pool = Some(pool.with_thinking_mode(self.thinking_mode));
        self
    }

    /// A caller that accounts for one provider request may select a pool
    /// without spending additional unmetered requests on fallback tiers.
    pub(crate) fn with_one_pool_attempt(mut self) -> Self {
        if let Some(pool) = self.pool.as_mut() {
            pool.candidates.truncate(1);
        }
        self
    }

    pub fn with_thinking_mode(mut self, mode: super::thinking::ThinkingMode) -> Self {
        self.set_thinking_mode(mode);
        self
    }

    pub(super) fn set_thinking_mode(&mut self, mode: super::thinking::ThinkingMode) {
        self.thinking_mode = mode;
        if let Some(pool) = &mut self.pool {
            pool.set_thinking_mode(mode);
        }
    }

    pub fn with_generation_overrides(
        mut self,
        generation: super::generation::GenerationOverrides,
    ) -> Self {
        self.set_generation_overrides(generation);
        self
    }

    pub(super) fn set_generation_overrides(
        &mut self,
        generation: super::generation::GenerationOverrides,
    ) {
        self.generation = generation;
        if let Some(pool) = &mut self.pool {
            pool.set_generation_overrides(generation);
        }
    }

    fn generation_options(&self) -> super::generation::GenerationOptions {
        super::generation::GenerationOptions::for_model(&self.config, self.thinking_mode)
    }

    fn request_body(&self, request: ChatCompletionRequest) -> Result<serde_json::Value, LlmError> {
        let mut value =
            serde_json::to_value(request).map_err(|e| LlmError::Parse(e.to_string()))?;
        self.thinking_mode
            .apply(&self.config, &mut value)
            .map_err(LlmError::Api)?;
        self.generation
            .apply_sampling(self.generation_options(), &mut value);
        self.apply_response_format(&mut value);
        Ok(value)
    }

    pub(super) fn begin_attempt(&self, request: &str, attempt: &str, tier: usize) {
        if let Some(recorder) = &self.usage_recorder {
            recorder.begin_attempt(request, attempt, tier)
        }
    }
    pub(super) fn finish_attempt(&self, failure: Option<&str>) {
        if let Some(recorder) = &self.usage_recorder {
            recorder.finish_attempt(failure)
        }
    }
    fn record_usage(&self, usage: &Usage) {
        if let Some(recorder) = &self.usage_recorder {
            recorder.record(usage);
        }
    }

    /// Return the maximum completion tokens that fit inside a caller's total
    /// token budget. The UTF-8 byte count is a conservative estimate for the
    /// visible prompt; provider-specific hidden framing/tokenizer costs remain
    /// uncertain and are still checked against the provider's actual usage.
    pub fn output_tokens_for_total_budget(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        total_token_budget: u64,
    ) -> Option<u32> {
        let input_upper_bound = conservative_input_token_upper_bound(messages, tools)?;
        let remaining = total_token_budget.checked_sub(input_upper_bound)?;
        let output_limit = remaining.min(u64::from(self.max_output_tokens()));
        (output_limit > 0).then_some(output_limit.min(u64::from(u32::MAX)) as u32)
    }

    pub fn max_output_tokens(&self) -> u32 {
        if let Some(pool) = &self.pool {
            let configured_limit = self.config.max_tokens;
            let pool_limit = pool
                .candidates
                .iter()
                .map(|candidate| candidate.client.max_output_tokens())
                .max();
            return pool_limit.map_or(configured_limit, |limit| configured_limit.min(limit));
        }
        self.generation
            .output_limit(self.generation_options(), self.config.max_tokens)
    }

    pub(crate) fn model_identity(&self) -> (&str, &str) {
        (&self.config.provider, &self.config.name)
    }

    pub fn invoke_timeout(&self) -> Duration {
        Duration::from_millis(self.config.invoke_timeout_ms)
    }

    /// Build the full API endpoint URL
    fn api_url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }

    /// Resolve the chat API key through the SecretResolver (never a String).
    async fn api_key(&self) -> Result<SecretString, LlmError> {
        let resolved = self
            .resolver
            .resolve_api_key(&self.config)
            .await
            .map_err(|_| LlmError::SecretUnavailable)?;
        match resolved {
            Some(key) => Ok(key),
            None if matches!(
                self.config.provider.as_str(),
                "ollama" | "lmstudio" | "llamacpp"
            ) =>
            {
                Ok(SecretString::from(String::new()))
            }
            None => Err(LlmError::NoApiKey),
        }
    }

    /// Resolve the embedding API key through the SecretResolver.
    async fn embedding_api_key(&self) -> Result<Option<SecretString>, LlmError> {
        self.resolver
            .resolve_embedding_api_key(&self.config)
            .await
            .map_err(|_| LlmError::SecretUnavailable)
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
        self.invoke_with_max_tokens(messages, tools, self.max_output_tokens())
            .await
    }

    /// Invoke using an explicit output cap while retaining this client's
    /// configured limiter, resolver, timeout, and usage recorder.
    pub async fn invoke_with_max_tokens(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
    ) -> Result<ChatCompletionResponse, LlmError> {
        if let Some(pool) = &self.pool {
            return pool.invoke(messages, tools, max_tokens).await;
        }
        self.invoke_single_with_max_tokens(messages, tools, max_tokens)
            .await
    }

    pub(super) async fn invoke_single_with_max_tokens(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
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
            max_tokens: Some(
                self.generation
                    .output_limit(self.generation_options(), max_tokens),
            ),
            stream: false,
            stream_options: None,
        };

        let request = self.request_body(request)?;
        let api_key = self.api_key().await?;
        self.check_external_model_permission(self.config.uses_local_chat())?;
        if let Some(recorder) = &self.usage_recorder {
            recorder.request_started();
        }

        tracing::debug!(
            "LLM invoke: {} messages, {} tools",
            messages.len(),
            tools.len()
        );

        let response = self
            .http
            .post(&self.api_url())
            .header(
                "Authorization",
                format!("Bearer {}", api_key.expose_secret()),
            )
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(LlmError::from_model_request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(provider_error(
                &self.config.provider,
                status.as_u16(),
                &body,
            ));
        }

        let value: serde_json::Value = response.json().await?;
        if let Some(error) = response_error(&self.config.provider, 200, &value) {
            return Err(error);
        }
        let completion: ChatCompletionResponse =
            serde_json::from_value(value).map_err(|error| LlmError::Parse(error.to_string()))?;
        if let Some(usage) = completion.usage.as_ref() {
            self.record_usage(usage);
        }
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

        let loopback = self.config.has_local_embedding();
        let client = if loopback {
            // Bypass proxy settings while permitting redirects within the same local origin.
            static LOCAL_HTTP: std::sync::LazyLock<reqwest::Client> =
                std::sync::LazyLock::new(|| {
                    reqwest::Client::builder()
                        .redirect(model_redirect_policy())
                        .no_proxy()
                        .build()
                        .expect("loopback embedding client configuration")
                });
            &*LOCAL_HTTP
        } else {
            &self.http
        };
        let mut request_builder = client.post(&url).timeout(self.invoke_timeout());
        let bundled_local_runtime = loopback
            && matches!(
                self.config.embedding_model.as_str(),
                "BAAI/bge-small-zh-v1.5" | "Qwen/Qwen3-Embedding-0.6B"
            );
        let key = if bundled_local_runtime {
            None
        } else {
            self.embedding_api_key().await?
        };
        match key {
            Some(key) => request_builder = request_builder.bearer_auth(key.expose_secret()),
            None if !loopback => return Err(LlmError::EmbeddingNoApiKey),
            None => {}
        }
        self.check_external_model_permission(loopback)?;
        let response = request_builder
            .json(&request)
            .send()
            .await
            .map_err(LlmError::from_model_request)?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(LlmError::Api(format!("embedding HTTP {status}")));
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

    pub async fn embed_query(&self, query: &str) -> Result<Vec<f32>, LlmError> {
        match self.config.embedding_model.as_str() {
            "BAAI/bge-small-zh-v1.5" => self.embed(&format!("为这个句子生成表示以用于检索相关文章：{query}")).await,
            "Qwen/Qwen3-Embedding-0.6B" => self.embed(&format!("Instruct: Retrieve relevant personal memories for the given query\nQuery: {query}")).await,
            _ => self.embed(query).await,
        }
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
        self.invoke_stream_with_max_tokens(messages, tools, self.max_output_tokens())
            .await
    }

    pub async fn invoke_stream_with_max_tokens(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
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
            max_tokens: Some(
                self.generation
                    .output_limit(self.generation_options(), max_tokens),
            ),
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        };

        let request = self.request_body(request)?;
        let api_key = self.api_key().await?;
        self.check_external_model_permission(self.config.uses_local_chat())?;
        if let Some(recorder) = &self.usage_recorder {
            recorder.request_started();
        }

        tracing::debug!(
            "LLM stream: {} messages, {} tools",
            messages.len(),
            tools.len()
        );

        let response = self
            .http
            .post(&self.api_url())
            .header(
                "Authorization",
                format!("Bearer {}", api_key.expose_secret()),
            )
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(LlmError::from_model_request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(provider_error(
                &self.config.provider,
                status.as_u16(),
                &body,
            ));
        }

        let (tx, rx) = tokio::sync::mpsc::channel(256);

        // Spawn a task to read the SSE stream
        let provider = self.config.provider.clone();
        tokio::spawn(async move {
            let mut byte_stream = response.bytes_stream();
            let mut buffer = Vec::new();
            let mut finished = false;

            loop {
                let next = tokio::select! {
                    _=tx.closed()=>return,
                    result=tokio::time::timeout(Duration::from_secs(60),futures::TryStreamExt::try_next(&mut byte_stream))=>result,
                };
                match next {
                    Ok(Ok(Some(bytes))) => {
                        buffer.extend_from_slice(&bytes);
                        let Some(complete) = buffer.iter().rposition(|&byte| byte == b'\n') else {
                            continue;
                        };
                        // Decode only complete lines: HTTP byte chunks can end
                        // partway through a CJK character or emoji.
                        for raw in buffer[..complete].split(|&byte| byte == b'\n') {
                            let line = String::from_utf8_lossy(raw);
                            if let Some(data) =
                                line.trim().strip_prefix("data:").map(str::trim_start)
                            {
                                if data == "[DONE]" {
                                    // Stream complete
                                    return;
                                }

                                if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                                    if let Some(error) = response_error(&provider, 200, &value) {
                                        let _ = tx.send(Err(error)).await;
                                        return;
                                    }
                                }
                                match serde_json::from_str::<StreamChunk>(data) {
                                    Ok(chunk) => {
                                        finished |=
                                            chunk.choices.iter().any(|c| c.finish_reason.is_some());
                                        if tx.send(Ok(chunk)).await.is_err() {
                                            // Receiver dropped
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("SSE parse error: {}", e);
                                        // Continue — some providers have non-standard fields
                                    }
                                }
                            }
                        }
                        buffer.drain(..=complete);
                    }
                    Ok(Ok(None)) => {
                        if !finished {
                            let _ = tx
                                .send(Err(LlmError::Stream(
                                    "stream closed before completion".into(),
                                )))
                                .await;
                        }
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
        on_token: F,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        self.stream_with_callbacks_max_tokens(messages, tools, self.config.max_tokens, on_token)
            .await
    }

    pub async fn stream_with_callbacks_max_tokens<F>(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
        mut on_token: F,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        self.stream_with_reasoning_max_tokens(
            messages,
            tools,
            max_tokens,
            |token| {
                on_token(token);
                std::future::ready(())
            },
            |_| {},
        )
        .await
    }
    pub async fn stream_with_reasoning_max_tokens<F, R, TokenFuture>(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
        on_token: F,
        on_reasoning: R,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) -> TokenFuture + Send,
        R: FnMut(&str) + Send,
        TokenFuture: std::future::Future<Output = ()> + Send,
    {
        if let Some(pool) = &self.pool {
            return pool
                .stream_with_reasoning(messages, tools, max_tokens, on_token, on_reasoning)
                .await;
        }
        self.stream_single_with_reasoning_max_tokens(
            messages,
            tools,
            max_tokens,
            on_token,
            on_reasoning,
        )
        .await
    }
    pub(super) async fn stream_single_with_callbacks_max_tokens<F>(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
        mut on_token: F,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        self.stream_single_with_reasoning_max_tokens(
            messages,
            tools,
            max_tokens,
            |token| {
                on_token(token);
                std::future::ready(())
            },
            |_| {},
        )
        .await
    }
    pub(super) async fn stream_single_with_reasoning_max_tokens<F, R, TokenFuture>(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        max_tokens: u32,
        mut on_token: F,
        mut on_reasoning: R,
    ) -> Result<StreamAccumulator, LlmError>
    where
        F: FnMut(&str) -> TokenFuture + Send,
        R: FnMut(&str) + Send,
        TokenFuture: std::future::Future<Output = ()> + Send,
    {
        let mut rx = self
            .invoke_stream_with_max_tokens(messages, tools, max_tokens)
            .await?;
        let mut accumulator = StreamAccumulator::new();

        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Some(usage) = &chunk.usage {
                        if let Some(recorder) = &self.usage_recorder {
                            recorder.record_stream_usage(usage);
                        }
                    }
                    if chunk.usage.is_some() {
                        accumulator.usage = chunk.usage.clone();
                    }
                    for choice in &chunk.choices {
                        accumulator.apply_delta(&choice.delta);
                        if choice.finish_reason.is_some() {
                            accumulator.finish_reason.clone_from(&choice.finish_reason);
                        }
                        if let Some(reasoning) = choice
                            .delta
                            .reasoning_content
                            .as_deref()
                            .filter(|value| !value.is_empty())
                        {
                            on_reasoning(reasoning);
                        }

                        // Emit content tokens as they arrive
                        if let Some(ref content) = choice.delta.content {
                            if !content.is_empty() {
                                on_token(content).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Some(usage) = accumulator.usage.as_ref() {
                        self.record_usage(usage);
                    }
                    tracing::error!("Stream error: {}", e);
                    return Err(e);
                }
            }
        }

        if let Some(usage) = accumulator.usage.as_ref() {
            self.record_usage(usage);
        }
        Ok(accumulator)
    }
}

/// Upper-bounds visible input by its UTF-8 bytes plus a small per-message/tool
/// framing allowance. This intentionally avoids a tokenizer dependency; real
/// provider usage remains authoritative after the request.
pub fn conservative_input_token_upper_bound(
    messages: &[ChatMessage],
    tools: &[serde_json::Value],
) -> Option<u64> {
    let message_bytes = serde_json::to_vec(messages).ok()?.len() as u64;
    let tools_bytes = serde_json::to_vec(tools).ok()?.len() as u64;
    message_bytes
        .checked_add(tools_bytes)?
        .checked_add(((messages.len() + tools.len()) as u64).checked_mul(64)?)?
        .checked_add(32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::ModelConfig;
    use crate::integrations::secret::SecretStore;

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

    fn test_resolver() -> Arc<crate::integrations::secret::SecretResolver> {
        Arc::new(crate::integrations::secret::SecretResolver::new(Arc::new(
            crate::integrations::secret::InMemorySecretStore::new(),
        )))
    }

    // ── Unit tests (no HTTP) ──

    #[test]
    fn embed_errors_when_not_configured() {
        let client = LlmClient::new(&unconfigured_config(), test_resolver());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(client.embed("test input"));
        assert!(matches!(result, Err(LlmError::EmbeddingNotConfigured)));
    }

    #[test]
    fn total_token_budget_reserves_utf8_prompt_before_output() {
        let client = LlmClient::new(&config_for_mock("http://127.0.0.1:1"), test_resolver());
        let messages = [ChatMessage::user("你好")];
        let input_upper_bound = conservative_input_token_upper_bound(&messages, &[]).unwrap();

        assert!(input_upper_bound >= "你好".len() as u64);
        assert_eq!(
            client.output_tokens_for_total_budget(&messages, &[], input_upper_bound + 100,),
            Some(100),
        );
        assert_eq!(
            client.output_tokens_for_total_budget(&messages, &[], input_upper_bound),
            None,
        );
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
    async fn embed_accepts_loopback_service_without_credentials() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": [0.4, 0.6]}]
        })])
        .await;
        let mut config = config_for_mock(&addr);
        config.embedding_api_key.clear();
        assert_eq!(
            LlmClient::new(&config, test_resolver())
                .embed("本地记忆")
                .await
                .unwrap(),
            vec![0.4, 0.6]
        );
    }

    #[tokio::test]
    async fn remote_invoke_rechecks_permission_after_client_creation() {
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).unwrap();
        let config = ModelConfig {
            base_url: "https://provider.example.invalid/v1".into(),
            api_key: "revocation-test-key".into(),
            ..Default::default()
        };
        let client = LlmClient::new(&config, test_resolver())
            .with_profile_permissions(db.clone_connection());
        assert!(crate::modules::onboarding::allowed(&db, "external_model"));
        db.conn().execute(
            "UPDATE local_onboarding SET profile=json_set(profile, '$.permissions.external_model', json('false'))",
            [],
        ).unwrap();
        assert!(matches!(
            client
                .invoke(&[ChatMessage::user("synthetic request")], &[])
                .await,
            Err(LlmError::ExternalModelDisabled)
        ));
    }

    #[tokio::test]
    async fn embed_returns_vector_on_success() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": [0.1, 0.2, 0.3]}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr), test_resolver());
        let result = client.embed("test input").await.unwrap();
        assert_eq!(result, vec![0.1, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn embed_errors_on_empty_data_array() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": []
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr), test_resolver());
        let result = client.embed("test").await;
        assert!(matches!(result, Err(LlmError::InvalidEmbeddingResponse(_))));
    }

    #[tokio::test]
    async fn embed_errors_on_empty_embedding_array() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": []}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr), test_resolver());
        let result = client.embed("test").await;
        assert!(matches!(result, Err(LlmError::InvalidEmbeddingResponse(_))));
    }

    #[tokio::test]
    async fn embed_errors_on_non_finite_values() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": [0.1, "NaN", null]}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr), test_resolver());
        let result = client.embed("test").await;
        assert!(matches!(result, Err(LlmError::InvalidEmbeddingResponse(_))));
    }

    #[tokio::test]
    async fn embed_handles_utf8_input() {
        let (addr, _state, _handle) = start_mock_embed_server(vec![serde_json::json!({
            "data": [{"embedding": [0.5]}]
        })])
        .await;

        let client = LlmClient::new(&config_for_mock(&addr), test_resolver());
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

        let client = LlmClient::new(&config_for_mock(&addr), test_resolver());
        client.embed("hello world").await.unwrap();

        let body = last_body.lock().unwrap().take().unwrap();
        assert_eq!(body["model"], "test-embed-model");
        assert_eq!(body["input"], "hello world");
    }

    #[tokio::test]
    async fn embed_resolves_secret_ref_from_store() {
        use std::sync::Mutex as StdMutex;
        let store = Arc::new(crate::integrations::secret::InMemorySecretStore::new());
        store
            .put(
                &crate::integrations::secret::SecretRef::new(
                    crate::integrations::secret::EMBEDDING_KEY_REF,
                ),
                secrecy::SecretString::from("SECRET_EMBED".to_string()),
            )
            .await
            .unwrap();
        let resolver = Arc::new(crate::integrations::secret::SecretResolver::new(store));

        let captured: Arc<StdMutex<Option<String>>> = Arc::new(StdMutex::new(None));
        #[derive(Clone)]
        struct CapState {
            auth: Arc<StdMutex<Option<String>>>,
        }
        let state = CapState {
            auth: Arc::clone(&captured),
        };
        let app = Router::new()
            .route("/embeddings", post(capture_embed_auth))
            .with_state(state);

        async fn capture_embed_auth(
            State(state): State<CapState>,
            headers: axum::http::HeaderMap,
            Json(_body): Json<serde_json::Value>,
        ) -> (axum::http::StatusCode, Json<serde_json::Value>) {
            *state.auth.lock().unwrap() = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({"data": [{"embedding": [0.1]}]})),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let mut config = ModelConfig::default();
        config.embedding_model = "test-embed".to_string();
        config.embedding_base_url = addr;
        config.embedding_api_key_ref = Some(crate::integrations::secret::SecretRef::new(
            crate::integrations::secret::EMBEDDING_KEY_REF,
        ));
        let client = LlmClient::new(&config, resolver);
        client.embed("hello").await.unwrap();

        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some("Bearer SECRET_EMBED")
        );
    }

    #[tokio::test]
    async fn invoke_resolves_secret_ref_from_store() {
        use std::sync::Mutex as StdMutex;
        let store = Arc::new(crate::integrations::secret::InMemorySecretStore::new());
        store
            .put(
                &crate::integrations::secret::SecretRef::new(
                    crate::integrations::secret::CHAT_KEY_REF,
                ),
                secrecy::SecretString::from("SECRET_CHAT".to_string()),
            )
            .await
            .unwrap();
        let resolver = Arc::new(crate::integrations::secret::SecretResolver::new(store));

        let captured: Arc<StdMutex<Option<String>>> = Arc::new(StdMutex::new(None));
        #[derive(Clone)]
        struct CapState {
            auth: Arc<StdMutex<Option<String>>>,
        }
        let state = CapState {
            auth: Arc::clone(&captured),
        };
        let app = Router::new()
            .route("/chat/completions", post(capture_chat_auth))
            .with_state(state);

        async fn capture_chat_auth(
            State(state): State<CapState>,
            headers: axum::http::HeaderMap,
            Json(_body): Json<serde_json::Value>,
        ) -> (axum::http::StatusCode, Json<serde_json::Value>) {
            *state.auth.lock().unwrap() = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "id": "chatcmpl-1",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": "ok" },
                        "finish_reason": "stop"
                    }]
                })),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let mut config = ModelConfig::default();
        config.base_url = addr;
        config.api_key_ref = Some(crate::integrations::secret::SecretRef::new(
            crate::integrations::secret::CHAT_KEY_REF,
        ));
        let client = LlmClient::new(&config, resolver);
        let msg = ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some("hi".to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        client.invoke(&[msg], &[]).await.unwrap();

        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some("Bearer SECRET_CHAT")
        );
    }
}

fn provider_error(provider: &str, status: u16, body: &str) -> LlmError {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| response_error(provider, status, &v))
        .unwrap_or(LlmError::Provider { status, code: None })
}
/// Registered official MiniMax business codes and explicit OpenAI/DashScope error objects.
pub(crate) fn response_error(
    provider: &str,
    status: u16,
    value: &serde_json::Value,
) -> Option<LlmError> {
    if provider == "minimax" {
        if let Some(code) = value
            .pointer("/base_resp/status_code")
            .and_then(serde_json::Value::as_i64)
            .filter(|c| *c != 0)
        {
            return Some(match code {
                1001 => LlmError::Timeout,
                1002 | 1039 | 1041 | 2045 => LlmError::Provider {
                    status: 429,
                    code: Some("rate_limit_exceeded".into()),
                },
                1004 | 2049 => LlmError::Provider {
                    status: 401,
                    code: Some("invalid_api_key".into()),
                },
                1008 | 2056 => LlmError::Provider {
                    status: 402,
                    code: Some("insufficient_quota".into()),
                },
                1000 | 1024 | 1033 => LlmError::Provider {
                    status: 500,
                    code: Some("service_error".into()),
                },
                _ => LlmError::Provider {
                    status,
                    code: Some(format!("minimax_{code}")),
                },
            });
        }
    }
    let error = value.get("error");
    let code = error
        .and_then(|e| {
            e.get("code")
                .filter(|v| v.is_string())
                .or_else(|| e.get("type"))
        })
        .or_else(|| value.get("code"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    if error.is_some() || code.is_some() {
        Some(LlmError::Provider { status, code })
    } else {
        None
    }
}
