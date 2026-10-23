use crate::api::chat::lifecycle::{acquire_turn, set_turn_status, ChatTurnEvents};
use crate::{
    agent::{engine::AgentEvent, user_input::UserInputRequest},
    app::state::AppServer,
    db::{
        conversation_inputs::{AnswerInputRequest, ConversationInput},
        ConversationRunStatus, Database,
    },
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures::Stream;
use std::{convert::Infallible, pin::Pin, sync::Arc};
use tokio::sync::mpsc::Sender;

type ApiError = (StatusCode, Json<serde_json::Value>);
fn error(message: String) -> ApiError {
    let status = match message.as_str() {
        "invalid_answer" => StatusCode::BAD_REQUEST,
        "question_already_resolved" => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(serde_json::json!({"error":message})))
}
pub async fn list_handler(
    State(server): State<Arc<AppServer>>,
    Path(conversation): Path<String>,
) -> Result<Json<Vec<ConversationInput>>, ApiError> {
    server
        .db
        .pending_conversation_inputs(&conversation)
        .map(Json)
        .map_err(error)
}

pub(crate) async fn persist_waiting_input(
    db: &Database,
    conversation: &str,
    tool_call: &str,
    request: &UserInputRequest,
    tx: &Sender<AgentEvent>,
    handoff: Option<&ChatTurnEvents>,
) {
    match db.create_conversation_input(conversation, tool_call, request) {
        Ok(question) => {
            let _ = set_turn_status(
                handoff,
                db,
                conversation,
                ConversationRunStatus::WaitingInput,
                None,
            );
            let mut event = AgentEvent::error_event(conversation, String::new());
            event.event_type = "user_input_required".into();
            event.error = None;
            event.tool_name = Some("ask_user".into());
            event.tool_call_id = Some(tool_call.into());
            event.args = Some(serde_json::to_value(question).expect("question serializes"));
            let _ = tx.send(event).await;
        }
        Err(failure) => {
            let _ = set_turn_status(
                handoff,
                db,
                conversation,
                ConversationRunStatus::Failed,
                Some(&failure),
            );
            let _ = tx
                .send(AgentEvent::error_event(conversation, failure))
                .await;
        }
    }
}

pub async fn answer_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(answer): Json<AnswerInputRequest>,
) -> Result<Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>, ApiError> {
    let question = server.db.conversation_input(&id).map_err(error)?;
    let conversation = question.conversation_id.clone();
    let guard = acquire_turn(&server, &conversation).await;
    let db = server.db.clone_connection();
    let current = db.conversation_input(&id).map_err(error)?;
    if current.status != "pending" || current.revision != answer.revision {
        return Err(error("question_already_resolved".into()));
    }
    let options = db.load_chat_options(&conversation).map_err(error)?;
    let (tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);
    let mut runtime = crate::api::chat::runtime::prepare_with_model(
        &server,
        Some(&conversation),
        None,
        options.model_profile_id.as_deref(),
    )
    .await
    .map_err(error)?;
    options.apply(&server, &mut runtime).map_err(error)?;
    let goal = crate::api::chat::load_conversation_goal_ref(&db, &conversation)
        .map_err(|failure| error(crate::api::chat::chat_error_message(failure)))?;
    let goal_run = goal
        .as_ref()
        .map(|reference| {
            crate::api::chat::resolve_goal_chat_run(
                &server,
                &db,
                reference,
                tokio_util::sync::CancellationToken::new(),
            )
        })
        .transpose()
        .map_err(|failure| error(crate::api::chat::chat_error_message(failure)))?;
    // This transaction is the answer's acceptance boundary; duplicate clients cannot consume it twice.
    let answered = db.answer_conversation_input(&id, &answer).map_err(error)?;
    let (handoff, mut rx) = ChatTurnEvents::start(db.clone_connection(), event_rx);
    let stream_conversation = conversation.clone();
    tokio::spawn(async move {
        let _guard = guard;
        let mut resolved = AgentEvent::error_event(&conversation, String::new());
        resolved.event_type = "user_input_resolved".into();
        resolved.error = None;
        resolved.tool_call_id = Some(answered.tool_call_id);
        resolved.args =
            Some(serde_json::json!({"question_id":answered.question_id,"run_id":answered.run_id}));
        let _ = tx.send(resolved).await;
        crate::api::approvals::resume_conversation(
            &server,
            &db,
            &runtime.tools,
            &runtime.config,
            &server.log_buffer,
            &runtime.prompt,
            &conversation,
            &tx,
            goal_run,
            &runtime,
            Some(&handoff),
        )
        .await;
        handoff.finish(&server, &conversation).await;
    });
    let stream = async_stream::stream! {
        yield Ok(Event::default().event("connected").data(serde_json::json!({"conversation_id":stream_conversation}).to_string()));
        while let Some(event)=rx.recv().await {yield Ok(Event::default().event("agent-event").data(serde_json::to_string(&event).expect("event serializes")));}
    };
    Ok(
        Sse::new(Box::pin(stream) as Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>)
            .keep_alive(axum::response::sse::KeepAlive::default()),
    )
}
