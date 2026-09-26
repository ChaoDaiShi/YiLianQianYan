//! Real, provider-neutral OpenAI-compatible speech providers.
//!
//! The implementation deliberately has no deterministic production fallback.
//! A provider with incomplete configuration or an unavailable credential
//! fails closed with a value-free error.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::HeaderValue;
use reqwest::multipart::{Form, Part};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use crate::config::types::{VoiceConfig, VoiceSttConfig, VoiceTtsConfig};
use crate::secret::{SecretRef, SecretResolver, SecretSource};

pub const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_SPEECH_TEXT_CHARS: usize = 4_096;
pub const DEFAULT_VOICE_TIMEOUT_MS: u64 = 60_000;
pub const MINIMAX_STT_MODEL: &str = "asr-1.0";
pub const MINIMAX_STT_MAX_DURATION_SECONDS: f64 = 500.0;
const MINIMAX_STT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MINIMAX_STT_MAX_RETRIES: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioInput {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub filename: String,
    pub language: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinalTranscript {
    pub text: String,
    pub provider: String,
    pub model: String,
    #[serde(default = "default_final_transcript")]
    pub is_final: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechRequest {
    pub text: String,
    pub voice: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl SpeechRequest {
    pub fn new(text: impl Into<String>, voice: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice: voice.into(),
            language: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechAudio {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VoiceProviderError {
    #[error("voice provider is unavailable")]
    ProviderUnavailable,
    #[error("invalid audio input: {0}")]
    InvalidAudio(String),
    #[error("invalid speech text")]
    InvalidText,
    #[error("voice provider request failed with status {0}")]
    RequestFailed(u16),
    #[error("voice provider request timed out")]
    Timeout,
    #[error("voice provider returned an invalid response")]
    InvalidResponse,
    #[error("voice provider returned an unsupported media type")]
    UnsupportedMediaType,
    #[error("voice provider rejected the request with code {0}")]
    ProviderRejected(i64),
    #[error("speech-to-text authentication failed")]
    AuthFailed,
    #[error("speech-to-text request was rate limited")]
    RateLimited,
    #[error("speech-to-text audio is too large")]
    AudioTooLarge,
    #[error("speech-to-text audio format is unsupported")]
    UnsupportedAudio,
    #[error("speech-to-text transcription failed")]
    TranscriptionFailed,
    #[error("speech-to-text request timed out")]
    SttTimeout,
}

impl VoiceProviderError {
    /// Stable, value-free codes for callers that need to normalize provider
    /// failures without exposing a vendor response body.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
            Self::AuthFailed => "AUTH_FAILED",
            Self::RateLimited => "RATE_LIMITED",
            Self::AudioTooLarge => "AUDIO_TOO_LARGE",
            Self::UnsupportedAudio => "UNSUPPORTED_AUDIO",
            Self::TranscriptionFailed => "TRANSCRIPTION_FAILED",
            Self::SttTimeout | Self::Timeout => "STT_TIMEOUT",
            Self::InvalidAudio(_) => "INVALID_AUDIO",
            Self::InvalidText => "INVALID_TEXT",
            Self::RequestFailed(_) | Self::ProviderRejected(_) => "PROVIDER_REQUEST_FAILED",
            Self::InvalidResponse => "INVALID_RESPONSE",
            Self::UnsupportedMediaType => "UNSUPPORTED_MEDIA_TYPE",
        }
    }
}

/// Safe provider configuration. It contains only a SecretRef and source
/// metadata; the credential value is resolved just before a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceProviderConfig {
    pub provider: String,
    pub base_url: String,
    pub stt_model: String,
    pub tts_model: String,
    pub voice: String,
    pub language: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_ref: Option<SecretRef>,
    #[serde(default)]
    pub api_key_source: SecretSource,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

impl Default for VoiceProviderConfig {
    fn default() -> Self {
        Self {
            provider: "openai-compatible".to_string(),
            base_url: String::new(),
            stt_model: String::new(),
            tts_model: String::new(),
            voice: String::new(),
            language: String::new(),
            api_key_env: String::new(),
            api_key_ref: None,
            api_key_source: SecretSource::None,
            timeout_ms: DEFAULT_VOICE_TIMEOUT_MS,
            available: false,
            unavailable_reason: Some("voice provider configuration is incomplete".to_string()),
        }
    }
}

impl VoiceProviderConfig {
    pub fn from_voice_config(config: &VoiceConfig) -> Self {
        let api_key_source = if config.stt.api_key_ref.is_some() {
            SecretSource::SecretStore
        } else if !config.stt.api_key_env.trim().is_empty() {
            SecretSource::Environment
        } else if !config.stt.api_key.is_empty() {
            SecretSource::LegacyPending
        } else {
            SecretSource::None
        };
        Self {
            provider: config.stt.provider.clone(),
            base_url: config.stt.base_url.clone(),
            stt_model: config.stt.model.clone(),
            tts_model: config.tts.model.clone(),
            voice: config.tts.voice.clone(),
            language: config.stt.language.clone(),
            api_key_env: config.stt.api_key_env.clone(),
            api_key_ref: config.stt.api_key_ref.clone(),
            api_key_source,
            timeout_ms: config.stt.timeout_ms,
            available: config.stt.structurally_configured() && config.tts.structurally_configured(),
            unavailable_reason: (!(config.stt.structurally_configured()
                && config.tts.structurally_configured()))
            .then(|| "voice provider configuration is incomplete".to_string()),
        }
    }

    pub fn structurally_configured(&self, stt: bool) -> bool {
        !self.provider.trim().is_empty()
            && !self.base_url.trim().is_empty()
            && if stt {
                !self.stt_model.trim().is_empty()
            } else {
                !self.tts_model.trim().is_empty() && !self.voice.trim().is_empty()
            }
            && self.timeout_ms > 0
    }
}

fn default_timeout() -> u64 {
    DEFAULT_VOICE_TIMEOUT_MS
}

fn default_final_transcript() -> bool {
    true
}

#[async_trait]
pub trait SpeechToTextProvider: Send + Sync {
    async fn transcribe(&self, input: AudioInput) -> Result<FinalTranscript, VoiceProviderError>;

    fn provider_name(&self) -> &str;
}

#[async_trait]
pub trait TextToSpeechProvider: Send + Sync {
    async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechAudio, VoiceProviderError>;

    fn provider_name(&self) -> &str;
}

/// OpenAI Audio API-compatible implementation. The base URL is configurable,
/// so compatible gateways can be used without hard-coding a provider.
#[derive(Clone)]
pub struct OpenAiCompatibleVoiceProvider {
    config: VoiceProviderConfig,
    resolver: Arc<SecretResolver>,
    client: reqwest::Client,
}

pub type OpenAICompatibleVoiceProvider = OpenAiCompatibleVoiceProvider;

impl OpenAiCompatibleVoiceProvider {
    pub fn new(config: VoiceProviderConfig, resolver: Arc<SecretResolver>) -> Self {
        let timeout = Duration::from_millis(config.timeout_ms.max(1));
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config,
            resolver,
            client,
        }
    }

    pub fn from_voice_config(config: &VoiceConfig, resolver: Arc<SecretResolver>) -> Self {
        Self::new(VoiceProviderConfig::from_voice_config(config), resolver)
    }

    pub fn config(&self) -> VoiceProviderConfig {
        self.config.clone()
    }

    pub fn is_structurally_available(&self) -> bool {
        self.config.available
            && self.config.structurally_configured(true)
            && self.config.structurally_configured(false)
    }

    pub async fn is_available(&self) -> bool {
        self.is_structurally_available() && self.resolve_api_key().await.is_ok()
    }

    pub async fn transcribe(
        &self,
        input: AudioInput,
    ) -> Result<FinalTranscript, VoiceProviderError> {
        self.transcribe_inner(input).await
    }

    pub async fn synthesize(
        &self,
        request: &SpeechRequest,
    ) -> Result<SpeechAudio, VoiceProviderError> {
        self.synthesize_inner(request).await
    }

    async fn transcribe_inner(
        &self,
        input: AudioInput,
    ) -> Result<FinalTranscript, VoiceProviderError> {
        if !self.config.structurally_configured(true) || !self.config.available {
            return Err(VoiceProviderError::ProviderUnavailable);
        }
        validate_audio(&input)?;
        let key = self.resolve_api_key().await?;
        let mut form = Form::new()
            .text("model", self.config.stt_model.clone())
            .text("response_format", "json")
            .part(
                "file",
                Part::bytes(input.bytes)
                    .file_name(input.filename)
                    .mime_str(&normalize_media_type(&input.media_type))
                    .map_err(|_| {
                        VoiceProviderError::InvalidAudio("invalid media type".to_string())
                    })?,
            );
        if let Some(language) = input
            .language
            .as_deref()
            .or_else(|| non_empty(&self.config.language))
        {
            form = form.text("language", language.to_string());
        }
        let response = self
            .client
            .post(endpoint(&self.config.base_url, "audio/transcriptions"))
            .bearer_auth(key.expose_secret())
            .multipart(form)
            .send()
            .await
            .map_err(map_request_error)?;
        if !response.status().is_success() {
            return Err(VoiceProviderError::RequestFailed(
                response.status().as_u16(),
            ));
        }
        let content_length = response.content_length().unwrap_or(0);
        if content_length > MAX_AUDIO_BYTES as u64 {
            return Err(VoiceProviderError::InvalidResponse);
        }
        let payload = response
            .json::<Value>()
            .await
            .map_err(|_| VoiceProviderError::InvalidResponse)?;
        let text = payload
            .get("text")
            .and_then(Value::as_str)
            .ok_or(VoiceProviderError::InvalidResponse)?
            .trim()
            .to_string();
        validate_text(&text)?;
        Ok(FinalTranscript {
            text,
            provider: self.config.provider.clone(),
            model: self.config.stt_model.clone(),
            is_final: true,
            duration: None,
            language: input
                .language
                .or_else(|| non_empty(&self.config.language).map(str::to_string)),
        })
    }

    async fn synthesize_inner(
        &self,
        request: &SpeechRequest,
    ) -> Result<SpeechAudio, VoiceProviderError> {
        if !self.config.structurally_configured(false) || !self.config.available {
            return Err(VoiceProviderError::ProviderUnavailable);
        }
        validate_text(&request.text)?;
        let voice = if request.voice.trim().is_empty() {
            self.config.voice.trim()
        } else {
            request.voice.trim()
        };
        if voice.is_empty() || voice.chars().count() > 128 {
            return Err(VoiceProviderError::InvalidText);
        }
        let key = self.resolve_api_key().await?;
        let mut body = json!({
            "model": self.config.tts_model,
            "input": request.text,
            "voice": voice,
            "response_format": "mp3",
        });
        if let Some(language) = request
            .language
            .as_deref()
            .or_else(|| non_empty(&self.config.language))
        {
            body["language"] = Value::String(language.to_string());
        }
        let response = self
            .client
            .post(endpoint(&self.config.base_url, "audio/speech"))
            .bearer_auth(key.expose_secret())
            .json(&body)
            .send()
            .await
            .map_err(map_request_error)?;
        if !response.status().is_success() {
            return Err(VoiceProviderError::RequestFailed(
                response.status().as_u16(),
            ));
        }
        let media_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(normalize_media_type)
            .unwrap_or_default();
        if !is_audio_media_type(&media_type) {
            return Err(VoiceProviderError::UnsupportedMediaType);
        }
        if response.content_length().unwrap_or(0) > MAX_AUDIO_BYTES as u64 {
            return Err(VoiceProviderError::InvalidResponse);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| VoiceProviderError::InvalidResponse)?;
        if bytes.is_empty() || bytes.len() > MAX_AUDIO_BYTES {
            return Err(VoiceProviderError::InvalidResponse);
        }
        Ok(SpeechAudio {
            bytes: bytes.to_vec(),
            media_type,
            provider: self.config.provider.clone(),
        })
    }

    async fn resolve_api_key(&self) -> Result<secrecy::SecretString, VoiceProviderError> {
        let key = if let Some(secret_ref) = &self.config.api_key_ref {
            self.resolver.resolve_ref(secret_ref).await.ok().flatten()
        } else if !self.config.api_key_env.trim().is_empty() {
            std::env::var(&self.config.api_key_env)
                .ok()
                .map(secrecy::SecretString::from)
        } else {
            None
        };
        key.filter(|value| !value.expose_secret().trim().is_empty())
            .ok_or(VoiceProviderError::ProviderUnavailable)
    }
}

#[derive(Clone)]
pub struct OpenAiCompatibleSttProvider {
    inner: OpenAiCompatibleVoiceProvider,
}

impl OpenAiCompatibleSttProvider {
    pub fn from_config(config: &VoiceSttConfig, resolver: Arc<SecretResolver>) -> Self {
        let available = config.provider.eq_ignore_ascii_case("openai-compatible")
            && config.structurally_configured();
        Self {
            inner: OpenAiCompatibleVoiceProvider::new(
                VoiceProviderConfig {
                    provider: config.provider.clone(),
                    base_url: config.base_url.clone(),
                    stt_model: config.model.clone(),
                    tts_model: String::new(),
                    voice: String::new(),
                    language: config.language.clone(),
                    api_key_env: config.api_key_env.clone(),
                    api_key_ref: config.api_key_ref.clone(),
                    api_key_source: secret_source(
                        config.api_key_ref.as_ref(),
                        &config.api_key_env,
                        &config.api_key,
                    ),
                    timeout_ms: config.timeout_ms,
                    available,
                    unavailable_reason: (!available)
                        .then(|| "STT provider configuration is incomplete".to_string()),
                },
                resolver,
            ),
        }
    }
}

#[async_trait]
impl SpeechToTextProvider for OpenAiCompatibleSttProvider {
    async fn transcribe(&self, input: AudioInput) -> Result<FinalTranscript, VoiceProviderError> {
        self.inner.transcribe_inner(input).await
    }

    fn provider_name(&self) -> &str {
        &self.inner.config.provider
    }
}

/// MiniMax's published Speech-to-Text adapter.
///
/// The current public contract accepts one complete multipart audio file and
/// returns JSON.  The API can emit SSE deltas after that upload when
/// `stream=true`, but it is not a bidirectional microphone stream.  The
/// product boundary therefore uses the final-first form (`stream=false`) so
/// that every utterance is uploaded exactly once.
#[derive(Clone)]
pub struct MiniMaxSttProvider {
    config: VoiceSttConfig,
    resolver: Arc<SecretResolver>,
    client: reqwest::Client,
    requests: Arc<Semaphore>,
}

impl MiniMaxSttProvider {
    pub fn from_config(config: &VoiceSttConfig, resolver: Arc<SecretResolver>) -> Self {
        Self::from_config_with_request_gate(config, resolver, Arc::new(Semaphore::new(1)))
    }

    pub fn from_config_with_request_gate(
        config: &VoiceSttConfig,
        resolver: Arc<SecretResolver>,
        requests: Arc<Semaphore>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(1)))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config: config.clone(),
            resolver,
            client,
            requests,
        }
    }

    pub fn config(&self) -> VoiceSttConfig {
        self.config.clone()
    }

    pub fn is_structurally_available(&self) -> bool {
        self.config.provider.eq_ignore_ascii_case("minimax")
            && self.config.base_url.trim().len() > 0
            && self.config.model.trim() == MINIMAX_STT_MODEL
            && self.config.timeout_ms > 0
    }

    pub async fn is_available(&self) -> bool {
        self.is_structurally_available() && self.resolve_api_key().await.is_ok()
    }

    async fn resolve_api_key(&self) -> Result<secrecy::SecretString, VoiceProviderError> {
        self.resolver
            .resolve_voice_stt_api_key(&self.config)
            .await
            .map_err(|_| VoiceProviderError::ProviderUnavailable)?
            .filter(|key| !key.expose_secret().trim().is_empty())
            .ok_or(VoiceProviderError::ProviderUnavailable)
    }

    async fn transcribe_with_retry(
        &self,
        input: &AudioInput,
        key: &secrecy::SecretString,
    ) -> Result<FinalTranscript, VoiceProviderError> {
        let mut retries = 0;
        loop {
            let mut form = Form::new()
                .text("model", MINIMAX_STT_MODEL)
                .text("response_format", "json")
                .text("stream", "false");
            let part = Part::bytes(input.bytes.clone())
                .file_name(input.filename.clone())
                .mime_str(&normalize_media_type(&input.media_type))
                .map_err(|_| VoiceProviderError::UnsupportedAudio)?;
            form = form.part("file", part);

            let mut request = self
                .client
                .post(minimax_stt_endpoint(&self.config.base_url))
                .bearer_auth(key.expose_secret())
                .multipart(form);
            if let Some(language) = minimax_language(&input, &self.config) {
                let value = HeaderValue::from_str(language)
                    .map_err(|_| VoiceProviderError::TranscriptionFailed)?;
                request = request.header("language", value);
            }

            let response = match request.send().await {
                Ok(response) => response,
                Err(error) if error.is_timeout() && retries < MINIMAX_STT_MAX_RETRIES => {
                    retries += 1;
                    continue;
                }
                Err(error) if error.is_timeout() => return Err(VoiceProviderError::SttTimeout),
                Err(_) => return Err(VoiceProviderError::ProviderUnavailable),
            };
            let status = response.status().as_u16();
            if (500..=599).contains(&status) && retries < MINIMAX_STT_MAX_RETRIES {
                retries += 1;
                continue;
            }
            if !response.status().is_success() {
                return Err(map_minimax_http_status(status));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MINIMAX_STT_MAX_RESPONSE_BYTES as u64)
            {
                return Err(VoiceProviderError::TranscriptionFailed);
            }
            let bytes = response
                .bytes()
                .await
                .map_err(|_| VoiceProviderError::TranscriptionFailed)?;
            if bytes.len() > MINIMAX_STT_MAX_RESPONSE_BYTES {
                return Err(VoiceProviderError::TranscriptionFailed);
            }
            let payload = serde_json::from_slice::<MiniMaxSttResponse>(&bytes)
                .map_err(|_| VoiceProviderError::TranscriptionFailed)?;
            let text = payload.text.trim().to_string();
            if text.is_empty() {
                return Err(VoiceProviderError::TranscriptionFailed);
            }
            return Ok(FinalTranscript {
                text,
                provider: self.config.provider.clone(),
                model: MINIMAX_STT_MODEL.to_string(),
                is_final: true,
                duration: payload.duration,
                language: minimax_language(&input, &self.config).map(str::to_string),
            });
        }
    }
}

