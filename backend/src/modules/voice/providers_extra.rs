use super::provider::{
    AudioInput, FinalTranscript, SpeechAudio, SpeechRequest, SpeechToTextProvider,
    TextToSpeechProvider, VoiceProviderError, MAX_AUDIO_BYTES, MAX_SPEECH_TEXT_CHARS,
};
use crate::{
    config::types::{VoiceSttConfig, VoiceTtsConfig},
    integrations::secret::SecretResolver,
};
use async_trait::async_trait;
use base64::Engine;
use secrecy::ExposeSecret;
use serde_json::{json, Value};
use std::sync::Arc;
fn network(error: reqwest::Error) -> VoiceProviderError {
    if error.is_timeout() {
        VoiceProviderError::Timeout
    } else {
        VoiceProviderError::ProviderUnavailable
    }
}
fn check(response: reqwest::Response) -> Result<reqwest::Response, VoiceProviderError> {
    match response.status().as_u16() {
        200..=299 => Ok(response),
        401 | 403 => Err(VoiceProviderError::AuthFailed),
        429 => Err(VoiceProviderError::RateLimited),
        status => Err(VoiceProviderError::RequestFailed(status)),
    }
}
fn client(timeout: u64) -> Result<reqwest::Client, VoiceProviderError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout))
        .build()
        .map_err(|_| VoiceProviderError::ProviderUnavailable)
}
pub struct AdditionalSttProvider {
    config: VoiceSttConfig,
    resolver: Arc<SecretResolver>,
}
impl AdditionalSttProvider {
    pub fn from_config(config: &VoiceSttConfig, resolver: Arc<SecretResolver>) -> Self {
        Self {
            config: config.clone(),
            resolver,
        }
    }
}
#[async_trait]
impl SpeechToTextProvider for AdditionalSttProvider {
    fn provider_name(&self) -> &str {
        &self.config.provider
    }
    async fn transcribe(&self, input: AudioInput) -> Result<FinalTranscript, VoiceProviderError> {
        if input.bytes.is_empty() {
            return Err(VoiceProviderError::UnsupportedAudio);
        }
        if input.bytes.len() > MAX_AUDIO_BYTES {
            return Err(VoiceProviderError::AudioTooLarge);
        }
        let key = self
            .resolver
            .resolve_voice_stt_api_key(&self.config)
            .await
            .map_err(|_| VoiceProviderError::ProviderUnavailable)?
            .ok_or(VoiceProviderError::AuthFailed)?;
        let base = self.config.base_url.trim_end_matches('/');
        let http = client(self.config.timeout_ms)?;
        let response = match self.config.provider.as_str() {
            "fish" => {
                let part = reqwest::multipart::Part::bytes(input.bytes)
                    .file_name(input.filename)
                    .mime_str(&input.media_type)
                    .map_err(|_| VoiceProviderError::UnsupportedAudio)?;
                let form = reqwest::multipart::Form::new()
                    .part("audio", part)
                    .text("language", self.config.language.clone())
                    .text("ignore_timestamps", "true");
                http.post(format!("{base}/asr"))
                    .bearer_auth(key.expose_secret())
                    .header("model", &self.config.model)
                    .multipart(form)
                    .send()
                    .await
                    .map_err(network)?
            }
            "aliyun" => {
                let data = format!(
                    "data:{};base64,{}",
                    input.media_type,
                    base64::engine::general_purpose::STANDARD.encode(input.bytes)
                );
                http.post(format!("{base}/chat/completions")).bearer_auth(key.expose_secret()).json(&json!({"model":self.config.model,"messages":[{"role":"user","content":[{"type":"input_audio","input_audio":{"data":data}}]}],"stream":false})).send().await.map_err(network)?
            }
            _ => return Err(VoiceProviderError::ProviderUnavailable),
        };
        let value: Value = check(response)?.json().await.map_err(network)?;
        let text = if self.config.provider == "fish" {
            value.get("text")
        } else {
            value.pointer("/choices/0/message/content")
        }
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or(VoiceProviderError::InvalidResponse)?;
        Ok(FinalTranscript {
            text: text.into(),
            provider: self.config.provider.clone(),
            model: self.config.model.clone(),
            is_final: true,
            duration: value.get("duration").and_then(Value::as_f64),
            language: Some(self.config.language.clone()),
        })
    }
}
pub struct AdditionalTtsProvider {
    config: VoiceTtsConfig,
    resolver: Arc<SecretResolver>,
}
impl AdditionalTtsProvider {
    pub fn from_config(config: &VoiceTtsConfig, resolver: Arc<SecretResolver>) -> Self {
        Self {
            config: config.clone(),
            resolver,
        }
    }
}
#[async_trait]
impl TextToSpeechProvider for AdditionalTtsProvider {
    fn provider_name(&self) -> &str {
        &self.config.provider
    }
    async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechAudio, VoiceProviderError> {
        if request.text.trim().is_empty() || request.text.chars().count() > MAX_SPEECH_TEXT_CHARS {
            return Err(VoiceProviderError::InvalidText);
        }
        let key = self
            .resolver
            .resolve_voice_tts_api_key(&self.config)
            .await
            .map_err(|_| VoiceProviderError::ProviderUnavailable)?
            .ok_or(VoiceProviderError::AuthFailed)?;
        let http = client(self.config.timeout_ms)?;
        let base = self.config.base_url.trim_end_matches('/');
        if self.config.provider == "volcengine" {
            let response=http.post(format!("{base}/tts/unidirectional")).header("X-Api-Key",key.expose_secret()).header("X-Api-Resource-Id",&self.config.model).header("X-Api-Request-Id",uuid::Uuid::new_v4().to_string()).json(&json!({"req_params":{"text":request.text,"speaker":request.voice,"audio_params":{"format":"mp3","sample_rate":24000}}})).send().await.map_err(network)?;
            let body = check(response)?.text().await.map_err(network)?;
            let mut bytes = Vec::new();
            for frame in serde_json::Deserializer::from_str(&body).into_iter::<Value>() {
                let frame = frame.map_err(|_| VoiceProviderError::InvalidResponse)?;
                let code = frame
                    .get("code")
                    .and_then(Value::as_i64)
                    .ok_or(VoiceProviderError::InvalidResponse)?;
                if code != 0 && code != 20000000 {
                    return Err(VoiceProviderError::ProviderRejected(code));
                }
                if let Some(data) = frame.get("data").and_then(Value::as_str) {
                    let chunk = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|_| VoiceProviderError::InvalidResponse)?;
                    bytes.extend(chunk);
                }
                if bytes.len() > MAX_AUDIO_BYTES {
                    return Err(VoiceProviderError::InvalidResponse);
                }
            }
            if bytes.is_empty() {
                return Err(VoiceProviderError::InvalidResponse);
            }
            return Ok(SpeechAudio {
                bytes,
                media_type: "audio/mpeg".into(),
                provider: self.config.provider.clone(),
            });
        }

