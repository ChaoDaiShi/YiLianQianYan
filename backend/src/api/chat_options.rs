//! User-selected per-turn model and thinking settings, persisted for approval continuation.
use crate::{
    app::state::AppServer,
    db::{Database, MessageRow},
    integrations::llm::{client::LlmClient, thinking::ThinkingMode, types::ChatMessage},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChatRequestOptions {
    #[serde(default)]
    pub workspace_mode: Option<crate::db::conversation_modes::WorkspaceMode>,
    #[serde(default)]
    pub model_profile_id: Option<String>,
    #[serde(default)]
    pub thinking_mode: ThinkingMode,
}
impl ChatRequestOptions {
    pub fn apply(
        &self,
        server: &AppServer,
        runtime: &mut super::runtime::ChatRuntime,
    ) -> Result<(), String> {
        if let Some(id) = &self.model_profile_id {
            let row = server.db.get_llm_model(id)?.ok_or("所选模型档案已删除")?;
            if row.api_format != "openai" {
                return Err("所选模型不支持对话协议".into());
            }
            runtime.config.model = row.to_model_config();
            runtime.model_row = Some(row);
            runtime.model_pool_id = None;
        }
        if let Some(pool_id) = &runtime.model_pool_id {
            let pool = server
                .db
                .get_model_pool(pool_id)?
                .ok_or("所选模型池已删除")?;
            for id in pool.tiers.iter().flat_map(|tier| &tier.profile_ids) {
                let row = server.db.get_llm_model(id)?.ok_or("模型池候选档案已删除")?;
                self.thinking_mode.validate(&row.to_model_config())?;
            }
        } else {
            self.thinking_mode.validate(&runtime.config.model)?;
        }
        Ok(())
    }
    pub fn client(&self, client: LlmClient) -> LlmClient {
        client.with_thinking_mode(self.thinking_mode)
    }
}
impl Database {
    pub fn save_chat_options(
        &self,
        conversation: &str,
        options: &ChatRequestOptions,
    ) -> Result<(), String> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES(?1,?2)",
                params![
                    format!("chat_options:{conversation}"),
                    serde_json::to_string(options).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn load_chat_options(&self, conversation: &str) -> Result<ChatRequestOptions, String> {
        let value: Option<String> = self
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                params![format!("chat_options:{conversation}")],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match value {
            Some(value) => serde_json::from_str(&value).map_err(|e| e.to_string()),
            None => Ok(ChatRequestOptions::default()),
        }
    }
    pub fn load_chat_messages(&self, conversation: &str) -> Result<Vec<ChatMessage>, String> {
        let rows = self.get_conversation(conversation)?.messages;
        let conn = self.conn();
        rows.into_iter()
            .map(|m| {
                let reasoning: Option<String> = conn
                    .query_row(
                        "SELECT value FROM settings WHERE key=?1",
                        params![format!("chat_reasoning:{}", m.id)],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                Ok(ChatMessage {
                    role: m.role,
                    content: (!m.content.is_empty()).then_some(m.content),
                    tool_calls: m
                        .tool_calls
                        .map(|v| serde_json::from_str(&v))
                        .transpose()
                        .map_err(|e| e.to_string())?,
                    tool_call_id: m.tool_call_id,
                    name: m.tool_name,
                    reasoning_content: reasoning,
                })
            })
            .collect()
    }
    pub fn persist_chat_message(
        &self,
        conversation: &str,
        message: &ChatMessage,
        now: i64,
    ) -> Result<(), String> {
        let row = MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.into(),
            role: message.role.clone(),
            content: message.content.clone().unwrap_or_default(),
            tool_calls: message
                .tool_calls
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| e.to_string())?,
            tool_call_id: message.tool_call_id.clone(),
            tool_name: message.name.clone(),
            tool_result: None,
            created_at: now,
        };
        self.add_chat_message_with_reasoning(&row, message.reasoning_content.as_deref())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_options_and_reasoning_roundtrip_for_approval_resume() {
        let root = std::env::temp_dir().join(format!("chat-options-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Database::new(&root.join("options.db")).unwrap();
        let conversation = db.create_conversation("thinking").unwrap();
        let options = ChatRequestOptions {
            workspace_mode: None,
            model_profile_id: Some("chosen-profile".into()),
            thinking_mode: ThinkingMode::Thinking,
        };
        db.save_chat_options(&conversation.id, &options).unwrap();
        let saved = db.load_chat_options(&conversation.id).unwrap();
        assert_eq!(saved.model_profile_id, options.model_profile_id);
        assert_eq!(saved.thinking_mode, ThinkingMode::Thinking);
        let mut assistant = ChatMessage::user("visible answer");
        assistant.role = "assistant".into();
        assistant.reasoning_content = Some("provider reasoning".into());
        db.persist_chat_message(&conversation.id, &assistant, 1)
            .unwrap();
        let history = db.load_chat_messages(&conversation.id).unwrap();
        let detail = db.get_conversation(&conversation.id).unwrap();
        assert!(detail
            .message_reasoning
            .values()
            .any(|value| value == "provider reasoning"));
        assert_eq!(
            history[0].reasoning_content.as_deref(),
            Some("provider reasoning")
        );
        assert_eq!(history[0].content.as_deref(), Some("visible answer"));
        db.delete_conversation(&conversation.id).unwrap();
        let remaining:i64=db.conn().query_row("SELECT COUNT(*) FROM settings WHERE key LIKE 'chat_options:%' OR key LIKE 'chat_reasoning:%'",[],|row|row.get(0)).unwrap();
        assert_eq!(remaining, 0);
    }
}
