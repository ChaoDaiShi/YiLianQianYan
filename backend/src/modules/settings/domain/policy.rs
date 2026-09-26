//! Settings provider policy — which provider identifiers and URLs are accepted.

pub(crate) fn valid_provider_url(value: &str) -> bool {
    url::Url::parse(value.trim())
        .map(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .unwrap_or(false)
}

pub(crate) fn supported_voice_provider(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("openai-compatible") || provider.eq_ignore_ascii_case("minimax")
}
