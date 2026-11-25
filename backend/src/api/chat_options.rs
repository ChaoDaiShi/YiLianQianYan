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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
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
            let row = super::runtime::explicit_chat_model_profile(&server.db, id)?;
            runtime.config.model = row.to_model_config();
            runtime.model_row = Some(row);
            runtime.model_pool_id = None;
        }
        // Model selection must not undo the personal runtime's output cap,
        // including approval and answer continuation paths using this method.
        if let Some(id) = runtime
            .execution_scope
            .as_ref()
            .and_then(|scope| scope.owner.as_ref())
            .and_then(crate::execution::ExecutionOwner::personal_delegation_id)
        {
            if server.db.get_automation(id)?.is_some_and(|automation| matches!(automation.target,
                Some(crate::modules::automation::model::AutomationTarget::PersonalAssistant { .. }))) {
                runtime.config.model.max_tokens = runtime.config.model.max_tokens.min(2048);
            }
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
    pub(crate) fn last_user_message(
        &self,
        conversation: &str,
    ) -> Result<Option<(String, String)>, String> {
        self.conn().query_row(
            "SELECT id,substr(content,1,600) FROM messages WHERE conversation_id=?1 AND role='user' ORDER BY created_at DESC,rowid DESC LIMIT 1",
            [conversation], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(|error| error.to_string())
    }

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
        self.load_chat_history(conversation)
            .map(|(messages, _)| messages)
    }

    pub(crate) fn load_chat_history(
        &self,
        conversation: &str,
    ) -> Result<(Vec<ChatMessage>, Option<usize>), String> {
        if self.conversation_run_status(conversation)?.is_none() {
            return Err(format!("Conversation not found: {conversation}"));
        }
        let conn = self.conn();
        let rows = self.get_messages_internal(&conn, conversation)?;
        let user_id: Option<String> = conn
            .query_row(
                "SELECT user_message_id FROM conversation_turns WHERE conversation_id=?1
             ORDER BY started_at DESC,rowid DESC LIMIT 1",
                [conversation],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let turn_user_start =
            user_id.and_then(|id| rows.iter().position(|message| message.id == id));
        let messages = rows
            .into_iter()
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
            .collect::<Result<Vec<_>, String>>()?;
        Ok((messages, turn_user_start))
    }
    pub fn persist_chat_message(
        &self,
        conversation: &str,
        message: &ChatMessage,
        now: i64,
    ) -> Result<(), String> {
        let row = MessageRow::from_chat_message(conversation, message, now)?;
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
            agent_id: None,
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
