// ============================================================
// Secret models — SecretRef, errors, status, and migration report.
//
// A `SecretRef` is a stable, non-secret pointer to an OS-backed credential.
// It is safe to Serialize / Debug / persist / send to the frontend — it is
// NOT the secret itself.
// ============================================================

use serde::{Deserialize, Serialize};

/// Persistent SecretRef schema version.
pub const SECRET_REF_VERSION: u8 = 1;
/// Maximum accepted secret value size (API keys / MCP env are far smaller).
pub const MAX_SECRET_VALUE_BYTES: usize = 64 * 1024;
/// Maximum SecretRef key length.
pub const MAX_SECRET_KEY_LEN: usize = 256;
/// Stable OS keyring service name.
pub const SECRET_SERVICE_NAME: &str = "io.yilianqianyan";

/// Stable SecretRef keys for the model secrets.
pub const CHAT_KEY_REF: &str = "model.chat.api_key";
pub const EMBEDDING_KEY_REF: &str = "model.embedding.api_key";
/// Stable SecretRef key for the provider-neutral voice API key.
pub const VOICE_KEY_REF: &str = "voice.api_key";
pub const STT_KEY_REF: &str = "voice.stt.api_key";
pub const TTS_KEY_REF: &str = "voice.tts.api_key";

/// Build a stable, non-sensitive keyring account for a saved LLM profile.
pub fn llm_model_key_ref(model_id: &str) -> SecretRef {
    SecretRef::new(format!("llm.model.{}", digest16(model_id.as_bytes())))
}

/// A stable, non-secret pointer to a secret. Never carries the value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretRef {
    pub version: u8,
    pub key: String,
}

impl SecretRef {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            version: SECRET_REF_VERSION,
            key: key.into(),
        }
    }

    /// Strictly validate the key: ASCII-safe, bounded, no control chars / CRLF.
    /// Although a SecretRef key is not a file path, it still must be safe to
    /// use as an OS credential account name.
    pub fn validate(&self) -> Result<(), SecretStoreError> {
        if self.version != SECRET_REF_VERSION {
            return Err(SecretStoreError::InvalidReference);
        }
        if self.key.is_empty() || self.key.len() > MAX_SECRET_KEY_LEN {
            return Err(SecretStoreError::InvalidReference);
        }
        if !self.key.chars().all(|c| c.is_ascii_graphic() || c == '.') {
            return Err(SecretStoreError::InvalidReference);
        }
        if self.key.chars().any(|c| c == '\r' || c == '\n') {
            return Err(SecretStoreError::InvalidReference);
        }
        Ok(())
    }
}

/// Build a stable MCP env SecretRef from a server id and env name. Both are
/// hashed (16 hex chars) so arbitrary names never become raw OS accounts.
pub fn mcp_env_ref(server_id: &str, env_name: &str) -> SecretRef {
    let server_digest = digest16(server_id.as_bytes());
    let env_digest = digest16(env_name.as_bytes());
    SecretRef::new(format!("mcp.{server_digest}.env.{env_digest}"))
}

fn digest16(bytes: &[u8]) -> String {
    let full = crate::safety::sha256_hex(bytes);
    full.chars().take(16).collect()
}

/// Categorised secret for audit + status (never the value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    ChatApiKey,
    EmbeddingApiKey,
    VoiceApiKey,
    VoiceSttApiKey,
    VoiceTtsApiKey,
    LlmModelApiKey,
    McpEnv,
}

/// Where a configured secret currently resolves from (status API).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    SecretStore,
    Environment,
    LegacyPending,
    None,
}

impl Default for SecretSource {
    fn default() -> Self {
        Self::None
    }
}

/// Whether the OS-backed store is usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretStoreStatus {
    Available,
    Unavailable,
    Locked,
}

/// Safe, value-free summary of a legacy migration run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretMigrationReport {
    pub migrated_chat_key: bool,
    pub migrated_embedding_key: bool,
    pub migrated_voice_stt_key: bool,
    pub migrated_voice_tts_key: bool,
    pub migrated_mcp_values: usize,
    pub pending: usize,
    pub failed: usize,
}

/// Typed, secret-safe error. `Display`/`Debug` must never contain a secret value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretStoreError {
    #[error("secret store unavailable")]
    Unavailable,
    #[error("secret not found")]
    NotFound,
    #[error("invalid secret reference")]
    InvalidReference,
    #[error("secret store backend error")]
    Backend,
    #[error("secret store access denied")]
    AccessDenied,
}
