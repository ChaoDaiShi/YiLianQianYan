//! Cycle 6 contract freeze for global Voice and cross-context interaction.

use yilian_backend::shared::interaction::{
    ContextAnchorSnapshot, ConversationalAnchor, FocusedSurface, InteractionInput,
    InteractionIntent, InteractionSource, InteractionTarget, TargetResolution,
};
use yilian_backend::shared::voice::{
    GlobalVoiceSession, VoiceAttentionPolicy, VoiceInputLease, VoiceInputOwner, VoiceSessionState,
    VoiceTurn,
};

#[test]
fn global_session_keeps_surface_anchor_and_task_independent() {
    let mut session = GlobalVoiceSession::new(FocusedSurface::Conversation, 100);
    session.conversational_anchor = Some(ConversationalAnchor::new("conversation-a", "Rust", 101));
    session.active_task = Some("task-123".to_string());
    session.focused_surface = FocusedSurface::Workspace;

    assert_eq!(session.generation, 1);
    assert_eq!(session.state, VoiceSessionState::Listening);
    assert_eq!(session.attention_mode, VoiceAttentionPolicy::Balanced);
    assert_eq!(session.focused_surface, FocusedSurface::Workspace);
    assert_eq!(
        session
            .conversational_anchor
            .as_ref()
            .map(|anchor| anchor.conversation_id.as_str()),
        Some("conversation-a")
    );
    assert_eq!(session.active_task.as_deref(), Some("task-123"));
}

#[test]
fn input_lease_and_turn_are_bound_to_session_generation() {
    let session = GlobalVoiceSession::new(FocusedSurface::TaskCanvas, 200);
    let lease = VoiceInputLease::new(&session, VoiceInputOwner::PushToTalk, 201);
    let anchors = ContextAnchorSnapshot {
        focused_surface: FocusedSurface::TaskCanvas,
        conversational_anchor: None,
        active_task: Some("graph-1".to_string()),
    };
    let turn = VoiceTurn::new(
        &session,
        &lease,
        InteractionSource::Voice,
        "暂停这个任务",
        anchors,
        TargetResolution::Resolved {
            target: InteractionTarget::Task {
                graph_id: "graph-1".to_string(),
            },
        },
        InteractionIntent::Command {
            name: "task.pause".to_string(),
        },
        202,
    )
    .expect("valid current-generation turn");

    assert_eq!(lease.voice_session_id, session.voice_session_id);
    assert_eq!(lease.generation, session.generation);
    assert_eq!(turn.generation, session.generation);
    assert_eq!(turn.lease_id, lease.lease_id);
    assert_eq!(turn.final_transcript, "暂停这个任务");
}

#[test]
fn turn_rejects_stale_generation_empty_text_and_oversized_input() {
    let mut session = GlobalVoiceSession::new(FocusedSurface::Conversation, 300);
    let lease = VoiceInputLease::new(&session, VoiceInputOwner::BuiltinAsr, 301);
    session.generation += 1;

    let result = VoiceTurn::new(
        &session,
        &lease,
        InteractionSource::Voice,
        "迟到文本",
        ContextAnchorSnapshot::new(FocusedSurface::Conversation),
        TargetResolution::Missing {
            reason: "no current target".to_string(),
        },
        InteractionIntent::Query {
            name: "conversation.continue".to_string(),
        },
        302,
    );
    assert_eq!(result.unwrap_err().code(), "stale_generation");

    let current = VoiceInputLease::new(&session, VoiceInputOwner::BuiltinAsr, 303);
    let empty = VoiceTurn::new(
        &session,
        &current,
        InteractionSource::Voice,
        "   ",
        ContextAnchorSnapshot::new(FocusedSurface::Conversation),
        TargetResolution::Missing {
            reason: "empty".to_string(),
        },
        InteractionIntent::ConversationTurn,
        304,
    );
    assert_eq!(empty.unwrap_err().code(), "invalid_transcript");

    let oversized = InteractionInput {
        source: InteractionSource::Text,
        utterance: "x".repeat(4_097),
        voice_session_id: None,
        focused_surface: FocusedSurface::System,
        conversational_anchor: None,
        active_task: None,
    };
    assert_eq!(
        oversized.validate().unwrap_err().code(),
        "invalid_utterance"
    );
}

#[test]
fn target_resolution_serializes_explicit_non_executing_ambiguity() {
    let resolution = TargetResolution::Ambiguous {
        candidates: vec![
            InteractionTarget::Approval {
                approval_id: "approval-a".to_string(),
            },
            InteractionTarget::Approval {
                approval_id: "approval-b".to_string(),
            },
        ],
    };
    let value = serde_json::to_value(&resolution).unwrap();

    assert_eq!(value["status"], "ambiguous");
    assert_eq!(value["candidates"].as_array().unwrap().len(), 2);
    assert!(value.get("command").is_none());
}

#[test]
fn shared_contracts_accept_unknown_additive_fields() {
    let value = serde_json::json!({
        "source": "global_command",
        "utterance": "现在做到哪了",
        "focused_surface": "task_canvas",
        "future": true
    });
    let input: InteractionInput = serde_json::from_value(value).unwrap();

    assert_eq!(input.source, InteractionSource::GlobalCommand);
    assert_eq!(input.focused_surface, FocusedSurface::TaskCanvas);
    assert!(input.validate().is_ok());
}