#[derive(Debug, Deserialize)]
struct MiniMaxSttResponse {
    text: String,
    #[serde(default)]
    duration: Option<f64>,
}

#[async_trait]
impl SpeechToTextProvider for MiniMaxSttProvider {
    async fn transcribe(&self, input: AudioInput) -> Result<FinalTranscript, VoiceProviderError> {
        if !self.is_structurally_available() {
            return Err(VoiceProviderError::ProviderUnavailable);
        }
        validate_minimax_audio(&input)?;
        let wait = Duration::from_millis(self.config.timeout_ms.max(1));
        let _permit = timeout(wait, self.requests.clone().acquire_owned())
            .await
            .map_err(|_| VoiceProviderError::SttTimeout)?
            .map_err(|_| VoiceProviderError::ProviderUnavailable)?;
        let key = self.resolve_api_key().await?;
        self.transcribe_with_retry(&input, &key).await
    }

    fn provider_name(&self) -> &str {
        &self.config.provider
    }
}

#[derive(Clone)]
pub struct OpenAiCompatibleTtsProvider {
    inner: OpenAiCompatibleVoiceProvider,
}

impl OpenAiCompatibleTtsProvider {
    pub fn from_config(config: &VoiceTtsConfig, resolver: Arc<SecretResolver>) -> Self {
        let available = config.provider.eq_ignore_ascii_case("openai-compatible")
            && config.structurally_configured();
        Self {
            inner: OpenAiCompatibleVoiceProvider::new(
                VoiceProviderConfig {
                    provider: config.provider.clone(),
                    base_url: config.base_url.clone(),
                    stt_model: String::new(),
                    tts_model: config.model.clone(),
                    voice: config.voice.clone(),
                    language: config.language.clone(),
                    api_key_env: config.api_key_env.clone(),
                    api_key_ref: config.api_key_ref.clone(),
                    api_key_source: secret_source(
                        config.api_key_ref.as_ref(),
                        &config.api_key_env,
                        &config.api_key,
                    ),
                    timeout_ms: config.timeout_ms,
                    available,
                    unavailable_reason: (!available)
                        .then(|| "TTS provider configuration is incomplete".to_string()),
                },
                resolver,
            ),
        }
    }
}

