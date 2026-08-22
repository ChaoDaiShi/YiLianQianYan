use std::sync::OnceLock;

use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const MAX_TEXT_CHARS: usize = 2_048;
const PREVIEW_CHARS: usize = 256;
const REDACTED: &str = "[REDACTED]";
const SENSITIVE_KEY_PARTS: [&str; 8] = [
    "api_key",
    "token",
    "password",
    "secret",
    "authorization",
    "cookie",
    "private_key",
    "private-key",
];

/// Sensitive substrings used for *content-based* secret detection. This is the
/// single source of truth shared by Chat Memory Extraction and Agent Memory
/// Learning: any content whose lowercased form contains one of these is never
/// persisted as a memory.
const SENSITIVE_CONTENT_PATTERNS: &[&str] = &[
    "api_key",
    "apikey",
    "api key",
    "token",
    "password",
    "secret",
    "authorization",
    "cookie",
    "private_key",
    "private key",
    "bearer",
    "sk-",
];

/// Returns true when `text` (lowercased) contains a sensitive keyword.
///
/// Shared by Chat Memory Extraction and Agent Memory Learning so that a secret
/// rejected by one path can never be accepted by the other.
pub fn contains_sensitive_content(text: &str) -> bool {
    let lower = text.to_lowercase();
    SENSITIVE_CONTENT_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

#[derive(Debug, Clone, PartialEq)]
pub struct RedactedJson {
    pub value: Value,
    pub digest: String,
}

pub fn redact_and_digest(value: &Value) -> RedactedJson {
    let safe_value = redact_value(value);
    let canonical = serde_json::to_vec(&safe_value).unwrap_or_else(|_| b"null".to_vec());
    RedactedJson {
        value: safe_value,
        digest: sha256_hex(&canonical),
    }
}

pub fn redact_error(message: &str) -> String {
    let safe = redact_inline_secrets(message);
    let length = safe.chars().count();
    if length <= MAX_TEXT_CHARS {
        return safe;
    }

    let preview = safe.chars().take(PREVIEW_CHARS).collect::<String>();
    format!(
        "{preview}… [truncated length={length} sha256={}]",
        sha256_hex(safe.as_bytes())
    )
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let mut safe = Map::new();
            for key in keys {
                let next = if is_sensitive_key(key) {
                    Value::String(REDACTED.to_string())
                } else {
                    redact_value(&object[key])
                };
                safe.insert(key.clone(), next);
            }
            Value::Object(safe)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::String(text) => redact_string(text),
        primitive => primitive.clone(),
    }
}

fn redact_string(text: &str) -> Value {
    if is_data_uri(text) {
        return json!({
            "kind": "omitted_data_uri",
            "length": text.chars().count(),
            "sha256": sha256_hex(text.as_bytes()),
        });
    }

    let original_length = text.chars().count();
    let safe = redact_inline_secrets(text);
    if original_length > MAX_TEXT_CHARS {
        return json!({
            "kind": "truncated_text",
            "preview": safe.chars().take(PREVIEW_CHARS).collect::<String>(),
            "length": original_length,
            "sha256": sha256_hex(safe.as_bytes()),
        });
    }

    Value::String(safe)
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    SENSITIVE_KEY_PARTS
        .iter()
        .any(|part| normalized.contains(part))
}

fn is_data_uri(text: &str) -> bool {
    text.as_bytes()
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"data:"))
}

fn redact_inline_secrets(text: &str) -> String {
    static BEARER: OnceLock<Regex> = OnceLock::new();
    static ASSIGNMENT: OnceLock<Regex> = OnceLock::new();

    let bearer = BEARER.get_or_init(|| {
        Regex::new(r"(?i)\bbearer\s+[a-z0-9._~+/=-]+").expect("valid bearer redaction regex")
    });
    let assignment = ASSIGNMENT.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(api[_-]?key|token|password|secret|authorization|cookie|private[_-]?key)(\s*[:=]\s*)(?:\"[^\"]*\"|'[^']*'|[^\s,;]+)"#,
        )
        .expect("valid credential assignment regex")
    });

    let without_bearer = bearer.replace_all(text, "Bearer [REDACTED]");
    assignment
        .replace_all(&without_bearer, format!("$1$2{REDACTED}"))
        .into_owned()
}
