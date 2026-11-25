use crate::execution::ExecutionOwner;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAction {
    Open,
    List,
    Navigate,
    Observe,
    Click,
    Fill,
    FillCredential,
    Scroll,
    Capture,
    Back,
    Forward,
    Reload,
    Stop,
    Close,
    Present,
}
impl BrowserAction {
    pub fn requires_replay_barrier(&self) -> bool {
        !matches!(
            self,
            Self::List | Self::Observe | Self::Capture | Self::Stop | Self::Close | Self::Present
        )
    }
    pub fn may_have_side_effects(&self) -> bool {
        !matches!(self, Self::List | Self::Observe | Self::Capture)
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::List => "list",
            Self::Navigate => "navigate",
            Self::Observe => "observe",
            Self::Click => "click",
            Self::Fill => "fill",
            Self::FillCredential => "fill_credential",
            Self::Scroll => "scroll",
            Self::Capture => "capture",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Reload => "reload",
            Self::Stop => "stop",
            Self::Close => "close",
            Self::Present => "present",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrowserTarget {
    pub session_id: String,
    pub tab_id: String,
    pub navigation_id: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserRequest {
    pub operation_id: String,
    pub action: BrowserAction,
    #[serde(default)]
    pub target: Option<BrowserTarget>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub owner: Option<ExecutionOwner>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTab {
    pub session_id: String,
    pub tab_id: String,
    pub workspace_id: Option<String>,
    pub site_id: Option<String>,
    pub url: String,
    pub title: String,
    pub navigation_id: u64,
    pub mode: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub error: Option<String>,
    pub ephemeral: bool,
}
impl BrowserTab {
    pub fn target(&self) -> BrowserTarget {
        BrowserTarget {
            session_id: self.session_id.clone(),
            tab_id: self.tab_id.clone(),
            navigation_id: self.navigation_id,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserReply {
    pub state: String,
    pub data: Value,
}
impl BrowserReply {
    pub fn verified(data: Value) -> Self {
        Self {
            state: "verified".into(),
            data,
        }
    }
    pub fn unknown(operation_id: &str, reason: &str) -> Self {
        Self {
            state: "result_unknown".into(),
            data: serde_json::json!({"operation_id":operation_id,"reason":reason,"automatic_replay_allowed":false}),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct BrowserError {
    pub code: String,
    pub message: String,
}
impl BrowserError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
#[async_trait]
pub trait BrowserHost: Send + Sync {
    fn tabs(&self) -> Vec<BrowserTab>;
    /// Must revoke pending creations, close all native views/proxies and destroy
    /// ephemeral session state. Unsupported hosts fail profile activation closed.
    async fn shutdown(&self) -> Result<(), BrowserError> {
        Err(BrowserError::new(
            "native_shutdown_unsupported",
            "Native browser host does not implement verified session shutdown",
        ))
    }
    async fn dispatch(
        &self,
        request: BrowserRequest,
        cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError>;
}
