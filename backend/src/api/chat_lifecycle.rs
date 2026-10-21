//! A conversation turn hands history, execution events and run state over as one ordered lifecycle.
use crate::{
    agent::engine::AgentEvent,
    app::state::AppServer,
    db::{ConversationRunStatus, Database},
};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

enum DrainCommand {
    Flush(oneshot::Sender<Result<(), String>>),
    Publish {
        conversation: String,
        status: ConversationRunStatus,
        error: Option<String>,
        complete: oneshot::Sender<()>,
    },
}
pub struct ChatTurnEvents {
    commands: mpsc::UnboundedSender<DrainCommand>,
    final_state: Mutex<Option<(ConversationRunStatus, Option<String>)>>,
}
fn persist_event(
    db: &Database,
    stream: &mpsc::UnboundedSender<AgentEvent>,
    pending: &mut Vec<AgentEvent>,
    failure: &mut Option<String>,
    reasoning_baseline: &mut Option<String>,
    mut event: AgentEvent,
) {
    if matches!(
        event.event_type.as_str(),
        "reasoning" | "reasoning_snapshot"
    ) {
        if reasoning_baseline.is_none() {
            match db.current_chat_reasoning(&event.conversation_id) {
                Ok(value) => *reasoning_baseline = Some(value),
                Err(error) => {
                    if failure.is_none() {
                        *failure = Some(error);
                    }
                    return;
                }
            }
        }
        if event.event_type == "reasoning_snapshot" {
            event.token = Some(format!(
                "{}{}",
                reasoning_baseline.as_deref().unwrap_or_default(),
                event.token.as_deref().unwrap_or_default()
            ));
        }
    }
    if matches!(
        event.event_type.as_str(),
        "tool_start" | "approval_required"
    ) {
        if let Some(name) = event.tool_name.as_deref() {
            let mode = db
                .conversation_mode(&event.conversation_id)
                .and_then(|current| {
                    let next = current.for_tool(name);
                    if next != current {
                        db.set_conversation_mode(&event.conversation_id, next)?;
                        Ok(Some(next))
                    } else {
                        Ok(None)
                    }
                });
            match mode {
                Ok(Some(next)) => {
                    let _ = stream.send(crate::api::conversation_modes::event(
                        &event.conversation_id,
                        next,
                        true,
                        &format!("智能体开始使用 {name}"),
                    ));
                }
                Err(error) => {
                    if failure.is_none() {
                        *failure = Some(error);
                    }
                }
                _ => {}
            }
        }
    }
    if let Err(error) = db.apply_execution_event(&event) {
        if failure.is_none() {
            *failure = Some(error);
        }
    }
    if let Err(error) = db.append_chat_reasoning_event(&event) {
        if failure.is_none() {
            *failure = Some(error);
        }
    }
    if matches!(
        event.event_type.as_str(),
        "done" | "error" | "approval_required"
    ) {
        pending.push(event);
    } else {
        let _ = stream.send(event);
    }
}
impl ChatTurnEvents {
    pub fn start(
        db: Database,
        mut events: mpsc::Receiver<AgentEvent>,
    ) -> (Self, mpsc::UnboundedReceiver<AgentEvent>) {
        let (commands, mut requests) = mpsc::unbounded_channel();
        let (stream, observer) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut pending = Vec::new();
            let mut failure = None;
            let mut reasoning_baseline = None;
            loop {
                tokio::select! {
                    event = events.recv() => match event {
                        Some(event) => persist_event(&db, &stream, &mut pending, &mut failure, &mut reasoning_baseline, event),
                        None => break,
                    },
                    request = requests.recv() => match request {
                        Some(DrainCommand::Flush(reply)) => {
                            // The producer calls this only after its engine/tool futures return.
                            // Every completed send is already queued, irrespective of select order.
                            while let Ok(event) = events.try_recv() { persist_event(&db, &stream, &mut pending, &mut failure, &mut reasoning_baseline, event); }
                            let _ = reply.send(match &failure { Some(error) => Err(error.clone()), None => Ok(()) });
                        }
                        Some(DrainCommand::Publish { conversation, status, error, complete }) => {
                            let expected = match status { ConversationRunStatus::Completed => "done", ConversationRunStatus::WaitingApproval => "approval_required", _ => "error" };
                            let mut published = false;
                            for event in pending { if event.event_type == expected { let _ = stream.send(event); published = true; } }
                            if !published { if let Some(error) = error { let _ = stream.send(AgentEvent::error_event(&conversation,error)); } }
                            let _ = complete.send(());
                            break;
                        }
                        None => break,
                    }
                }
            }
        });
        (
            Self {
                commands,
                final_state: Mutex::new(None),
            },
            observer,
        )
    }
    pub fn set_status(
        &self,
        db: &Database,
        conversation: &str,
        status: ConversationRunStatus,
        error: Option<&str>,
    ) -> Result<(), String> {
        if status == ConversationRunStatus::Running {
            return db.set_conversation_run_status(conversation, status, error);
        }
        *self.final_state.lock() = Some((status, error.map(str::to_string)));
        Ok(())
    }
    pub async fn flush(&self) -> Result<(), String> {
        let (reply, complete) = oneshot::channel();
        self.commands
            .send(DrainCommand::Flush(reply))
            .map_err(|_| "chat execution drain unavailable".to_string())?;
        complete
            .await
            .map_err(|_| "chat execution drain interrupted".to_string())?
    }
    pub async fn finish(self, server: &AppServer, conversation: &str) {
        let failure = self.flush().await.err();
        let (mut status, mut error) = self.final_state.lock().take().unwrap_or((
            ConversationRunStatus::Failed,
            Some("chat turn ended without an outcome".into()),
        ));
        if let Some(failure) = failure {
            status = ConversationRunStatus::Failed;
            error = Some(failure);
        }
        if let Err(failure) =
            server
                .db
                .set_conversation_run_status(conversation, status, error.as_deref())
        {
            status = ConversationRunStatus::Failed;
            error = Some(failure);
        }
        server.active_tasks.lock().remove(conversation);
        let (complete, published) = oneshot::channel();
        if self
            .commands
            .send(DrainCommand::Publish {
                conversation: conversation.into(),
                status,
                error,
                complete,
            })
            .is_ok()
        {
            let _ = published.await;
        }
    }
}
pub fn set_turn_status(
    handoff: Option<&ChatTurnEvents>,
    db: &Database,
    conversation: &str,
    status: ConversationRunStatus,
    error: Option<&str>,
) -> Result<(), String> {
    match handoff {
        Some(handoff) => handoff.set_status(db, conversation, status, error),
        None => db.set_conversation_run_status(conversation, status, error),
    }
}

