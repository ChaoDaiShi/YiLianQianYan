//! Narrow Cycle 6 v1 tests for deterministic Task/Conversation routing.

use yilian_backend::interaction::{InteractionContext, InteractionDecision, InteractionRouter};
use yilian_backend::shared::interaction::{
    ConversationalAnchor, FocusedSurface, InteractionInput, InteractionIntent, InteractionSource,
    InteractionTarget, TargetResolution,
};

fn input(utterance: &str) -> InteractionInput {
    InteractionInput {
        source: InteractionSource::Voice,
        utterance: utterance.to_string(),
        voice_session_id: Some("voice-session-1".to_string()),
        focused_surface: FocusedSurface::TaskCanvas,
        conversational_anchor: None,
        active_task: Some("task-a".to_string()),
    }
}

fn approval_input(utterance: &str) -> InteractionInput {
    let mut value = input(utterance);
    value.conversational_anchor = Some(ConversationalAnchor::new("conversation-a", "当前对话", 1));
    value
}

fn task_context() -> InteractionContext {
    InteractionContext::default().with_task("task-a", "Cycle 6")
}

fn assert_command(utterance: &str, expected: &str) {
    let decision = InteractionRouter::new().resolve(&input(utterance), &task_context());
    assert_eq!(decision.command.as_deref(), Some(expected));
    assert!(matches!(decision.target, TargetResolution::Resolved { .. }));
}

#[test]
fn exact_fast_path_commands_are_classified_without_deep_planning() {
    assert_command("暂停这个任务", "task.pause");
    assert_command("继续", "task.resume");
    assert_command("取消当前任务", "task.cancel");
    assert_command("重试这个任务", "task.retry");
    assert_command("重新运行这个任务", "task.rerun");
    assert_command("现在做到哪了", "task.status");
    assert_command("当前任务", "task.current");

    let approval_context =
        task_context().with_approval_in_conversation("approval-a", "conversation-a", "打开记事本");
    let decision = InteractionRouter::new().resolve(&approval_input("同意"), &approval_context);
    assert_eq!(decision.command.as_deref(), Some("task.approval.resolve"));
    assert_eq!(decision.parameters["resolution"], "approve");
    assert_eq!(decision.parameters["conversation_id"], "conversation-a");

    let decision = InteractionRouter::new().resolve(&approval_input("拒绝"), &approval_context);
    assert_eq!(decision.command.as_deref(), Some("task.approval.resolve"));
    assert_eq!(decision.parameters["resolution"], "reject");
}

#[test]
fn bare_approval_without_a_current_conversation_is_not_executable() {
    let context =
        task_context().with_approval_in_conversation("approval-a", "conversation-a", "打开记事本");
    let decision = InteractionRouter::new().resolve(&input("同意"), &context);
    assert!(matches!(decision.target, TargetResolution::Missing { .. }));
    assert!(decision.command.is_none());
}

#[test]
fn conversation_continuation_uses_the_explicit_anchor() {
    let mut request = input("继续刚才那个问题");
    request.focused_surface = FocusedSurface::Workspace;
    request.conversational_anchor = Some(ConversationalAnchor::new("conversation-a", "Rust", 10));

    let decision = InteractionRouter::new().resolve(&request, &InteractionContext::default());
    assert_eq!(decision.intent, InteractionIntent::ConversationTurn);
    assert_eq!(
        decision.target,
        TargetResolution::Resolved {
            target: InteractionTarget::Conversation {
                conversation_id: "conversation-a".to_string()
            }
        }
    );
    assert!(decision.command.is_none());
}

#[test]
fn unknown_and_multiple_referents_are_non_executable() {
    let mut no_active_task = input("暂停那个任务");
    no_active_task.active_task = None;
    let missing = InteractionRouter::new().resolve(&no_active_task, &InteractionContext::default());
    assert!(matches!(missing.target, TargetResolution::Missing { .. }));
    assert!(missing.command.is_none());

    let ambiguous = InteractionRouter::new().resolve(
        &no_active_task,
        &InteractionContext::default()
            .with_task("task-a", "同名任务")
            .with_task("task-b", "同名任务"),
    );
    assert!(matches!(
        ambiguous.target,
        TargetResolution::Ambiguous { .. }
    ));
    assert!(ambiguous.command.is_none());
}

#[test]
fn title_matching_can_continue_only_when_unique() {
    let mut request = input("切到昨天那个 Rust 对话");
    request.active_task = None;
    request.focused_surface = FocusedSurface::Workspace;

    let unique = InteractionRouter::new().resolve(
        &request,
        &InteractionContext::default().with_conversation("conversation-a", "昨天的 Rust 对话"),
    );
    assert_eq!(
        unique.target,
        TargetResolution::Resolved {
            target: InteractionTarget::Conversation {
                conversation_id: "conversation-a".to_string()
            }
        }
    );

    let duplicate = InteractionRouter::new().resolve(
        &request,
        &InteractionContext::default()
            .with_conversation("conversation-a", "昨天的 Rust 对话")
            .with_conversation("conversation-b", "昨天的 Rust 对话"),
    );
    assert!(matches!(
        duplicate.target,
        TargetResolution::Ambiguous { .. }
    ));
    assert!(duplicate.command.is_none());
}

#[test]
fn decisions_preserve_the_contract_intent_and_resolution_together() {
    let decision: InteractionDecision =
        InteractionRouter::new().resolve(&input("现在做到哪了"), &task_context());
    assert_eq!(
        decision.intent,
        InteractionIntent::Query {
            name: "task.status".to_string()
        }
    );
    assert_eq!(
        decision.target,
        TargetResolution::Resolved {
            target: InteractionTarget::Task {
                graph_id: "task-a".to_string()
            }
        }
    );
}
