use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

use crate::{
    agent::steering::{Steering, SubmitError},
    app::state::AppServer,
    db::Database,
    integrations::llm::types::ChatMessage,
};

#[derive(Deserialize)]
pub struct Input {
    conversation_id: String,
    turn_id: String,
    message: String,
}

#[derive(Serialize)]
pub struct Reply {
    turn_id: String,
    message_id: String,
}

pub(in crate::api) struct RegisteredSteering {
    server: Arc<AppServer>,
    conversation: String,
    mailbox: Arc<Steering>,
}

impl RegisteredSteering {
    pub(in crate::api) async fn run<F: Future>(&self, run: F) -> F::Output {
        self.mailbox.run(run).await
    }

    pub(in crate::api) fn persist(
        &self,
        db: &Database,
        conversation: &str,
        messages: &[ChatMessage],
        new_start: usize,
    ) -> Result<(), String> {
        self.mailbox.persist(db, conversation, messages, new_start)
    }
}

pub(in crate::api) async fn register_continuation(
    server: &Arc<AppServer>,
    conversation: &str,
    cancellation: CancellationToken,
    tx: &tokio::sync::mpsc::Sender<crate::agent::engine::AgentEvent>,
) -> Result<Option<RegisteredSteering>, String> {
    if !super::runtime::ordinary_conversation(server, conversation)? || cancellation.is_cancelled()
    {
        return Ok(None);
    }
    let Some(turn_id) = super::super::chat_stop::latest(&server.db, conversation)? else {
        return Ok(None);
    };
    let registered = register(server, conversation, &turn_id, cancellation);
    let mut event = crate::agent::engine::AgentEvent::error_event(conversation, String::new());
    event.event_type = "connected".into();
    event.error = None;
    event.args = Some(serde_json::json!({"turn_id":turn_id,"accepts_steering":true}));
    let _ = tx.send(event).await;
    Ok(Some(registered))
}

impl Drop for RegisteredSteering {
    fn drop(&mut self) {
        self.server.chat_steering.lock().remove(&self.conversation);
    }
}

pub(super) fn register(
    server: &Arc<AppServer>,
    conversation: &str,
    turn_id: &str,
    cancellation: CancellationToken,
) -> RegisteredSteering {
    let mailbox = Arc::new(Steering::new(
        conversation.to_owned(),
        turn_id.to_owned(),
        cancellation,
    ));
    server
        .chat_steering
        .lock()
        .insert(conversation.to_owned(), mailbox.clone());
    RegisteredSteering {
        server: server.clone(),
        conversation: conversation.to_owned(),
        mailbox,
    }
}

pub async fn handle(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<Input>,
) -> Result<Json<Reply>, super::ChatApiError> {
    let closed = || {
        super::chat_error(
            StatusCode::CONFLICT,
            "chat_steering_closed",
            "该轮已经结束或正在等待审批或回答，补充内容仍留在输入框。",
        )
    };
    let mailbox = server
        .chat_steering
        .lock()
        .get(&input.conversation_id)
        .cloned()
        .ok_or_else(closed)?;
    let message_id = mailbox
        .submit(
            &server.db,
            &input.conversation_id,
            &input.turn_id,
            &input.message,
        )
        .map_err(|error| match error {
            SubmitError::InvalidText => super::chat_error(
                StatusCode::BAD_REQUEST,
                "invalid_steering_text",
                "补充内容须为 1 至 4000 个字符。",
            ),
            SubmitError::Closed => closed(),
            SubmitError::LimitReached => super::chat_error(
                StatusCode::CONFLICT,
                "chat_steering_limit",
                "当前轮次已收到 8 条补充，请等待处理。",
            ),
            SubmitError::Storage(error) => super::chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chat_steering_write_failed",
                error,
            ),
        })?;
    Ok(Json(Reply {
        turn_id: input.turn_id,
        message_id,
    }))
}