pub async fn acquire_turn(
    server: &AppServer,
    conversation: &str,
) -> tokio::sync::OwnedMutexGuard<()> {
    let gate = server
        .chat_turn_gates
        .lock()
        .entry(conversation.into())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    gate.lock_owned().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        integrations::llm::types::{ChatMessage, ToolCall, ToolCallFunction},
        safety::ControlSession,
    };
    use tokio_util::sync::CancellationToken;
    fn server() -> Arc<AppServer> {
        let root = std::env::temp_dir().join(format!("chat-handoff-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Arc::new(
            AppServer::new_with_control_session(
                &root.join("test.db"),
                root.to_str().unwrap(),
                ControlSession::new("a".repeat(64)).unwrap(),
            )
            .unwrap(),
        )
    }
    fn event(conversation: &str, kind: &str) -> AgentEvent {
        let mut event = AgentEvent::error_event(conversation, "test".into());
        event.event_type = kind.into();
        event.error = None;
        event.tool_call_id = Some("call-1".into());
        event.tool_name = Some("read_file".into());
        event.args = Some(serde_json::json!({"path":"file.txt"}));
        event
    }
    #[tokio::test]
    async fn complete_snapshot_reconciles_live_loss_and_preserves_approval_reasoning_prefix() {
        let server = server();
        let conversation = server
            .db
            .create_conversation("reasoning continuation")
            .unwrap()
            .id;
        server
            .db
            .persist_chat_message(&conversation, &ChatMessage::user("actual question"), 1)
            .unwrap();
        let mut prior = event(&conversation, "reasoning");
        prior.token = Some("before approval. ".into());
        server.db.append_chat_reasoning_event(&prior).unwrap();
        let (tx, events) = mpsc::channel(8);
        let (handoff, mut observer) = ChatTurnEvents::start(server.db.clone_connection(), events);
        let mut delta = event(&conversation, "reasoning");
        delta.token = Some("resumed ".into());
        tx.send(delta).await.unwrap();
        let mut snapshot = event(&conversation, "reasoning_snapshot");
        snapshot.token = Some("resumed complete reasoning".into());
        tx.send(snapshot).await.unwrap();
        handoff.flush().await.unwrap();
        assert_eq!(
            server.db.current_chat_reasoning(&conversation).unwrap(),
            "before approval. resumed complete reasoning"
        );
        assert_eq!(observer.recv().await.unwrap().event_type, "reasoning");
        assert_eq!(
            observer.recv().await.unwrap().token.as_deref(),
            Some("before approval. resumed complete reasoning")
        );
    }
    #[tokio::test]
    async fn provider_reasoning_fragments_survive_disconnect_without_a_final_answer() {
        let server = server();
        let conversation = server
            .db
            .create_conversation("partial reasoning")
            .unwrap()
            .id;
        server
            .db
            .persist_chat_message(&conversation, &ChatMessage::user("actual question"), 1)
            .unwrap();
        let user_id = server.db.get_conversation(&conversation).unwrap().messages[0]
            .id
            .clone();
        let (tx, events) = mpsc::channel(256);
        let (handoff, observer) = ChatTurnEvents::start(server.db.clone_connection(), events);
        drop(observer);
        for fragment in ["provider ", "reasoning"] {
            let mut reasoning = event(&conversation, "reasoning");
            reasoning.token = Some(fragment.into());
            tx.send(reasoning).await.unwrap();
        }
        handoff.flush().await.unwrap();
        let detail = server.db.get_conversation(&conversation).unwrap();
        assert_eq!(
            detail.turn_reasoning.get(&user_id).map(String::as_str),
            Some("provider reasoning")
        );
        assert!(detail.message_reasoning.is_empty());
        assert_eq!(detail.messages.len(), 1);
        server.db.delete_conversation(&conversation).unwrap();
        let remaining: i64 = server
            .db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key=?1",
                [format!("chat_turn_reasoning:{user_id}")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
    }
    #[tokio::test]
    async fn paused_history_and_events_commit_before_fast_approval_owns_turn() {
        let server = server();
        let conversation = server.db.create_conversation("race").unwrap().id;
        server
            .db
            .set_conversation_run_status(&conversation, ConversationRunStatus::Running, None)
            .unwrap();
        let initial = acquire_turn(&server, &conversation).await;
        server
            .active_tasks
            .lock()
            .insert(conversation.clone(), CancellationToken::new());
        let (tx, events) = mpsc::channel(256);
        let (handoff, mut observer) = ChatTurnEvents::start(server.db.clone_connection(), events);
        tx.send(event(&conversation, "tool_start")).await.unwrap();
        let mut approval = event(&conversation, "approval_required");
        approval.approval_id = Some("approval-1".into());
        tx.send(approval).await.unwrap();
        handoff.flush().await.unwrap();
        assert_eq!(observer.try_recv().unwrap().event_type, "tool_start");
        assert!(observer.try_recv().is_err());
        let (attempting, attempted) = oneshot::channel();
        let (ready, acquired) = oneshot::channel();
        let (release, resume) = oneshot::channel();
        let continued = server.clone();
        let id = conversation.clone();
        let continuation = tokio::spawn(async move {
            let _ = attempting.send(());
            let _guard = acquire_turn(&continued, &id).await;
            let persisted = continued.db.get_conversation(&id).unwrap();
            assert_eq!(persisted.run_status, ConversationRunStatus::WaitingApproval);
            assert_eq!(
                persisted.messages[0]
                    .tool_calls
                    .as_ref()
                    .map(|value| serde_json::from_str::<Vec<ToolCall>>(value).unwrap().len()),
                Some(1)
            );
            assert_eq!(persisted.execution_history[0].approval_status, "pending");
            assert!(!continued.active_tasks.lock().contains_key(&id));
            continued
                .active_tasks
                .lock()
                .insert(id.clone(), CancellationToken::new());
            continued
                .db
                .set_conversation_run_status(&id, ConversationRunStatus::Running, None)
                .unwrap();
            let _ = ready.send(());
            let _ = resume.await;
        });
        attempted.await.unwrap();
        let mut acquired = acquired;
        assert!(matches!(
            acquired.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        let mut assistant = ChatMessage::user("");
        assistant.role = "assistant".into();
        assistant.tool_calls = Some(vec![ToolCall {
            id: "call-1".into(),
            call_type: "function".into(),
            function: ToolCallFunction {
                name: "read_file".into(),
                arguments: "{}".into(),
            },
        }]);
        server
            .db
            .persist_chat_message(&conversation, &assistant, 1)
            .unwrap();
        handoff
            .set_status(
                &server.db,
                &conversation,
                ConversationRunStatus::WaitingApproval,
                None,
            )
            .unwrap();
        assert_eq!(
            server
                .db
                .get_conversation(&conversation)
                .unwrap()
                .run_status,
            ConversationRunStatus::Running
        );
        handoff.finish(&server, &conversation).await;
        assert_eq!(
            observer.recv().await.unwrap().event_type,
            "approval_required"
        );
        drop(initial);
        acquired.await.unwrap();
        assert!(server.active_tasks.lock().contains_key(&conversation));
        assert_eq!(
            server
                .db
                .get_conversation(&conversation)
                .unwrap()
                .run_status,
            ConversationRunStatus::Running
        );
        let _ = release.send(());
        continuation.await.unwrap();
    }
    #[tokio::test]
    async fn terminal_state_waits_for_event_drain_even_when_observer_drops() {
        let server = server();
        let conversation = server.db.create_conversation("drain").unwrap().id;
        server
            .db
            .set_conversation_run_status(&conversation, ConversationRunStatus::Running, None)
            .unwrap();
        let _guard = acquire_turn(&server, &conversation).await;
        let (tx, events) = mpsc::channel(256);
        let (handoff, observer) = ChatTurnEvents::start(server.db.clone_connection(), events);
        drop(observer);
        tx.send(event(&conversation, "tool_start")).await.unwrap();
        let mut end = event(&conversation, "tool_end");
        end.status = Some("success".into());
        end.result = Some("read result".into());
        tx.send(end).await.unwrap();
        let mut verified = event(&conversation, "verification");
        verified.verification_success = Some(true);
        verified.verification_reason = Some("verified".into());
        tx.send(verified).await.unwrap();
        tx.send(event(&conversation, "done")).await.unwrap();
        handoff
            .set_status(
                &server.db,
                &conversation,
                ConversationRunStatus::Completed,
                None,
            )
            .unwrap();
        assert_eq!(
            server
                .db
                .get_conversation(&conversation)
                .unwrap()
                .run_status,
            ConversationRunStatus::Running
        );
        handoff.finish(&server, &conversation).await;
        let persisted = server.db.get_conversation(&conversation).unwrap();
        assert_eq!(persisted.run_status, ConversationRunStatus::Completed);
        assert_eq!(persisted.execution_history[0].execution_status, "succeeded");
        assert_eq!(persisted.execution_history[0].verification_status, "passed");
    }
}
