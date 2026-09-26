// ============================================================
// MCP transport configuration — stdio vs Streamable HTTP.
//
// Legacy `command`/`args`/`env` configs deserialize as Stdio (backward
// compatible). Streamable HTTP uses env-var *names* (never plaintext secrets)
// and a strict URL policy.
//
// This module is configuration data plus the legacy plugin-row conversion. The
// behaviour that consumes it (the `McpTransport` trait and its stdio / HTTP
// implementations) lives in `transport.rs`, `stdio.rs` and `http.rs`.
// ============================================================

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// Non-secret environment (legacy / non-secret only). Secret values are
        /// referenced via `env_secret_refs` and resolved at spawn time.
        #[serde(default)]
        env: BTreeMap<String, String>,
        /// env name → SecretRef (resolved through the SecretResolver at spawn).
        #[serde(default)]
        env_secret_refs: BTreeMap<String, crate::integrations::secret::SecretRef>,
    },
    StreamableHttp {
        url: String,
        /// Header name → environment variable *name* (never a literal value).
        #[serde(default)]
        headers_from_env: BTreeMap<String, String>,
    },
}

/// Validate a Streamable HTTP URL: https, or http on loopback only; no
/// embedded credentials; no fragment.
pub fn validate_mcp_url(url: &str) -> Result<(), String> {
    if url.contains('#') {
        return Err("MCP URL must not contain a fragment".to_string());
    }
    let scheme = if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err("MCP URL must be http(s)".to_string());
    };
    let authority = scheme.1.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return Err("MCP URL has no host".to_string());
    }
    if authority.contains('@') {
        return Err("MCP URL must not contain embedded credentials".to_string());
    }
    let host = extract_host(authority);
    if scheme.0 == "http" {
        if host == "localhost" || host == "127.0.0.1" || host == "::1" {
            Ok(())
        } else {
            Err("remote plain HTTP MCP is not allowed".to_string())
        }
    } else {
        Ok(())
    }
}

fn extract_host(authority: &str) -> &str {
    if authority.starts_with('[') {
        if let Some(end) = authority.find(']') {
            return &authority[1..end];
        }
    }
    authority.split(':').next().unwrap_or("")
}

/// Convert a legacy DB MCP server row into a runtime transport config.
///
/// Moved here from `app::state` so the plugin row shape and the transport config
/// it maps onto are defined next to each other; `app::state` no longer needs to
/// know the transport vocabulary.
pub(crate) fn mcp_transport_config(server: &crate::db::McpServer) -> McpTransportConfig {
    let env_map: BTreeMap<String, String> = server
        .env
        .as_ref()
        .and_then(|e| e.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    match server.transport.as_str() {
        "http" | "streamable_http" => McpTransportConfig::StreamableHttp {
            url: server.url.clone().unwrap_or_default(),
            headers_from_env: env_map,
        },
        _ => McpTransportConfig::Stdio {
            command: server.command.clone().unwrap_or_default(),
            args: server.args.clone().unwrap_or_default(),
            env: env_map,
            env_secret_refs: server.env_secret_refs.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localhost_http_allowed() {
        assert!(validate_mcp_url("http://localhost:8080/mcp").is_ok());
        assert!(validate_mcp_url("http://127.0.0.1:8080/mcp").is_ok());
    }

    #[test]
    fn remote_plain_http_rejected() {
        assert!(validate_mcp_url("http://example.com/mcp").is_err());
    }

    #[test]
    fn url_credentials_rejected() {
        assert!(validate_mcp_url("https://user:pass@example.com/mcp").is_err());
    }

    #[test]
    fn fragment_rejected() {
        assert!(validate_mcp_url("https://example.com/mcp#frag").is_err());
    }

    #[test]
    fn https_allowed() {
        assert!(validate_mcp_url("https://example.com/mcp").is_ok());
    }
}
