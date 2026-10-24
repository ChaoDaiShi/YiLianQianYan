use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::channel::robot::types::{RobotAction, RobotMission, RobotPolicy, RobotRun};
use rusqlite::{params, OptionalExtension};
use serde::de::DeserializeOwned;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(1064, "1064_channel_robots", MigrationOwner::V1TaskWorld,
    "CREATE TABLE channel_robot_policies(channel_id TEXT PRIMARY KEY REFERENCES external_channels(id),payload TEXT NOT NULL);
     CREATE TABLE channel_robot_missions(id TEXT PRIMARY KEY,channel_id TEXT NOT NULL REFERENCES external_channels(id),payload TEXT NOT NULL);
     CREATE TABLE channel_robot_runs(id TEXT PRIMARY KEY,channel_id TEXT NOT NULL REFERENCES external_channels(id),conversation_id TEXT NOT NULL REFERENCES conversations(id),event_id TEXT,payload TEXT NOT NULL,UNIQUE(channel_id,event_id));
     CREATE INDEX robot_run_conversation ON channel_robot_runs(conversation_id);
     CREATE TABLE channel_robot_actions(id TEXT PRIMARY KEY,channel_id TEXT NOT NULL,run_id TEXT,payload TEXT NOT NULL);");

fn decode<T: DeserializeOwned>(payload: Option<String>) -> Result<Option<T>, String> {
    payload
        .map(|v| serde_json::from_str(&v).map_err(|e| e.to_string()))
        .transpose()
}
impl Database {
    pub fn initialize_channel_robots(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)
    }
    pub fn interrupt_running_robots(&self) -> Result<(), String> {
        let conn = self.conn();
        conn.execute("UPDATE channel_robot_runs SET payload=json_set(payload,'$.status','stopped','$.error','应用在执行期间退出，本次已中断，不会自动重跑','$.updated_at',?1) WHERE json_extract(payload,'$.status') IN ('queued','running','awaiting_approval')",[chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        conn.execute("UPDATE channel_robot_actions SET payload=json_set(payload,'$.status',CASE WHEN json_extract(payload,'$.tool_name') IN ('qq_send_private','qq_send_group','qq_owner_report','desktop_send') THEN 'delivery_unknown' ELSE 'failed' END,'$.error','应用在操作期间退出；未自动重试','$.updated_at',?1) WHERE json_extract(payload,'$.status') IN ('running','dispatching')",[chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn robot_policy(&self, channel: &str) -> Result<RobotPolicy, String> {
        let payload = self
            .conn()
            .query_row(
                "SELECT payload FROM channel_robot_policies WHERE channel_id=?1",
                [channel],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(decode(payload)?.unwrap_or_else(|| RobotPolicy::new(channel)))
    }
    pub fn save_robot_policy(&self, value: &RobotPolicy) -> Result<(), String> {
        value.validate()?;
        self.conn().execute("INSERT INTO channel_robot_policies(channel_id,payload) VALUES(?1,?2) ON CONFLICT(channel_id) DO UPDATE SET payload=excluded.payload", params![value.channel_id, serde_json::to_string(value).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    fn robot_values<T: DeserializeOwned>(&self, sql: &str, id: &str) -> Result<Vec<T>, String> {
        let conn = self.conn();
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let values = stmt
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        values
            .map(|r| {
                serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect()
    }
    pub fn robot_missions(&self, channel: &str) -> Result<Vec<RobotMission>, String> {
        self.robot_values(
            "SELECT payload FROM channel_robot_missions WHERE channel_id=?1 ORDER BY rowid DESC",
            channel,
        )
    }
    pub fn robot_mission(&self, id: &str) -> Result<Option<RobotMission>, String> {
        decode(
            self.conn()
                .query_row(
                    "SELECT payload FROM channel_robot_missions WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?,
        )
    }
    pub fn save_robot_mission(&self, value: &RobotMission) -> Result<(), String> {
        self.conn().execute("INSERT INTO channel_robot_missions(id,channel_id,payload) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![value.id,value.channel_id,serde_json::to_string(value).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn delete_robot_mission(&self, channel: &str, id: &str) -> Result<bool, String> {
        Ok(self
            .conn()
            .execute(
                "DELETE FROM channel_robot_missions WHERE id=?1 AND channel_id=?2",
                params![id, channel],
            )
            .map_err(|e| e.to_string())?
            > 0)
    }
    pub fn robot_runs(&self, channel: &str) -> Result<Vec<RobotRun>, String> {
        self.robot_values("SELECT payload FROM channel_robot_runs WHERE channel_id=?1 ORDER BY rowid DESC LIMIT 100", channel)
    }
    pub fn robot_run(&self, id: &str) -> Result<Option<RobotRun>, String> {
        decode(
            self.conn()
                .query_row(
                    "SELECT payload FROM channel_robot_runs WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?,
        )
    }
    pub fn robot_run_for_conversation(&self, id: &str) -> Result<Option<RobotRun>, String> {
        decode(self.conn().query_row("SELECT payload FROM channel_robot_runs WHERE conversation_id=?1 ORDER BY rowid DESC LIMIT 1",[id],|r|r.get(0)).optional().map_err(|e|e.to_string())?)
    }
    pub fn robot_run_for_event(
        &self,
        channel: &str,
        event: &str,
    ) -> Result<Option<RobotRun>, String> {
        decode(
            self.conn()
                .query_row(
                    "SELECT payload FROM channel_robot_runs WHERE channel_id=?1 AND event_id=?2",
                    params![channel, event],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?,
        )
    }
    pub fn create_robot_run(&self, value: &RobotRun, event_id: Option<&str>) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO channel_robot_runs(id,channel_id,conversation_id,event_id,payload) VALUES(?1,?2,?3,?4,?5)",params![value.id,value.channel_id,value.conversation_id,event_id,serde_json::to_string(value).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        if value.origin != "desktop" {
            tx.execute("INSERT OR REPLACE INTO settings(key,value) SELECT 'conversation_origin:'||?1,json_object('kind','channel','provider',json_extract(payload,'$.provider'),'channel_name',json_extract(payload,'$.name')) FROM external_channels WHERE id=?2",params![value.conversation_id,value.channel_id]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn save_robot_run(&self, value: &RobotRun) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE channel_robot_runs SET payload=?2 WHERE id=?1",
                params![
                    value.id,
                    serde_json::to_string(value).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn save_active_robot_run(&self, value: &RobotRun) -> Result<bool, String> {
        Ok(self.conn().execute("UPDATE channel_robot_runs SET payload=?2 WHERE id=?1 AND json_extract(payload,'$.status') IN ('queued','running','awaiting_approval')",params![value.id,serde_json::to_string(value).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())? > 0)
    }
    pub fn robot_actions(&self, run: &str) -> Result<Vec<RobotAction>, String> {
        self.robot_values(
            "SELECT payload FROM channel_robot_actions WHERE run_id=?1 ORDER BY rowid",
            run,
        )
    }
    pub fn robot_manual_sends(&self, channel: &str) -> Result<Vec<RobotAction>, String> {
        self.robot_values("SELECT payload FROM channel_robot_actions WHERE channel_id=?1 AND run_id IS NULL ORDER BY rowid DESC LIMIT 100",channel)
    }
    pub fn save_robot_action(&self, value: &RobotAction) -> Result<(), String> {
        self.conn().execute("INSERT INTO channel_robot_actions(id,channel_id,run_id,payload) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![value.id,value.channel_id,value.run_id,serde_json::to_string(value).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn claim_robot_action(&self, value: &RobotAction) -> Result<bool, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let (account,enabled):(Option<String>,bool)=tx.query_row("SELECT json_extract(payload,'$.self_id'),json_extract(payload,'$.enabled') FROM external_channels WHERE id=?1",[&value.channel_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
        if !enabled || account.as_deref() != Some(value.account_id.as_str()) {
            return Err("渠道已停用或发送账号已变更，未开始本次操作".into());
        }
        if let Some(id) = value.run_id.as_deref() {
            let run: RobotRun = serde_json::from_str(
                &tx.query_row(
                    "SELECT payload FROM channel_robot_runs WHERE id=?1",
                    [id],
                    |r| r.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if run.channel_id != value.channel_id
                || (!run.active() && value.tool_name != "qq_owner_report")
            {
                return Err("机器人运行已停止，未开始下一条操作".into());
            }
            let policy: RobotPolicy = serde_json::from_str(
                &tx.query_row(
                    "SELECT payload FROM channel_robot_policies WHERE channel_id=?1",
                    [&run.channel_id],
                    |r| r.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if !enabled
                || account.as_deref() != Some(policy.account_id.as_str())
                || account.as_deref() != Some(run.account_id.as_str())
            {
                return Err("账号已变更或渠道已停用，请重新确认操作范围".into());
            }
            if !policy.enabled
                || (run.origin == "owner"
                    && !run
                        .owner_id
                        .as_ref()
                        .is_some_and(|id| policy.owner_ids.contains(id)))
            {
                return Err("机器人或主人授权已撤销".into());
            }
            if value.tool_name.starts_with("qq_")
                && !matches!(
                    value.tool_name.as_str(),
                    "qq_send_private" | "qq_send_group" | "qq_owner_report"
                )
                && !policy.allows_tool_read(&value.tool_name)
            {
                return Err("读取范围授权已撤销，未读取平台数据".into());
            }
            if matches!(
                value.tool_name.as_str(),
                "qq_send_private" | "qq_send_group"
            ) && !policy.allows_send(
                value.peer_type.as_deref().unwrap_or_default(),
                value.peer_id.as_deref().unwrap_or_default(),
            ) && value
                .arguments
                .get("_gateway_one_shot_approval")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
            {
                return Err("自动发言目标授权已撤销，未发送".into());
            }
            if value.tool_name == "qq_owner_report"
                && (run.status != "completed"
                    || run.owner_id != value.peer_id
                    || value.peer_type.as_deref() != Some("private"))
            {
                return Err("主人报告与实际运行不匹配".into());
            }
        }
        let changed=tx.execute("INSERT INTO channel_robot_actions(id,channel_id,run_id,payload) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO NOTHING",params![value.id,value.channel_id,value.run_id,serde_json::to_string(value).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(changed > 0)
    }
    pub fn robot_ambiguous_send(&self, run: &str, kind: &str, peer: &str) -> Result<bool, String> {
        Ok(self.robot_actions(run)?.iter().any(|a| {
            matches!(a.tool_name.as_str(), "qq_send_private" | "qq_send_group")
                && a.peer_type.as_deref() == Some(kind)
                && a.peer_id.as_deref() == Some(peer)
                && matches!(
                    a.status.as_str(),
                    "delivery_unknown" | "dispatching" | "running"
                )
        }))
    }
    pub fn robot_action(&self, id: &str) -> Result<Option<RobotAction>, String> {
        decode(
            self.conn()
                .query_row(
                    "SELECT payload FROM channel_robot_actions WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?,
        )
    }
    pub fn interrupt_robot_actions(&self, run: &str) -> Result<(), String> {
        self.conn().execute("UPDATE channel_robot_actions SET payload=json_set(payload,'$.status',CASE WHEN json_extract(payload,'$.tool_name') IN ('qq_send_private','qq_send_group','qq_owner_report','desktop_send') THEN 'delivery_unknown' ELSE 'failed' END,'$.error','运行已停止；进行中的发送未取得最终回执，不会自动重发','$.updated_at',?2) WHERE run_id=?1 AND json_extract(payload,'$.status') IN ('running','dispatching')",params![run,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_robot_policy_defaults_do_not_authorize_any_recipient() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let policy = db.robot_policy("new-channel").unwrap();
        assert!(!policy.enabled);
        assert!(policy.read_friends && policy.read_groups);
        assert!(!policy.allows_send("private", "123"));
        let mut policy = policy;
        policy.enabled = true;
        policy.send_private_ids.push("123".into());
        assert!(policy.allows_send("private", "123"));
        assert!(!policy.allows_send("group", "123"));
        assert!(!policy.allows_send("private", "456"));
        policy.send_mode = "manual".into();
        assert!(!policy.allows_send("private", "123"));
    }
}
