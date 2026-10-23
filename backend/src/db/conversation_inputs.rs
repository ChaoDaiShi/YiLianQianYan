use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::agent::user_input::UserInputRequest;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ConversationInput {
    pub question_id: String,
    pub run_id: String,
    pub conversation_id: String,
    pub tool_call_id: String,
    #[serde(flatten)]
    pub request: UserInputRequest,
    pub status: String,
    pub revision: i64,
    pub answer: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Database, String, String) {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let conversation = db.create_conversation("Question test").unwrap().id;
        let turn = db
            .accept_conversation_turn(&super::super::MessageRow {
                id: "user-message".into(),
                conversation_id: conversation.clone(),
                role: "user".into(),
                content: "process request".into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: 1,
            })
            .unwrap();
        (db, conversation, turn)
    }
    #[test]
    fn answer_is_atomic_and_resumes_the_original_turn_only_once() {
        let (db, conversation, turn) = setup();
        let request = UserInputRequest {
            title: "输出语言？".into(),
            kind: "text".into(),
            options: vec![],
            allow_text: true,
        };
        let input = db
            .create_conversation_input(&conversation, "ask-call", &request)
            .unwrap();
        assert_eq!(input.run_id, turn);
        assert_eq!(
            db.pending_conversation_inputs(&conversation).unwrap().len(),
            1
        );
        let answer = AnswerInputRequest {
            revision: 1,
            option_id: None,
            text: Some("中文".into()),
        };
        db.answer_conversation_input(&input.question_id, &answer)
            .unwrap();
        assert_eq!(
            db.answer_conversation_input(&input.question_id, &answer)
                .unwrap_err(),
            "question_already_resolved"
        );
        assert!(db
            .pending_conversation_inputs(&conversation)
            .unwrap()
            .is_empty());
        let turns = db.conversation_turns(&conversation).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, turn);
        assert_eq!(turns[0].status, "running");
        let messages = db.load_chat_messages(&conversation).unwrap();
        assert_eq!(
            messages
                .iter()
                .filter(|m| m.tool_call_id.as_deref() == Some("ask-call"))
                .count(),
            1
        );
    }
    #[test]
    fn invalid_answers_leave_question_pending_and_cancel_pairs_tool_result() {
        let (db, conversation, _) = setup();
        let request:UserInputRequest=serde_json::from_value(serde_json::json!({"title":"选一个","kind":"single_choice","options":[{"id":"a","label":"A"},{"id":"b","label":"B"}]})).unwrap();
        let input = db
            .create_conversation_input(&conversation, "ask-choice", &request)
            .unwrap();
        assert!(db
            .answer_conversation_input(
                &input.question_id,
                &AnswerInputRequest {
                    revision: 1,
                    option_id: Some("c".into()),
                    text: None
                }
            )
            .is_err());
        assert_eq!(
            db.conversation_input(&input.question_id).unwrap().status,
            "pending"
        );
        assert!(db.cancel_conversation_inputs(&conversation).unwrap());
        assert_eq!(
            db.conversation_input(&input.question_id).unwrap().status,
            "cancelled"
        );
        assert_eq!(
            db.conversation_turns(&conversation).unwrap()[0].status,
            "cancelled"
        );
        assert!(db
            .pending_conversation_inputs(&conversation)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn input_options_must_be_unique_and_text_has_no_options() {
        let request:UserInputRequest=serde_json::from_value(serde_json::json!({"title":"请选择","kind":"single_choice","options":[{"id":"a","label":"A"},{"id":"a","label":"B"}]})).unwrap();
        assert!(request.validate().is_err());
        let mut text = request;
        text.kind = "text".into();
        assert!(text.validate().is_err());
    }
}
#[derive(Debug, Deserialize)]
pub struct AnswerInputRequest {
    pub revision: i64,
    pub option_id: Option<String>,
    pub text: Option<String>,
}
impl Database {
    pub fn initialize_conversation_inputs(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&ProductMigrationSpec::new(1053,"1053_conversation_inputs",MigrationOwner::V1TaskWorld,
        "CREATE TABLE conversation_inputs(id TEXT PRIMARY KEY,turn_id TEXT NOT NULL REFERENCES conversation_turns(id) ON DELETE CASCADE,conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,tool_call_id TEXT NOT NULL,request_json TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',revision INTEGER NOT NULL DEFAULT 1,answer TEXT,created_at INTEGER NOT NULL,resolved_at INTEGER,UNIQUE(turn_id,tool_call_id)); CREATE UNIQUE INDEX conversation_input_pending ON conversation_inputs(conversation_id) WHERE status='pending';"))
    }
    pub fn create_conversation_input(
        &self,
        conversation: &str,
        tool_call: &str,
        request: &UserInputRequest,
    ) -> Result<ConversationInput, String> {
        request.validate()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let turn:String=tx.query_row("SELECT id FROM conversation_turns WHERE conversation_id=?1 AND finished_at IS NULL ORDER BY started_at DESC,rowid DESC LIMIT 1",[conversation],|r|r.get(0)).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO conversation_inputs(id,turn_id,conversation_id,tool_call_id,request_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,turn,conversation,tool_call,serde_json::to_string(request).map_err(|e|e.to_string())?,now]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE conversation_turns SET status='waiting_input' WHERE id=?1",
            [&turn],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("UPDATE conversations SET run_status='waiting_input',run_error=NULL,run_finished_at=NULL,updated_at=?2 WHERE id=?1",params![conversation,now]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.conversation_input(&id)
    }
    pub fn conversation_input(&self, id: &str) -> Result<ConversationInput, String> {
        self.conn().query_row("SELECT id,turn_id,conversation_id,tool_call_id,request_json,status,revision,answer,created_at,resolved_at FROM conversation_inputs WHERE id=?1",[id],|r|{
            let json:String=r.get(4)?;
            let request=serde_json::from_str(&json).map_err(|e|rusqlite::Error::FromSqlConversionFailure(4,rusqlite::types::Type::Text,Box::new(e)))?;
            Ok(ConversationInput{question_id:r.get(0)?,run_id:r.get(1)?,conversation_id:r.get(2)?,tool_call_id:r.get(3)?,request,status:r.get(5)?,revision:r.get(6)?,answer:r.get(7)?,created_at:r.get(8)?,resolved_at:r.get(9)?})
        }).map_err(|e|e.to_string())
    }
    pub fn pending_conversation_inputs(
        &self,
        conversation: &str,
    ) -> Result<Vec<ConversationInput>, String> {
        let ids = {
            let conn = self.conn();
            let mut stmt=conn.prepare("SELECT id FROM conversation_inputs WHERE conversation_id=?1 AND status='pending' ORDER BY created_at").map_err(|e|e.to_string())?;
            let rows = stmt
                .query_map([conversation], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        ids.iter().map(|id| self.conversation_input(id)).collect()
    }
    /// Answer and paired tool result commit together. The same turn is resumed once.
    pub fn answer_conversation_input(
        &self,
        id: &str,
        answer: &AnswerInputRequest,
    ) -> Result<ConversationInput, String> {
        let input = self.conversation_input(id)?;
        let text = answer.text.as_deref().unwrap_or("").trim();
        if text.chars().count() > 10000 {
            return Err("invalid_answer".into());
        }
        match input.request.kind.as_str() {
            "text" if !text.is_empty() && answer.option_id.is_none() => {}
            "single_choice"
                if answer.option_id.as_ref().is_some_and(|id| {
                    input.request.options.iter().any(|option| &option.id == id)
                }) && (input.request.allow_text || text.is_empty()) => {}
            "single_choice"
                if input.request.allow_text && answer.option_id.is_none() && !text.is_empty() => {}
            _ => return Err("invalid_answer".into()),
        }
        let now = chrono::Utc::now().timestamp_millis();
        let payload = serde_json::json!({"option_id":answer.option_id,"text":text}).to_string();
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let changed=tx.execute("UPDATE conversation_inputs SET status='answered',revision=revision+1,answer=?1,resolved_at=?2 WHERE id=?3 AND revision=?4 AND status='pending' AND EXISTS(SELECT 1 FROM conversation_turns WHERE id=conversation_inputs.turn_id AND status='waiting_input' AND finished_at IS NULL)",params![payload,now,id,answer.revision]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("question_already_resolved".into());
        }
        tx.execute("INSERT INTO messages(id,conversation_id,role,content,tool_call_id,tool_name,tool_result,created_at) VALUES(?1,?2,'tool',?3,?4,'ask_user',?3,?5)",params![uuid::Uuid::new_v4().to_string(),input.conversation_id,payload,input.tool_call_id,now]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE conversation_turns SET status='running' WHERE id=?1",
            [&input.run_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("UPDATE conversations SET run_status='running',run_error=NULL,run_finished_at=NULL,updated_at=?2 WHERE id=?1",params![input.conversation_id,now]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        drop(conn);
        self.conversation_input(id)
    }
    pub fn cancel_conversation_inputs(&self, conversation: &str) -> Result<bool, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let pending:Option<(String,String)>=tx.query_row("SELECT id,tool_call_id FROM conversation_inputs WHERE conversation_id=?1 AND status='pending'",[conversation],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        let Some((id, tool_call)) = pending else {
            return Ok(false);
        };
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute("UPDATE conversation_inputs SET status='cancelled',revision=revision+1,resolved_at=?2 WHERE id=?1",params![id,now]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO messages(id,conversation_id,role,content,tool_call_id,tool_name,tool_result,created_at) VALUES(?1,?2,'tool','User cancelled the question; action not executed',?3,'ask_user','cancelled',?4)",params![uuid::Uuid::new_v4().to_string(),conversation,tool_call,now]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE conversation_turns SET status='cancelled',finished_at=?2 WHERE id=(SELECT turn_id FROM conversation_inputs WHERE id=?1)",params![id,now]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE conversations SET run_status='cancelled',run_finished_at=?2,updated_at=?2 WHERE id=?1",params![conversation,now]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(true)
    }
}
