//! Software registrations are configuration, never grants or executable plugins.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegistrationInput {
    pub name: String,
    pub adapter: String,
    pub executable: String,
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub allow_observe: bool,
    #[serde(default)]
    pub allow_content: bool,
    #[serde(default)]
    pub allow_window_ocr: bool,
    #[serde(default)]
    pub allow_actions: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Registration {
    pub id: String,
    pub revision: u64,
    #[serde(flatten)]
    pub config: RegistrationInput,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WindowIdentity {
    pub hwnd: String,
    pub pid: u32,
    #[serde(with = "decimal_u64")]
    pub process_started_at: u64,
    pub executable: String,
    pub class_name: String,
}

// Windows FILETIME exceeds JavaScript's exact integer range. It is an
// identity witness, so JSON must retain every bit through browser clients.
mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let raw = String::deserialize(deserializer)?;
        let parsed = raw.parse::<u64>().map_err(serde::de::Error::custom)?;
        if parsed.to_string() != raw {
            return Err(serde::de::Error::custom(
                "Creation time must be a canonical decimal string",
            ));
        }
        Ok(parsed)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowCandidate {
    pub identity: WindowIdentity,
    pub visible: bool,
    pub minimized: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowState {
    pub identity: WindowIdentity,
    pub mode: String,
    pub visible: bool,
    pub focused: bool,
    pub minimized: bool,
    pub embedding_supported: bool,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionTarget {
    pub session_id: String,
    pub generation: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub registration_id: String,
    pub generation: u64,
    pub window: WindowState,
    pub state: String,
    pub launched_by_host: bool,
    pub last_error: Option<String>,
}
impl Session {
    pub fn target(&self) -> SessionTarget {
        SessionTarget {
            session_id: self.id.clone(),
            generation: self.generation,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareControl {
    pub id: String,
    pub name: String,
    pub role: String,
    pub automation_id: String,
    pub enabled: bool,
    pub can_invoke: bool,
    pub can_set_value: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expand_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_text: Option<String>,
    pub is_password: bool,
    pub fingerprint: String,
    #[serde(skip)]
    pub path: Vec<u32>,
}
impl SoftwareControl {
    pub fn supports(&self, action: &str) -> bool {
        self.enabled
            && !self.is_password
            && match action {
                "focus" | "focus_control" => true,
                "invoke" => self.can_invoke,
                "set_value" => self.can_set_value,
                "select" => self.selected.is_some(),
                "check" | "uncheck" => self.toggle_state.is_some(),
                "expand" | "collapse" => self.expand_state.as_deref().is_some_and(|s| s != "leaf"),
                _ => false,
            }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub target: SessionTarget,
    pub controls: Vec<SoftwareControl>,
    pub truncated: bool,
    pub untrusted_application_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_evidence: Option<WindowEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WechatConversation {
    pub title: String,
    pub messages: Vec<String>,
    #[serde(skip)]
    pub message_rows: Vec<String>,
    pub draft: String,
    pub minimized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRecord {
    pub operation_id: String,
    pub action: String,
    pub session_id: Option<String>,
    pub registration_id: String,
    pub state: String,
    pub reason: Option<String>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_evidence: Option<WindowEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowEvidence {
    pub generation: u64,
    pub mode: String,
    pub visible: bool,
    pub minimized: bool,
    pub observed_at: i64,
}

impl WindowEvidence {
    pub(super) fn capture(generation: u64, window: &WindowState) -> Self {
        Self {
            generation,
            mode: window.mode.clone(),
            visible: window.visible,
            minimized: window.minimized,
            observed_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub(super) fn matches(&self, session: &Session) -> bool {
        self.generation == session.generation
            && self.mode == session.window.mode
            && self.visible == session.window.visible
            && self.minimized == session.window.minimized
    }

    pub(super) fn is_background(&self) -> bool {
        !self.visible || self.minimized
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SoftwareCapabilityEvidence {
    pub registration_id: String,
    pub session_id: Option<String>,
    pub generation: Option<u64>,
    pub entries: Vec<SoftwareCapabilityEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SoftwareCapabilityEntry {
    pub dimension: &'static str,
    pub status: &'static str,
    pub source: &'static str,
    pub evidence_id: Option<String>,
    pub observed_at: Option<i64>,
}

/// Source-filtered Windows notification IDs. The payload stays in Windows.
pub enum SoftwareNotificationSnapshot {
    Available(Vec<u32>),
    AccessRequired,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct SoftwareError {
    pub code: String,
    pub message: String,
}
impl SoftwareError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn unknown(message: impl Into<String>) -> Self {
        Self::new("result_unknown", message)
    }
}

/// Synchronous platform work runs off the async executor. Implementations must
/// verify exact executable, PID creation time, HWND and class on EVERY call.
/// Parent HWND is supplied by trusted native bootstrap, never request JSON.
pub trait SoftwareHost: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    fn discover(&self, registration: &Registration) -> Result<Vec<WindowCandidate>, SoftwareError>;
    fn launch(&self, registration: &Registration) -> Result<WindowCandidate, SoftwareError>;
    fn verify(&self, target: &WindowIdentity) -> Result<WindowState, SoftwareError>;
    /// A metadata-only check for the original process lifetime.
    fn monitor_process_alive(&self, _target: &WindowIdentity) -> Result<bool, SoftwareError> {
        Err(SoftwareError::new(
            "software_monitor_unavailable",
            "Lifecycle monitoring requires a desktop host",
        ))
    }
    /// Events wake the monitor early. The monitor still verifies the window state.
    fn monitor_window_events(
        &self,
        _pid: u32,
        _cancel: tokio_util::sync::CancellationToken,
        _wake: std::sync::Arc<tokio::sync::Notify>,
    ) {
    }
    fn monitor_notifications(&self, _target: &WindowIdentity) -> SoftwareNotificationSnapshot {
        SoftwareNotificationSnapshot::Unavailable
    }
    fn present(
        &self,
        target: &WindowIdentity,
        mode: &str,
        viewport: Option<Viewport>,
    ) -> Result<WindowState, SoftwareError>;
    fn release(&self, target: &WindowIdentity) -> Result<(), SoftwareError>;
    fn write_wechat_draft(
        &self,
        _target: &WindowIdentity,
        _group: Option<&str>,
        _text: &str,
        _replace: bool,
        _cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), SoftwareError> {
        Err(SoftwareError::new(
            "wechat_draft_unavailable",
            "此宿主未提供微信草稿输入",
        ))
    }
    fn wechat_frame_png(&self, _target: &WindowIdentity) -> Result<Vec<u8>, SoftwareError> {
        Err(SoftwareError::new(
            "wechat_capture_unavailable",
            "当前宿主不支持微信窗口取图",
        ))
    }
    fn wechat_conversation(
        &self,
        _target: &WindowIdentity,
    ) -> Result<WechatConversation, SoftwareError> {
        Err(SoftwareError::new(
            "wechat_conversation_unavailable",
            "当前微信窗口未提供可读取的会话内容",
        ))
    }
    fn recognize_window(&self, _target: &WindowIdentity) -> Result<String, SoftwareError> {
        Err(SoftwareError::new(
            "window_ocr_unavailable",
            "当前宿主不支持窗口文字识别",
        ))
    }
    fn observe(
        &self,
        target: &WindowIdentity,
        observation_id: &str,
        read_content: bool,
    ) -> Result<Vec<SoftwareControl>, SoftwareError>;
    fn control(
        &self,
        target: &WindowIdentity,
        control: &SoftwareControl,
        action: &str,
        value: Option<&str>,
        cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<(), SoftwareError>;
    /// Restore mounted windows only. Never terminate user or launched processes.
    fn shutdown(&self) -> Result<(), SoftwareError>;
}
