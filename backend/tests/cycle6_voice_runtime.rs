//! Narrow Cycle 6 v2 tests for the global voice lifecycle.

use yilian_backend::shared::event::EventHub;
use yilian_backend::shared::interaction::{
    ContextAnchorSnapshot, ConversationalAnchor, FocusedSurface,
};
use yilian_backend::shared::voice::{VoiceInputOwner, VoiceSessionState};
use yilian_backend::voice::runtime::{GlobalVoiceSessionRuntime, VoiceRuntimeError};

fn runtime() -> GlobalVoiceSessionRuntime {
    GlobalVoiceSessionRuntime::new(EventHub::new(32))
}

#[test]
fn explicit_anchor_switch_invalidates_inflight_capture_and_accepted_final() {
    for accept_before_switch in [false, true] {
        let runtime = runtime();
        let initial = runtime.start(FocusedSurface::Conversation).unwrap();
        let first = runtime
            .update_context(
                &initial.voice_session_id,
                initial.generation,
                ContextAnchorSnapshot {
                    focused_surface: FocusedSurface::Conversation,
                    conversational_anchor: Some(ConversationalAnchor::new("a", "A", 1)),
                    active_task: None,
                },
            )
            .unwrap();
        let lease = runtime
            .acquire_lease(
                &first.voice_session_id,
                first.generation,
                VoiceInputOwner::BuiltinAsr,
            )
            .unwrap();
        if accept_before_switch {
            runtime
                .commit_final(
                    &first.voice_session_id,
                    first.generation,
                    &lease.lease_id,
                    "hello",
                )
                .unwrap();
        }
        let switched = runtime
            .update_context(
                &first.voice_session_id,
                first.generation,
                ContextAnchorSnapshot {
                    focused_surface: FocusedSurface::Conversation,
                    conversational_anchor: Some(ConversationalAnchor::new("b", "B", 2)),
                    active_task: None,
                },
            )
            .unwrap();
        assert!(switched.generation > first.generation);
        assert_eq!(
            runtime.commit_final(
                &first.voice_session_id,
                first.generation,
                &lease.lease_id,
                "hello"
            ),
            Err(VoiceRuntimeError::StaleGeneration)
        );
        assert_eq!(
            runtime.accepted_final(
                &first.voice_session_id,
                first.generation,
                &lease.lease_id,
                "hello"
            ),
            Err(VoiceRuntimeError::StaleGeneration)
        );
    }
}

#[test]
fn session_generation_is_monotonic_and_reinitialize_releases_old_input() {
    let runtime = runtime();
    let started = runtime
        .start(FocusedSurface::Conversation)
        .expect("session starts");
    let lease = runtime
        .acquire_lease(
            &started.voice_session_id,
            started.generation,
            VoiceInputOwner::BuiltinAsr,
        )
        .expect("first lease acquires");

    let restarted = runtime
        .reinitialize_input(&started.voice_session_id, started.generation)
        .expect("input reinitializes");

    assert_eq!(restarted.voice_session_id, started.voice_session_id);
    assert_eq!(restarted.generation, started.generation + 1);
    assert!(runtime
        .acquire_lease(
            &started.voice_session_id,
            started.generation,
            VoiceInputOwner::ExternalAsr,
        )
        .is_err());
    let current = runtime
        .acquire_lease(
            &restarted.voice_session_id,
            restarted.generation,
            VoiceInputOwner::PushToTalk,
        )
        .expect("new-generation lease acquires");
    assert_ne!(current.lease_id, lease.lease_id);
    assert_eq!(current.generation, restarted.generation);
}

#[test]
fn only_one_final_input_lease_can_be_active() {
    let runtime = runtime();
    let session = runtime.start(FocusedSurface::TaskCanvas).unwrap();
    runtime
        .acquire_lease(
            &session.voice_session_id,
            session.generation,
            VoiceInputOwner::BuiltinAsr,
        )
        .unwrap();

    let error = runtime
        .acquire_lease(
            &session.voice_session_id,
            session.generation,
            VoiceInputOwner::ExternalAsr,
        )
        .expect_err("second final submitter must be rejected");
    assert_eq!(error, VoiceRuntimeError::LeaseConflict);
}

#[test]
fn start_and_context_updates_keep_one_session_across_surfaces() {
    let runtime = runtime();
    let started = runtime.start(FocusedSurface::Conversation).unwrap();
    let context = ContextAnchorSnapshot {
        focused_surface: FocusedSurface::TaskCanvas,
        conversational_anchor: Some(ConversationalAnchor::new("conversation-a", "Rust", 10)),
        active_task: Some("task-123".to_string()),
    };

    let task = runtime
        .update_context(&started.voice_session_id, started.generation, context)
        .unwrap();
    assert_eq!(task.focused_surface, FocusedSurface::TaskCanvas);
    assert_eq!(task.active_task.as_deref(), Some("task-123"));

    let desktop = runtime
        .update_context(
            &started.voice_session_id,
            task.generation,
            ContextAnchorSnapshot {
                focused_surface: FocusedSurface::Workspace,
                conversational_anchor: task.conversational_anchor.clone(),
                active_task: task.active_task.clone(),
            },
        )
        .unwrap();
    let returned = runtime.start(FocusedSurface::Conversation).unwrap();

    assert_eq!(desktop.voice_session_id, started.voice_session_id);
    assert_eq!(returned.voice_session_id, started.voice_session_id);
    assert_eq!(returned.generation, task.generation);
    assert_eq!(desktop.generation, task.generation);
    assert_eq!(returned.focused_surface, FocusedSurface::Conversation);
    assert_eq!(returned.active_task.as_deref(), Some("task-123"));
    assert_eq!(
        returned
            .conversational_anchor
            .as_ref()
            .map(|anchor| anchor.conversation_id.as_str()),
        Some("conversation-a")
    );
}

