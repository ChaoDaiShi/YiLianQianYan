//! Settings provider policy — which provider identifiers and URLs are accepted.

pub(crate) fn valid_provider_url(value: &str) -> bool {
    url::Url::parse(value.trim())
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .unwrap_or(false)
}

pub(crate) fn valid_model_temperature(value: f64) -> bool {
    (0.0..=2.0).contains(&value)
}

pub(crate) fn valid_model_max_tokens(value: u32) -> bool {
    (1..=1_000_000).contains(&value)
}

pub(crate) fn valid_model_timeout(value: u64) -> bool {
    (1_000..=600_000).contains(&value)
}

pub(crate) fn supported_voice_tts_provider(provider: &str) -> bool {
    matches!(
        provider.to_ascii_lowercase().as_str(),
        "openai-compatible" | "minimax" | "fish" | "aliyun" | "volcengine"
    )
}

pub(crate) fn supported_voice_stt_provider(provider: &str) -> bool {
    matches!(
        provider.to_ascii_lowercase().as_str(),
        "openai-compatible" | "minimax" | "fish" | "aliyun"
    )
}
