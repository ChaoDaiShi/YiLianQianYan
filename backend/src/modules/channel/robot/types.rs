use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobotPolicy {
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub account_id: String,
    pub enabled: bool,
    pub owner_ids: Vec<String>,
    pub read_friends: bool,
    pub read_groups: bool,
    pub send_mode: String,
    pub send_private_ids: Vec<String>,
    pub send_group_ids: Vec<String>,
    pub send_all_private: bool,
    pub send_all_groups: bool,
    #[serde(default)]
    pub updated_at: i64,
}
impl RobotPolicy {
    pub fn new(channel_id: &str) -> Self {
        Self {
            channel_id: channel_id.into(),
            account_id: String::new(),
            enabled: false,
            owner_ids: vec![],
            read_friends: true,
            read_groups: true,
            send_mode: "automatic".into(),
            send_private_ids: vec![],
            send_group_ids: vec![],
            send_all_private: false,
            send_all_groups: false,
            updated_at: 0,
        }
    }
    pub fn allows_send(&self, kind: &str, id: &str) -> bool {
        self.enabled
            && self.send_mode == "automatic"
            && match kind {
                "private" => self.send_all_private || self.send_private_ids.iter().any(|v| v == id),
                "group" => self.send_all_groups || self.send_group_ids.iter().any(|v| v == id),
                _ => false,
            }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.send_mode.as_str(), "automatic" | "manual") {
            return Err("发送模式须为 automatic 或 manual".into());
        }
        for id in self
            .owner_ids
            .iter()
            .chain(&self.send_private_ids)
            .chain(&self.send_group_ids)
        {
            if id.parse::<i64>().ok().filter(|v| *v > 0).is_none() {
                return Err("QQ 联系人和群 ID 必须为正整数".into());
            }
        }
        Ok(())
    }
    pub fn allows_tool_read(&self, name: &str) -> bool {
        match name {
            "qq_account" => true,
            "qq_friends" | "qq_private_history" => self.read_friends,
            "qq_groups" | "qq_group_info" | "qq_group_members" | "qq_group_history" => {
                self.read_groups
            }
            "qq_recent" | "qq_message" => self.read_friends && self.read_groups,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RobotMission {
    pub id: String,
    pub channel_id: String,
    pub name: String,
    pub goal: String,
    pub preset_id: Option<String>,
    pub workspace_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobotMissionInput {
    pub name: String,
    pub goal: String,
    pub preset_id: Option<String>,
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RobotRun {
    pub id: String,
    pub account_id: String,
    pub channel_id: String,
    pub mission_id: Option<String>,
    pub conversation_id: String,
    pub user_message_id: Option<String>,
    pub origin: String,
    pub owner_id: Option<String>,
    pub goal: String,
    pub preset_id: Option<String>,
    pub status: String,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl RobotRun {
    pub fn active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "queued" | "running" | "awaiting_approval"
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RobotAction {
    pub id: String,
    pub account_id: String,
    pub run_id: Option<String>,
    pub channel_id: String,
    pub tool_name: String,
    pub peer_type: Option<String>,
    pub peer_id: Option<String>,
    pub arguments: Value,
    pub result: Option<Value>,
    pub status: String,
    pub platform_message_id: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct RobotContact {
    pub id: String,
    pub name: String,
    pub peer_type: String,
}
