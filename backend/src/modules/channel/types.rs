use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(default = "http_mode")]
    pub connection_mode: String,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default = "manual_reply_mode")]
    pub reply_mode: String,
    pub enabled: bool,
    pub app_id: Option<String>,
    pub api_base_url: Option<String>,
    pub self_id: Option<String>,
    pub allowed_senders: Vec<String>,
    #[serde(default)]
    pub allowed_groups: Vec<String>,
    #[serde(default = "mention_trigger")]
    pub group_trigger: String,
    pub status: String,
    pub last_received_at: Option<i64>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_event: Option<ChannelEvent>,
    pub created_at: i64,
    pub updated_at: i64,
    pub callback_path: String,
    pub credentials_configured: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelSession {
    pub id: String,
    pub channel_id: String,
    pub peer_id: String,
    pub peer_type: String,
    pub conversation_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: String,
    pub channel_id: String,
    pub conversation_id: String,
    pub event_id: String,
    pub external_message_id: String,
    pub sender_id: String,
    pub peer_type: String,
    pub peer_id: String,
    pub content: String,
    pub draft_reply: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub reply_message_id: Option<String>,
    pub approved_at: Option<i64>,
    pub send_mode: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct InboundMessage {
    pub event_id: String,
    pub external_message_id: String,
    pub peer_id: String,
    pub peer_type: String,
    pub sender_id: String,
    pub content: String,
    pub context_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelInput {
    pub name: String,
    pub provider: String,
    #[serde(default = "http_mode")]
    pub connection_mode: String,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default = "manual_reply_mode")]
    pub reply_mode: String,
    pub enabled: bool,
    pub app_id: Option<String>,
    pub api_base_url: Option<String>,
    pub self_id: Option<String>,
    pub allowed_senders: Vec<String>,
    #[serde(default)]
    pub allowed_groups: Vec<String>,
    #[serde(default = "mention_trigger")]
    pub group_trigger: String,
    pub secrets: Option<ChannelSecrets>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ChannelSecrets {
    pub app_secret: Option<String>,
    pub verification_token: Option<String>,
    pub access_token: Option<String>,
    pub event_secret: Option<String>,
}

fn http_mode() -> String {
    "http".into()
}

fn manual_reply_mode() -> String {
    "manual".into()
}
fn mention_trigger() -> String {
    "mention".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelPreset {
    pub id: String,
    pub name: String,
    pub agent_id: Option<String>,
    pub model_profile_id: Option<String>,
    pub instructions: String,
    pub builtin: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelPresetInput {
    pub name: String,
    pub agent_id: Option<String>,
    pub model_profile_id: Option<String>,
    pub instructions: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ChannelLogin {
    pub status: String,
    pub qr_code_data_url: Option<String>,
    pub expires_at: Option<i64>,
    pub bot_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelEvent {
    pub at: i64,
    pub kind: String,
    pub sender_id: Option<String>,
    pub detail: String,
}