#[async_trait]
impl TextToSpeechProvider for OpenAiCompatibleTtsProvider {
    async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechAudio, VoiceProviderError> {
        self.inner.synthesize_inner(request).await
    }

    fn provider_name(&self) -> &str {
        &self.inner.config.provider
    }
}

#[derive(Clone)]
pub struct MiniMaxTtsProvider {
    config: VoiceTtsConfig,
    resolver: Arc<SecretResolver>,
    client: reqwest::Client,
}

impl MiniMaxTtsProvider {
    pub fn from_config(config: &VoiceTtsConfig, resolver: Arc<SecretResolver>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(1)))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config: config.clone(),
            resolver,
            client,
        }
    }

    pub fn is_structurally_available(&self) -> bool {
        self.config.provider.eq_ignore_ascii_case("minimax")
            && self.config.structurally_configured()
    }

    async fn resolve_api_key(&self) -> Result<secrecy::SecretString, VoiceProviderError> {
        self.resolver
            .resolve_voice_tts_api_key(&self.config)
            .await
            .map_err(|_| VoiceProviderError::ProviderUnavailable)?
            .filter(|key| !key.expose_secret().trim().is_empty())
            .ok_or(VoiceProviderError::ProviderUnavailable)
    }
}

#[async_trait]
impl TextToSpeechProvider for MiniMaxTtsProvider {
    async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechAudio, VoiceProviderError> {
        if !self.is_structurally_available() {
            return Err(VoiceProviderError::ProviderUnavailable);
        }
        validate_text(&request.text)?;
        let voice = if request.voice.trim().is_empty() {
            self.config.voice.trim()
        } else {
            request.voice.trim()
        };
        if voice.is_empty() || voice.chars().count() > 128 {
            return Err(VoiceProviderError::InvalidText);
        }
        let key = self.resolve_api_key().await?;
        let mut body = json!({
            "model": self.config.model,
            "text": request.text,
            "stream": false,
            "output_format": "hex",
            "voice_setting": {
                "voice_id": voice,
                "speed": 1.0,
                "vol": 1.0,
                "pitch": 0
            },
            "audio_setting": {
                "sample_rate": 32000,
                "bitrate": 128000,
                "format": "mp3",
                "channel": 1
            }
        });
        if let Some(language) = request
            .language
            .as_deref()
            .or_else(|| non_empty(&self.config.language))
        {
            body["language_boost"] = Value::String(language.to_string());
        }
        let response = self
            .client
            .post(endpoint(&self.config.base_url, "v1/t2a_v2"))
            .bearer_auth(key.expose_secret())
            .json(&body)
            .send()
            .await
            .map_err(map_request_error)?;
        if !response.status().is_success() {
            return Err(VoiceProviderError::RequestFailed(
                response.status().as_u16(),
            ));
        }
        const MAX_MINIMAX_RESPONSE_BYTES: usize = MAX_AUDIO_BYTES * 2 + 1024 * 1024;
        if response.content_length().unwrap_or(0) > MAX_MINIMAX_RESPONSE_BYTES as u64 {
            return Err(VoiceProviderError::InvalidResponse);
        }
        let response_bytes = response
            .bytes()
            .await
            .map_err(|_| VoiceProviderError::InvalidResponse)?;
        if response_bytes.len() > MAX_MINIMAX_RESPONSE_BYTES {
            return Err(VoiceProviderError::InvalidResponse);
        }
        let payload = serde_json::from_slice::<Value>(&response_bytes)
            .map_err(|_| VoiceProviderError::InvalidResponse)?;
        let status_code = payload
            .pointer("/base_resp/status_code")
            .and_then(Value::as_i64)
            .ok_or(VoiceProviderError::InvalidResponse)?;
        if status_code != 0 {
            return Err(VoiceProviderError::ProviderRejected(status_code));
        }
        let encoded = payload
            .pointer("/data/audio")
            .and_then(Value::as_str)
            .ok_or(VoiceProviderError::InvalidResponse)?;
        let bytes = decode_hex(encoded)?;
        if bytes.is_empty() || bytes.len() > MAX_AUDIO_BYTES {
            return Err(VoiceProviderError::InvalidResponse);
        }
        Ok(SpeechAudio {
            bytes,
            media_type: "audio/mpeg".to_string(),
            provider: self.config.provider.clone(),
        })
    }

