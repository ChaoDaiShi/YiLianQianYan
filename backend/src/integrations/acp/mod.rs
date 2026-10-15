//! Optional ACP v1 process adapter for configured external agents.

pub mod client;
pub mod gateway_permission;
pub mod model;
pub mod session;
pub mod tool;

pub use client::{AcpClient, AcpError, AcpPermissionHandler};
pub use gateway_permission::GatewayAcpPermissionHandler;
pub use model::{
    AcpContextUsage, AcpLaunchConfig, AcpPermissionDecision, AcpPermissionRequest,
    AcpPromptOutcome, AcpSessionUpdate, ExternalSessionState, ExternalTaskResult,
};
pub use session::AcpSession;
pub use tool::{
    register_configured_acp_agent, register_configured_acp_agent_with_slot, AcpAgentTool,
    AcpToolError,
};

#[cfg(test)]
mod tests;
