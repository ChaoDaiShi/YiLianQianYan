use std::{future::Future, sync::Arc};

use parking_lot::Mutex;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use super::{engine::AgentEvent, state::AgentState};
use crate::{
    db::{Database, MessageRow},
    execution::context_manifest::{
        load_conversation_manifest, save_conversation_manifest, ContextStatus,
    },
    integrations::llm::types::{ChatMessage, ToolCall},
    interaction::chat_understanding::explicit_read_only,
};

const MAX_SUPPLEMENTS: usize = 8;
const MAX_SUPPLEMENT_CHARS: usize = 4_000;

tokio::task_local! {
    pub(crate) static CURRENT_STEERING: Arc<Steering>;
}

#[derive(Default)]
struct Mailbox {
    messages: Vec<Supplement>,
    closed: bool,
    read_only: bool,
}

struct Supplement {
    row: MessageRow,
    applied_at: Option<usize>,
}

pub(crate) struct Steering {
    conversation: String,
    turn_id: String,
    cancellation: CancellationToken,
    mailbox: Mutex<Mailbox>,
}

pub(crate) enum SubmitError {
    InvalidText,
    Closed,
    LimitReached,
    Storage(String),
}

impl Steering {
    pub(crate) fn new(
        conversation: String,
        turn_id: String,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            conversation,
            turn_id,
            cancellation,
            mailbox: Mutex::default(),
        }
    }

    pub(crate) fn owns_conversation(&self, conversation: &str) -> bool {
        self.conversation == conversation
    }

    pub(crate) fn submit(
        &self,
        db: &Database,
        conversation: &str,
        turn_id: &str,
        text: &str,
    ) -> Result<String, SubmitError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_SUPPLEMENT_CHARS {
            return Err(SubmitError::InvalidText);
        }
        let mut mailbox = self.mailbox.lock();
        if mailbox.closed || turn_id != self.turn_id || self.cancellation.is_cancelled() {
            return Err(SubmitError::Closed);
        }
        if mailbox.messages.len() >= MAX_SUPPLEMENTS {
            return Err(SubmitError::LimitReached);
        }
        let row = MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.to_owned(),
            role: "user".to_owned(),
            content: text.to_owned(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        if !db
            .accept_turn_supplement(turn_id, &row)
            .map_err(SubmitError::Storage)?
        {
            return Err(SubmitError::Closed);
        }
        let id = row.id.clone();
        mailbox.messages.push(Supplement {
            row,
            applied_at: None,
        });
        Ok(id)
    }

    pub(crate) async fn run<F: Future>(self: &Arc<Self>, run: F) -> F::Output {
        let output = CURRENT_STEERING.scope(self.clone(), run).await;
        self.mailbox.lock().closed = true;
        output
    }

    pub(crate) fn pending(&self) -> bool {
        self.mailbox
            .lock()
            .messages
            .iter()
            .any(|message| message.applied_at.is_none())
    }

    /// A terminal branch may settle only if admission is closed at the same
    /// boundary where it checks that every accepted message was applied.
    pub(crate) fn close_if_settled(&self) -> bool {
        let mut mailbox = self.mailbox.lock();
        if mailbox
            .messages
            .iter()
            .any(|message| message.applied_at.is_none())
        {
            return false;
        }
        mailbox.closed = true;
        true
    }

    pub(crate) fn read_only(&self) -> bool {
        self.mailbox.lock().read_only
    }

    pub(crate) async fn apply(
        &self,
        state: &mut AgentState,
        tx: &Sender<AgentEvent>,
        conversation: &str,
    ) -> bool {
        let mut applied = Vec::new();
        {
            let mut mailbox = self.mailbox.lock();
            let mut read_only = mailbox.read_only;
            for message in mailbox
                .messages
                .iter_mut()
                .filter(|message| message.applied_at.is_none())
            {
                message.applied_at = Some(state.messages.len());
                state.add_user_message(message.row.content.clone());
                read_only |= explicit_read_only(&message.row.content);
                applied.push((message.row.id.clone(), message.row.content.clone()));
            }
            mailbox.read_only = read_only;
        }
        if applied.is_empty() {
            return false;
        }
        for (id, content) in applied {
            let mut event = AgentEvent::error_event(conversation, String::new());
            event.event_type = "steering_applied".into();
            event.error = None;
            event.message_id = Some(id);
            event.result = Some(content);
            let _ = tx.send(event).await;
        }
        true
    }

    pub(crate) fn persist(
        &self,
        db: &Database,
        conversation: &str,
        messages: &[ChatMessage],
        new_start: usize,
    ) -> Result<(), String> {
        let mailbox = self.mailbox.lock();
        let updates = mailbox
            .messages
            .iter()
            .map(|message| (message.applied_at, message.row.id.as_str()))
            .collect::<Vec<_>>();
        if updates.is_empty() {
            let now = chrono::Utc::now().timestamp_millis();
            for message in messages
                .iter()
                .skip(new_start)
                .filter(|message| matches!(message.role.as_str(), "assistant" | "tool"))
            {
                db.persist_chat_message(conversation, message, now)?;
            }
            return Ok(());
        }
        db.persist_steered_turn_messages(conversation, messages, new_start, &updates)?;
        if let Some(mut snapshot) = load_conversation_manifest(db, conversation)? {
            for message in &mailbox.messages {
                let (status, reason) = if message.applied_at.is_some() {
                    (ContextStatus::Resolved, "用户在执行中补充本轮要求")
                } else {
                    (ContextStatus::Excluded, "本轮结束前未能应用此补充")
                };
                snapshot.manifest.record(
                    "user_supplement",
                    &message.row.id,
                    None,
                    status,
                    0,
                    Some(reason),
                );
            }
            if mailbox.read_only {
                if let Some(route) = snapshot
                    .manifest
                    .entries
                    .iter_mut()
                    .find(|entry| entry.kind == "interaction_route")
                {
                    if route.source_id == "work" {
                        route.source_id = "read".into();
                    }
                    route.version = Some("standard".into());
                }
            }
            save_conversation_manifest(
                db,
                conversation,
                &self.turn_id,
                &snapshot.manifest,
                snapshot.timeline,
            )?;
        }
        Ok(())
    }
}

pub(crate) fn skip_tool(state: &mut AgentState, call: &ToolCall, reason: &str) {
    state.add_tool_result(
        call.id.clone(),
        call.function.name.clone(),
        serde_json::json!({
            "status": "not_executed",
            "reason": reason,
            "message": "用户补充了当前任务的要求，此工具未执行。"
        })
        .to_string(),
    );
}