    fn provider_name(&self) -> &str {
        &self.config.provider
    }
}

fn secret_source(reference: Option<&SecretRef>, env: &str, legacy: &str) -> SecretSource {
    if reference.is_some() {
        SecretSource::SecretStore
    } else if !env.trim().is_empty() {
        SecretSource::Environment
    } else if !legacy.is_empty() {
        SecretSource::LegacyPending
    } else {
        SecretSource::None
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, VoiceProviderError> {
    if value.len() % 2 != 0 || value.len() / 2 > MAX_AUDIO_BYTES {
        return Err(VoiceProviderError::InvalidResponse);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(value: u8) -> Result<u8, VoiceProviderError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(VoiceProviderError::InvalidResponse),
    }
}

#[async_trait]
impl SpeechToTextProvider for OpenAiCompatibleVoiceProvider {
    async fn transcribe(&self, input: AudioInput) -> Result<FinalTranscript, VoiceProviderError> {
        self.transcribe_inner(input).await
    }

    fn provider_name(&self) -> &str {
        &self.config.provider
    }
}

#[async_trait]
impl TextToSpeechProvider for OpenAiCompatibleVoiceProvider {
    async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechAudio, VoiceProviderError> {
        self.synthesize_inner(request).await
    }

    fn provider_name(&self) -> &str {
        &self.config.provider
    }
}

fn endpoint(base_url: &str, suffix: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        suffix.trim_start_matches('/')
    )
}

fn normalize_media_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn is_audio_media_type(value: &str) -> bool {
    value.starts_with("audio/") || value == "application/octet-stream"
}

fn validate_minimax_audio(input: &AudioInput) -> Result<(), VoiceProviderError> {
    if input.bytes.is_empty() {
        return Err(VoiceProviderError::UnsupportedAudio);
    }
    if input.bytes.len() > MAX_AUDIO_BYTES {
        return Err(VoiceProviderError::AudioTooLarge);
    }
    if input.filename.trim().is_empty()
        || input.filename.chars().any(char::is_control)
        || !minimax_audio_format_supported(&input.filename, &input.media_type)
    {
        return Err(VoiceProviderError::UnsupportedAudio);
    }
    Ok(())
}

/// MiniMax publishes a finite container list for ASR.  Do not silently send
/// browser-only containers such as WebM and call them supported by the API.
fn minimax_audio_format_supported(filename: &str, media_type: &str) -> bool {
    let media_type = normalize_media_type(media_type);
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let Some(extension) = extension else {
        return matches!(
            media_type.as_str(),
            "audio/wav"
                | "audio/x-wav"
                | "audio/wave"
                | "audio/aiff"
                | "audio/x-aiff"
                | "audio/flac"
                | "audio/x-flac"
                | "audio/mp4"
                | "audio/x-m4a"
                | "audio/m4a"
                | "audio/mpeg"
                | "audio/mp3"
                | "audio/aac"
                | "audio/x-aac"
                | "audio/opus"
                | "audio/ogg"
                | "application/octet-stream"
        );
    };
    match extension.as_str() {
        "wav" => matches!(
            media_type.as_str(),
            "audio/wav"
                | "audio/x-wav"
                | "audio/wave"
                | "application/wav"
                | "application/octet-stream"
        ),
        "aiff" | "aif" => matches!(
            media_type.as_str(),
            "audio/aiff" | "audio/x-aiff" | "application/octet-stream"
        ),
        "flac" => matches!(
            media_type.as_str(),
            "audio/flac" | "audio/x-flac" | "application/octet-stream"
        ),
        "m4a" | "alac" => matches!(
            media_type.as_str(),
            "audio/mp4" | "audio/x-m4a" | "audio/m4a" | "application/octet-stream"
        ),
        "mp3" => matches!(
            media_type.as_str(),
            "audio/mpeg" | "audio/mp3" | "application/octet-stream"
        ),
        "aac" => matches!(
            media_type.as_str(),
            "audio/aac" | "audio/x-aac" | "application/octet-stream"
        ),
        "opus" => matches!(
            media_type.as_str(),
            "audio/opus" | "application/octet-stream"
        ),
        "ogg" => matches!(
            media_type.as_str(),
            "audio/ogg" | "application/octet-stream"
        ),
        _ => false,
    }
}

fn map_minimax_http_status(status: u16) -> VoiceProviderError {
    match status {
        401 | 403 => VoiceProviderError::AuthFailed,
        408 => VoiceProviderError::SttTimeout,
        413 => VoiceProviderError::AudioTooLarge,
        429 => VoiceProviderError::RateLimited,
        400 => VoiceProviderError::UnsupportedAudio,
        422 => VoiceProviderError::TranscriptionFailed,
        500..=599 => VoiceProviderError::ProviderUnavailable,
        _ => VoiceProviderError::TranscriptionFailed,
    }
}

fn minimax_language<'a>(input: &'a AudioInput, config: &'a VoiceSttConfig) -> Option<&'a str> {
    let language = input
        .language
        .as_deref()
        .or_else(|| non_empty(&config.language))?;
    (!language.eq_ignore_ascii_case("auto")).then_some(language)
}

fn minimax_stt_endpoint(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    if base_url
        .rsplit_once('/')
        .is_some_and(|(prefix, suffix)| !prefix.is_empty() && suffix.eq_ignore_ascii_case("v1"))
    {
        format!("{base_url}/speech_to_text")
    } else {
        format!("{base_url}/v1/speech_to_text")
    }
}

fn validate_audio(input: &AudioInput) -> Result<(), VoiceProviderError> {
    if input.bytes.is_empty() || input.bytes.len() > MAX_AUDIO_BYTES {
        return Err(VoiceProviderError::InvalidAudio(
            "audio size is outside the accepted range".to_string(),
        ));
    }
    let media_type = normalize_media_type(&input.media_type);
    if !is_audio_media_type(&media_type) {
        return Err(VoiceProviderError::InvalidAudio(
            "audio media type is not accepted".to_string(),
        ));
    }
    if input.filename.trim().is_empty() || input.filename.chars().any(char::is_control) {
        return Err(VoiceProviderError::InvalidAudio(
            "audio filename is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_text(text: &str) -> Result<(), VoiceProviderError> {
    if text.trim().is_empty() || text.chars().count() > MAX_SPEECH_TEXT_CHARS {
        return Err(VoiceProviderError::InvalidText);
    }
    Ok(())
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value.trim())
}

fn map_request_error(error: reqwest::Error) -> VoiceProviderError {
    if error.is_timeout() {
        VoiceProviderError::Timeout
    } else {
        VoiceProviderError::ProviderUnavailable
    }
}