        let response = match self.config.provider.as_str() {
            "fish" => http
                .post(format!("{base}/tts"))
                .bearer_auth(key.expose_secret())
                .header("model", &self.config.model)
                .json(&json!({"text":request.text,"reference_id":request.voice,"format":"mp3"}))
                .send()
                .await
                .map_err(network)?,
            "aliyun" => {
                let response=http.post(format!("{base}/services/audio/tts/SpeechSynthesizer")).bearer_auth(key.expose_secret()).json(&json!({"model":self.config.model,"input":{"text":request.text,"voice":request.voice,"format":"wav","sample_rate":24000}})).send().await.map_err(network)?;
                let value: Value = check(response)?.json().await.map_err(network)?;
                let audio_url = value
                    .pointer("/output/audio/url")
                    .and_then(Value::as_str)
                    .ok_or(VoiceProviderError::InvalidResponse)?;
                http.get(audio_url).send().await.map_err(network)?
            }
            _ => return Err(VoiceProviderError::ProviderUnavailable),
        };
        let response = check(response)?;
        let media = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .ok_or(VoiceProviderError::UnsupportedMediaType)?
            .split(';')
            .next()
            .unwrap_or("")
            .to_owned();
        if !media.starts_with("audio/") && media != "application/octet-stream" {
            return Err(VoiceProviderError::UnsupportedMediaType);
        }
        let bytes = response.bytes().await.map_err(network)?.to_vec();
        if bytes.is_empty() || bytes.len() > MAX_AUDIO_BYTES {
            return Err(VoiceProviderError::InvalidResponse);
        }
        Ok(SpeechAudio {
            bytes,
            media_type: media,
            provider: self.config.provider.clone(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        routing::{get, post},
        Json, Router,
    };
    #[tokio::test]
    async fn additional_voice_http_round_trip() {
        for provider in ["fish", "aliyun"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let download = format!("{base}/audio.wav");
            let app = Router::new()
                .route(
                    "/asr",
                    post(|| async { Json(json!({"text":"你好","duration":1.0})) }),
                )
                .route(
                    "/chat/completions",
                    post(|Json(v): Json<Value>| async move {
                        assert!(v
                            .pointer("/messages/0/content/0/input_audio/data")
                            .and_then(Value::as_str)
                            .unwrap()
                            .starts_with("data:audio/wav;base64,"));
                        Json(json!({"choices":[{"message":{"content":"你好"}}]}))
                    }),
                )
                .route(
                    "/tts",
                    post(|Json(v): Json<Value>| async move {
                        assert_eq!(v["reference_id"], "voice");
                        ([("content-type", "audio/mpeg")], vec![1u8, 2, 3])
                    }),
                )
                .route(
                    "/services/audio/tts/SpeechSynthesizer",
                    post(move |Json(v): Json<Value>| {
                        let url = download.clone();
                        async move {
                            assert_eq!(v["input"]["voice"], "voice");
                            Json(json!({"output":{"audio":{"url":url}}}))
                        }
                    }),
                )
                .route(
                    "/audio.wav",
                    get(|| async { ([("content-type", "audio/wav")], vec![1u8, 2, 3]) }),
                );
            let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let resolver = Arc::new(SecretResolver::new(Arc::new(
                crate::integrations::secret::InMemorySecretStore::new(),
            )));
            let stt = AdditionalSttProvider::from_config(
                &VoiceSttConfig {
                    provider: provider.into(),
                    base_url: base.clone(),
                    model: "fixture".into(),
                    api_key: "fixture".into(),
                    api_key_env: String::new(),
                    ..Default::default()
                },
                resolver.clone(),
            );
            let tts = AdditionalTtsProvider::from_config(
                &VoiceTtsConfig {
                    provider: provider.into(),
                    base_url: base,
                    model: "fixture".into(),
                    voice: "voice".into(),
                    api_key: "fixture".into(),
                    api_key_env: String::new(),
                    ..Default::default()
                },
                resolver,
            );
            assert_eq!(
                stt.transcribe(AudioInput {
                    bytes: vec![1, 2, 3],
                    media_type: "audio/wav".into(),
                    filename: "fixture.wav".into(),
                    language: None
                })
                .await
                .unwrap()
                .text,
                "你好"
            );
            assert_eq!(
                tts.synthesize(&SpeechRequest::new("你好", "voice"))
                    .await
                    .unwrap()
                    .bytes,
                vec![1, 2, 3]
            );
            task.abort();
        }
    }
}
#[cfg(test)]
mod volc_tests {
    use super::*;
    use axum::{routing::post, Json, Router};
    #[tokio::test]
    async fn volcengine_tts_http_chunks_decode_in_order() {
        let app=Router::new().route("/tts/unidirectional",post(|headers:axum::http::HeaderMap,Json(body):Json<Value>|async move{assert_eq!(headers["x-api-resource-id"],"seed-tts-2.0");assert_eq!(body["req_params"]["speaker"],"voice");"{\"code\":0,\"data\":\"AQI=\"}\n{\"code\":0,\"data\":\"Aw==\"}\n{\"code\":20000000}"}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let resolver = Arc::new(SecretResolver::new(Arc::new(
            crate::integrations::secret::InMemorySecretStore::new(),
        )));
        let provider = AdditionalTtsProvider::from_config(
            &VoiceTtsConfig {
                provider: "volcengine".into(),
                model: "seed-tts-2.0".into(),
                base_url: base,
                api_key: "fixture".into(),
                api_key_env: String::new(),
                ..Default::default()
            },
            resolver,
        );
        let audio = provider
            .synthesize(&SpeechRequest::new("fixture", "voice"))
            .await
            .unwrap();
        assert_eq!(audio.bytes, vec![1, 2, 3]);
        assert_eq!(audio.media_type, "audio/mpeg");
        task.abort();
    }
}
