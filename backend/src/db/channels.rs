use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::channel::{Channel, ChannelMessage, ChannelSession, InboundMessage};
use rusqlite::{params, OptionalExtension};
mod preset_storage;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(1056,"1056_external_channels",MigrationOwner::V1TaskWorld,
    "CREATE TABLE external_channels(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
     CREATE TABLE channel_sessions(id TEXT PRIMARY KEY,channel_id TEXT NOT NULL REFERENCES external_channels(id),peer_id TEXT NOT NULL,conversation_id TEXT NOT NULL REFERENCES conversations(id),created_at INTEGER NOT NULL,UNIQUE(channel_id,peer_id));
     CREATE TABLE channel_messages(id TEXT PRIMARY KEY,channel_id TEXT NOT NULL REFERENCES external_channels(id),conversation_id TEXT NOT NULL REFERENCES conversations(id),event_id TEXT NOT NULL,external_message_id TEXT NOT NULL,sender_id TEXT NOT NULL,content TEXT NOT NULL,draft_reply TEXT,status TEXT NOT NULL,error TEXT,reply_message_id TEXT,approved_at INTEGER,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(channel_id,event_id));
     CREATE INDEX channel_messages_channel ON channel_messages(channel_id,created_at);
     CREATE INDEX channel_sessions_conversation ON channel_sessions(conversation_id);");
const AUTOMATIC_REPLIES:ProductMigrationSpec=ProductMigrationSpec::new(1060,"1060_channel_automatic_replies",MigrationOwner::V1TaskWorld,"ALTER TABLE channel_messages ADD COLUMN send_mode TEXT; UPDATE channel_messages SET send_mode='manual' WHERE approved_at IS NOT NULL;");
const MESSAGE_COLUMNS: &str = "id,channel_id,conversation_id,event_id,external_message_id,sender_id,content,draft_reply,status,error,reply_message_id,approved_at,created_at,updated_at,send_mode";
fn message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelMessage> {
    Ok(ChannelMessage {
        id: row.get(0)?,
        channel_id: row.get(1)?,
        conversation_id: row.get(2)?,
        event_id: row.get(3)?,
        external_message_id: row.get(4)?,
        sender_id: row.get(5)?,
        content: row.get(6)?,
        draft_reply: row.get(7)?,
        status: row.get(8)?,
        error: row.get(9)?,
        reply_message_id: row.get(10)?,
        approved_at: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        send_mode: row.get(14)?,
    })
}
impl Database {
    pub fn initialize_channels(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)?;
        self.initialize_channel_presets()?;
        self.apply_product_migration(&AUTOMATIC_REPLIES)
    }
    pub fn channels(&self) -> Result<Vec<Channel>, String> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT payload FROM external_channels ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn channel(&self, id: &str) -> Result<Option<Channel>, String> {
        let value: Option<String> = self
            .conn()
            .query_row(
                "SELECT payload FROM external_channels WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        value
            .map(|value| serde_json::from_str(&value).map_err(|e| e.to_string()))
            .transpose()
    }
    pub fn save_channel(&self, channel: &Channel) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        if let Some(preset) = channel.preset_id.as_deref() {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM channel_presets WHERE id=?1)",
                    [preset],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !exists {
                return Err("渠道回复预设不存在".into());
            }
        }
        tx.execute("INSERT INTO external_channels(id,payload) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![channel.id,serde_json::to_string(channel).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn channel_received(&self, id: &str) -> Result<(), String> {
        self.conn().execute("UPDATE external_channels SET payload=json_set(payload,'$.status','receiving','$.last_received_at',?2,'$.last_error',NULL,'$.updated_at',?2) WHERE id=?1",params![id,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn channel_for_conversation(&self, conversation: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(
                "SELECT channel_id FROM channel_sessions WHERE conversation_id=?1",
                [conversation],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn channel_sessions(&self, channel: &str) -> Result<Vec<ChannelSession>, String> {
        let conn = self.conn();
        let mut statement=conn.prepare("SELECT id,channel_id,peer_id,conversation_id,created_at FROM channel_sessions WHERE channel_id=?1 ORDER BY created_at DESC").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([channel], |r| {
                Ok(ChannelSession {
                    id: r.get(0)?,
                    channel_id: r.get(1)?,
                    peer_id: r.get(2)?,
                    conversation_id: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }
    pub fn accept_channel_message(
        &self,
        channel: &Channel,
        inbound: &InboundMessage,
    ) -> Result<(ChannelMessage, bool), String> {
        self.accept_channel_message_with_id(channel, inbound, uuid::Uuid::new_v4().to_string())
    }
    pub fn channel_message_for_event(
        &self,
        channel: &str,
        event: &str,
    ) -> Result<Option<ChannelMessage>, String> {
        self.conn().query_row(&format!("SELECT {MESSAGE_COLUMNS} FROM channel_messages WHERE channel_id=?1 AND event_id=?2"),params![channel,event],message_row).optional().map_err(|e|e.to_string())
    }
    pub fn accept_channel_message_with_id(
        &self,
        channel: &Channel,
        inbound: &InboundMessage,
        message_id: String,
    ) -> Result<(ChannelMessage, bool), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let existing=tx.query_row(&format!("SELECT {MESSAGE_COLUMNS} FROM channel_messages WHERE channel_id=?1 AND event_id=?2"),params![channel.id,inbound.event_id],message_row).optional().map_err(|e|e.to_string())?;
        if let Some(existing) = existing {
            return Ok((existing, false));
        }
        let now = chrono::Utc::now().timestamp_millis();
        let conversation: Option<String> = tx
            .query_row(
                "SELECT conversation_id FROM channel_sessions WHERE channel_id=?1 AND peer_id=?2",
                params![channel.id, inbound.peer_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let conversation = if let Some(conversation) = conversation {
            conversation
        } else {
            let conversation = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO conversations(id,title,created_at,updated_at) VALUES(?1,?2,?3,?3)",
                params![
                    conversation,
                    format!("{} · {}", channel.name, inbound.sender_id),
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.execute("INSERT INTO channel_sessions(id,channel_id,peer_id,conversation_id,created_at) VALUES(?1,?2,?3,?4,?5)",params![uuid::Uuid::new_v4().to_string(),channel.id,inbound.peer_id,conversation,now]).map_err(|e|e.to_string())?;
            conversation
        };
        let message = ChannelMessage {
            id: message_id,
            channel_id: channel.id.clone(),
            conversation_id: conversation,
            event_id: inbound.event_id.clone(),
            external_message_id: inbound.external_message_id.clone(),
            sender_id: inbound.sender_id.clone(),
            content: inbound.content.clone(),
            draft_reply: None,
            status: "queued".into(),
            error: None,
            reply_message_id: None,
            approved_at: None,
            send_mode: Some(channel.reply_mode.clone()),
            created_at: now,
            updated_at: now,
        };
        tx.execute("INSERT INTO channel_messages(id,channel_id,conversation_id,event_id,external_message_id,sender_id,content,status,created_at,updated_at,send_mode) VALUES(?1,?2,?3,?4,?5,?6,?7,'queued',?8,?8,?9)",params![message.id,channel.id,message.conversation_id,message.event_id,message.external_message_id,message.sender_id,message.content,now,message.send_mode]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE external_channels SET payload=json_set(payload,'$.status','receiving','$.last_received_at',?2,'$.last_error',NULL,'$.updated_at',?2) WHERE id=?1",params![channel.id,now]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok((message, true))
    }
    pub fn channel_messages(&self, channel: &str) -> Result<Vec<ChannelMessage>, String> {
        let conn = self.conn();
        let mut statement=conn.prepare(&format!("SELECT {MESSAGE_COLUMNS} FROM channel_messages WHERE channel_id=?1 ORDER BY created_at DESC,id LIMIT 100")).map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([channel], message_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }
    pub fn channel_message(
        &self,
        channel: &str,
        id: &str,
    ) -> Result<Option<ChannelMessage>, String> {
        self.conn()
            .query_row(
                &format!(
                    "SELECT {MESSAGE_COLUMNS} FROM channel_messages WHERE channel_id=?1 AND id=?2"
                ),
                params![channel, id],
                message_row,
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn set_channel_message_result(
        &self,
        id: &str,
        status: &str,
        draft: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE channel_messages SET status=?2,draft_reply=?3,error=?4,updated_at=?5 WHERE id=?1 AND status IN ('queued','running')",params![id,status,draft,error,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn approve_channel_send(
        &self,
        channel: &str,
        id: &str,
        text: &str,
    ) -> Result<bool, String> {
        Ok(self.conn().execute("UPDATE channel_messages SET status='sending',draft_reply=?3,approved_at=?4,updated_at=?4,error=NULL,send_mode='manual' WHERE channel_id=?1 AND id=?2 AND status IN ('awaiting_send','send_failed')",params![channel,id,text,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?>0)
    }
    pub fn claim_automatic_channel_send(
        &self,
        channel: &str,
        id: &str,
        text: &str,
    ) -> Result<bool, String> {
        Ok(self.conn().execute("UPDATE channel_messages SET status='sending',draft_reply=?3,approved_at=NULL,updated_at=?4,error=NULL,send_mode='automatic' WHERE channel_id=?1 AND id=?2 AND status='awaiting_send' AND send_mode='automatic' AND EXISTS(SELECT 1 FROM external_channels WHERE id=?1 AND json_extract(payload,'$.enabled')=1 AND json_extract(payload,'$.reply_mode')='automatic')",params![channel,id,text,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?>0)
    }
    pub fn fail_automatic_channel_send(
        &self,
        channel: &str,
        id: &str,
        reason: &str,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE channel_messages SET status='send_failed',error=?3,updated_at=?4 WHERE channel_id=?1 AND id=?2 AND status='awaiting_send' AND send_mode='automatic'",params![channel,id,reason,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn finish_channel_send(
        &self,
        id: &str,
        remote: Option<&str>,
        error: Option<&str>,
        may_have_sent: bool,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE channel_messages SET status=?2,reply_message_id=?3,error=?4,updated_at=?5 WHERE id=?1 AND status='sending'",params![id,if remote.is_some(){"sent"}else if may_have_sent{"delivery_unknown"}else{"send_failed"},remote,error,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn interrupt_channel_runs(&self) -> Result<(), String> {
        self.conn().execute("UPDATE channel_messages SET status=CASE WHEN status='sending' THEN 'delivery_unknown' ELSE 'failed' END,error='应用在处理期间退出；未自动重发',updated_at=?1 WHERE status IN ('queued','running','sending')",[chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn set_channel_status(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE external_channels SET payload=json_set(payload,'$.status',?2,'$.last_error',?3,'$.updated_at',?4) WHERE id=?1",params![id,status,error,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn record_channel_event(
        &self,
        id: &str,
        event: &crate::modules::channel::ChannelEvent,
    ) -> Result<(), String> {
        self.conn().execute("UPDATE external_channels SET payload=json_set(payload,'$.last_event',json(?2)) WHERE id=?1",params![id,serde_json::to_string(event).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn channel_message_ids(&self, id: &str) -> Result<Vec<String>, String> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT id FROM channel_messages WHERE channel_id=?1")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Login owns only platform identity and credentials metadata. It must not
    /// overwrite a concurrently changed delivery policy or enabled state.
    pub fn complete_wechat_login(
        &self,
        id: &str,
        bot: &str,
        base: &str,
        scanned_user: Option<&str>,
    ) -> Result<Channel, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let changed=tx.execute("UPDATE external_channels SET payload=json_set(payload,'$.self_id',?2,'$.api_base_url',?3,'$.credentials_configured',json('true'),'$.allowed_senders',json(CASE WHEN json_array_length(json_extract(payload,'$.allowed_senders'))=0 AND ?4 IS NOT NULL THEN json_array(?4) ELSE json_extract(payload,'$.allowed_senders') END),'$.updated_at',?5) WHERE id=?1",params![id,bot,base,scanned_user,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        if changed == 0 {
            return Err("微信渠道配置已删除".into());
        }
        let payload: String = tx
            .query_row(
                "SELECT payload FROM external_channels WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let channel = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(channel)
    }

    pub fn set_channel_reply_mode(&self, id: &str, mode: &str) -> Result<(), String> {
        self.conn().execute("UPDATE external_channels SET payload=json_set(payload,'$.reply_mode',?2,'$.updated_at',?3) WHERE id=?1",params![id,mode,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn channel_is_sending(&self, id: &str) -> Result<bool, String> {
        self.conn().query_row("SELECT EXISTS(SELECT 1 FROM channel_messages WHERE channel_id=?1 AND status='sending')",[id],|r|r.get(0)).map_err(|e|e.to_string())
    }

    pub fn channel_state(&self, id: &str) -> Result<Option<serde_json::Value>, String> {
        let raw: Option<String> = self
            .conn()
            .query_row(
                "SELECT payload FROM channel_runtime_state WHERE channel_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        raw.map(|value| serde_json::from_str(&value).map_err(|e| e.to_string()))
            .transpose()
    }

    pub fn save_channel_state(&self, id: &str, state: &serde_json::Value) -> Result<(), String> {
        self.conn().execute("INSERT INTO channel_runtime_state(channel_id,payload) VALUES(?1,?2) ON CONFLICT(channel_id) DO UPDATE SET payload=excluded.payload",params![id,serde_json::to_string(state).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }

    /// Local chat history stays available after the user removes an external channel.
    pub fn delete_channel(&self, id: &str) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM channel_messages WHERE channel_id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM channel_sessions WHERE channel_id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM channel_runtime_state WHERE channel_id=?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM external_channels WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn regenerate_channel_message(&self, channel: &str, message: &str) -> Result<bool, String> {
        Ok(self.conn().execute("UPDATE channel_messages SET status='queued',draft_reply=NULL,error=NULL,updated_at=?3 WHERE channel_id=?1 AND id=?2 AND status IN ('failed','awaiting_send','send_failed','dismissed')",params![channel,message,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())? > 0)
    }

    pub fn dismiss_channel_message(&self, channel: &str, message: &str) -> Result<bool, String> {
        Ok(self.conn().execute("UPDATE channel_messages SET status='dismissed',updated_at=?3 WHERE channel_id=?1 AND id=?2 AND status IN ('failed','awaiting_send','send_failed')",params![channel,message,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())? > 0)
    }

    pub fn interrupt_channel(&self, channel: &str) -> Result<(), String> {
        self.conn().execute("UPDATE channel_messages SET status='failed',error='渠道已停用，处理已取消',updated_at=?2 WHERE channel_id=?1 AND status IN ('queued','running')",params![channel,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn bind_channel_agent(
        &self,
        conversation: &str,
        agent: Option<&str>,
    ) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let source = format!("conversation:{conversation}");
        tx.execute(
            "DELETE FROM connector_bindings WHERE kind='conversation_agent' AND source_id=?1",
            [&source],
        )
        .map_err(|e| e.to_string())?;
        if let Some(agent) = agent {
            tx.execute("INSERT INTO connector_bindings(id,agent_id,kind,source_id,target_id,service_kind,model_source) VALUES(?1,?2,'conversation_agent',?3,?4,'','')",params![format!("channel-agent:{conversation}"),agent,source,format!("agent:{agent}")]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
}