#[test]
fn stale_final_transcript_is_dropped_after_generation_changes() {
    let runtime = runtime();
    let session = runtime.start(FocusedSurface::Conversation).unwrap();
    let lease = runtime
        .acquire_lease(
            &session.voice_session_id,
            session.generation,
            VoiceInputOwner::BuiltinAsr,
        )
        .unwrap();
    let current = runtime
        .reinitialize_input(&session.voice_session_id, session.generation)
        .unwrap();

    let error = runtime
        .commit_final(
            &session.voice_session_id,
            session.generation,
            &lease.lease_id,
            "迟到文本",
        )
        .expect_err("old ASR final must be dropped");
    assert_eq!(error, VoiceRuntimeError::StaleGeneration);
    assert_eq!(runtime.snapshot().final_transcript, None);
    assert_eq!(
        runtime.snapshot().session.unwrap().generation,
        current.generation
    );
}

#[test]
fn end_cleans_ephemeral_voice_state_without_touching_context_facts() {
    let runtime = runtime();
    let session = runtime.start(FocusedSurface::Workspace).unwrap();
    let context = runtime
        .update_context(
            &session.voice_session_id,
            session.generation,
            ContextAnchorSnapshot {
                focused_surface: FocusedSurface::Workspace,
                conversational_anchor: Some(ConversationalAnchor::new(
                    "conversation-a",
                    "Rust",
                    10,
                )),
                active_task: Some("task-123".to_string()),
            },
        )
        .unwrap();
    let lease = runtime
        .acquire_lease(
            &context.voice_session_id,
            context.generation,
            VoiceInputOwner::PushToTalk,
        )
        .unwrap();
    runtime
        .update_partial(
            &context.voice_session_id,
            context.generation,
            &lease.lease_id,
            "暂停",
        )
        .unwrap();
    runtime
        .commit_final(
            &context.voice_session_id,
            context.generation,
            &lease.lease_id,
            "暂停这个任务",
        )
        .unwrap();

    let ended = runtime
        .end(&context.voice_session_id, context.generation)
        .expect("session ends");
    let snapshot = runtime.snapshot();

    assert_eq!(ended.state, VoiceSessionState::Ended);
    assert_eq!(ended.generation, context.generation + 1);
    assert_eq!(snapshot.lease, None);
    assert_eq!(snapshot.partial_transcript, None);
    assert_eq!(snapshot.final_transcript, None);
    assert_eq!(
        snapshot.presence.interaction,
        yilian_backend::shared::voice::PresenceInteraction::None
    );
    assert_eq!(ended.active_task.as_deref(), Some("task-123"));
    assert!(serde_json::to_string(&snapshot)
        .unwrap()
        .find("audio")
        .is_none());
}

#[test]
fn restarting_an_ended_runtime_starts_a_new_session_as_listening() {
    let runtime = runtime();
    let ended = runtime
        .end(
            &runtime
                .start(FocusedSurface::Conversation)
                .unwrap()
                .voice_session_id,
            1,
        )
        .unwrap();
    let restarted = runtime.start(FocusedSurface::Conversation).unwrap();

    assert_eq!(ended.state, VoiceSessionState::Ended);
    assert_eq!(restarted.state, VoiceSessionState::Listening);
    assert_ne!(restarted.voice_session_id, ended.voice_session_id);
    assert_eq!(restarted.generation, 1);
}

#[test]
fn interrupt_advances_generation_and_invalidates_old_speech_and_input() {
    let runtime = runtime();
    let session = runtime.start(FocusedSurface::Conversation).unwrap();
    let lease = runtime
        .acquire_lease(
            &session.voice_session_id,
            session.generation,
            VoiceInputOwner::BuiltinAsr,
        )
        .unwrap();

    let interrupted = runtime
        .interrupt(&session.voice_session_id, session.generation)
        .expect("interrupt succeeds");

    assert_eq!(interrupted.generation, session.generation + 1);
    assert_eq!(interrupted.state, VoiceSessionState::Interrupted);
    assert_eq!(runtime.lease(), None);
    assert_eq!(
        runtime
            .begin_speaking(&session.voice_session_id, session.generation)
            .expect_err("old TTS generation must be rejected"),
        VoiceRuntimeError::StaleGeneration
    );
    assert_eq!(
        runtime
            .commit_final(
                &session.voice_session_id,
                session.generation,
                &lease.lease_id,
                "迟到语音",
            )
            .expect_err("old ASR result must be rejected"),
        VoiceRuntimeError::StaleGeneration
    );
}
