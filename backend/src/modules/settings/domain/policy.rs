//! Settings provider policy — which provider identifiers and URLs are accepted.

pub(crate) fn valid_provider_url(value: &str) -> bool {
    url::Url::parse(value.trim())
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .unwrap_or(false)
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
