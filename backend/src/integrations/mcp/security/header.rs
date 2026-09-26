// ============================================================
// MCP header safety — encoding and HTTP header injection defense.
//
// Every outbound header derived from tool arguments passes through here. A
// value is either a legal ASCII field value or is base64-wrapped; CR/LF never
// survive. The same rules are enforced again when scanning `x-mcp-header`
// annotations (`security::header_schema`).
// ============================================================

use base64::Engine as _;

/// Validate a header value has no CR/LF (header injection) and is a legal
/// HTTP field value.
pub fn reject_crlf(value: &str) -> Result<(), String> {
    if value.contains('\r') || value.contains('\n') {
        return Err("header value contains CR/LF".to_string());
    }
    Ok(())
}

/// Whether a value is a legal HTTP field-name token (RFC 7230 token).
pub fn is_valid_header_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

/// Encode an MCP header value safely. If the value is non-ASCII, contains a
/// control character, has leading/trailing whitespace, or already matches the
/// base64 sentinel pattern, it is wrapped as `=?base64?<b64>?=`.
pub fn encode_header_value(value: &str) -> String {
    let needs_encoding = value.chars().any(|c| !c.is_ascii() || c.is_control())
        || value != value.trim()
        || value.starts_with("=?")
        || value.is_empty();
    if needs_encoding {
        let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
        format!("=?base64?{encoded}?=")
    } else {
        value.to_string()
    }
}

/// The `Mcp-Param-{Name}` header for an `x-mcp-header` annotated argument.
pub fn param_header(name: &str) -> String {
    format!("Mcp-Param-{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_header_encoding_non_ascii() {
        let encoded = encode_header_value("你好");
        assert!(encoded.starts_with("=?base64?"));
        assert!(encoded.ends_with("?="));
    }

    #[test]
    fn base64_header_encoding_whitespace() {
        assert!(encode_header_value(" has space ").starts_with("=?base64?"));
        assert_eq!(encode_header_value("plain"), "plain");
    }

    #[test]
    fn header_crlf_injection_rejected() {
        assert!(reject_crlf("value\r\nInjected: x").is_err());
        assert!(reject_crlf("ok").is_ok());
    }

    #[test]
    fn header_token_validation() {
        assert!(is_valid_header_token("X-Custom-Tool"));
        assert!(!is_valid_header_token(""));
        assert!(!is_valid_header_token("bad header"));
    }
}
