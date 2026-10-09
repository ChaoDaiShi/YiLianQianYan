// ============================================================
// Capability provider abstraction.
//
// A provider only *discovers* capabilities. It never executes a tool, never
// invokes MCP `tools/call`, and never grants permission.
// ============================================================

use async_trait::async_trait;
use thiserror::Error;

use super::model::{CapabilityDescriptor, CapabilityProviderKind};

#[derive(Debug, Error)]
pub enum CapabilityProviderError {
    #[error("provider discovery failed: {0}")]
    Discovery(String),
    #[error("provider produced an invalid descriptor: {0}")]
    InvalidDescriptor(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
}

#[async_trait]
pub trait CapabilityProvider: Send + Sync {
    fn provider_kind(&self) -> CapabilityProviderKind;

    async fn discover(&self) -> Result<Vec<CapabilityDescriptor>, CapabilityProviderError>;
}
