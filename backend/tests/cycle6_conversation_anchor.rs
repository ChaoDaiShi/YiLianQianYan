//! Narrow Cycle 6 v1 tests for persisted Conversation anchors.

use std::path::Path;

use yilian_backend::db::{Database, MessageRow};
use yilian_backend::interaction::ConversationVoiceAdapter;
use yilian_backend::shared::interaction::{
    ConversationalAnchor, InteractionTarget, TargetResolution,
};

fn message(conversation_id: &str, id: &str, content: &str) -> MessageRow {
    MessageRow {
        id: id.to_string(),
        conversation_id: conversation_id.to_string(),
        role: "user".to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: 100,
    }
}

fn database() -> Database {
    Database::new(Path::new(":memory:")).expect("database opens")
}

fn resolved_id(result: TargetResolution) -> String {
    match result {
        TargetResolution::Resolved {
            target: InteractionTarget::Conversation { conversation_id },
        } => conversation_id,
        other => panic!("expected one resolved conversation, got {other:?}"),
    }
}

#[test]
fn anchors_survive_surface_changes_and_explicit_targets() {
    let db = database();
    let conversation = db
        .create_conversation("项目复盘")
        .expect("conversation creates");
    db.add_message(&message(&conversation.id, "message-1", "开始"))
        .unwrap();
    let adapter = ConversationVoiceAdapter::new(&db);
    let anchor = ConversationalAnchor::new(&conversation.id, &conversation.title, 101);

    // A focused-surface change does not alter the persisted conversation identity.
    assert_eq!(
        resolved_id(adapter.validate_anchor(&anchor)),
        conversation.id
    );
    assert_eq!(
        resolved_id(
            adapter.resolve_anchor(Some(&format!("conversation://{}", conversation.id)), None)
        ),
        conversation.id
    );
}

#[test]
fn one_normalized_title_match_resolves_and_duplicates_fail_closed() {
    let db = database();
    let first = db.create_conversation("Rust 复盘").expect("first creates");
    let second = db.create_conversation("Rust 复盘").expect("second creates");
    db.add_message(&message(&first.id, "message-first", "一"))
        .unwrap();
    db.add_message(&message(&second.id, "message-second", "二"))
        .unwrap();
    let adapter = ConversationVoiceAdapter::new(&db);

    // Whitespace and case/punctuation are not part of the title identity.
    let ambiguous = adapter.resolve_anchor(None, Some(" rust 复盘！ "));
    assert!(matches!(ambiguous, TargetResolution::Ambiguous { .. }));

    db.delete_conversation(&second.id).expect("second deletes");
    assert_eq!(
        resolved_id(adapter.resolve_anchor(None, Some("Rust复盘"))),
        first.id
    );
}

#[test]
fn deleted_anchor_is_missing_and_commit_does_not_create_a_ghost_message() {
    let db = database();
    let conversation = db
        .create_conversation("待删除对话")
        .expect("conversation creates");
    db.add_message(&message(&conversation.id, "message-seed", "保留"))
        .unwrap();
    let adapter = ConversationVoiceAdapter::new(&db);
    let anchor = ConversationalAnchor::new(&conversation.id, &conversation.title, 102);

    db.delete_conversation(&conversation.id)
        .expect("conversation deletes");
    assert!(matches!(
        adapter.validate_anchor(&anchor),
        TargetResolution::Missing { .. }
    ));

    let result = adapter.commit_turn(&anchor, &message(&conversation.id, "ghost", "不应写入"));
    assert!(
        result.is_err(),
        "deleted anchors must fail before persistence"
    );
    assert!(db.get_conversation(&conversation.id).is_err());
    assert!(db
        .add_message_if_conversation_exists(&message(
            &conversation.id,
            "ghost-direct",
            "也不应写入",
        ))
        .is_err());
}

#[test]
fn atomic_message_commit_rechecks_conversation_identity_after_anchor_validation() {
    let db = database();
    let conversation = db
        .create_conversation("原子提交")
        .expect("conversation creates");
    let adapter = ConversationVoiceAdapter::new(&db);
    let anchor = ConversationalAnchor::new(&conversation.id, &conversation.title, 104);

    assert!(matches!(
        adapter.validate_anchor(&anchor),
        TargetResolution::Resolved { .. }
    ));
    db.delete_conversation(&conversation.id)
        .expect("conversation deletes");
    let result = adapter.commit_turn(&anchor, &message(&conversation.id, "ghost-race", "拒绝"));
    assert!(result.is_err());
    assert!(db.get_conversation(&conversation.id).is_err());
}

#[test]
fn commit_turn_revalidates_anchor_and_persists_only_to_that_conversation() {
    let db = database();
    let conversation = db
        .create_conversation("可继续对话")
        .expect("conversation creates");
    db.add_message(&message(&conversation.id, "message-seed", "之前"))
        .unwrap();
    let adapter = ConversationVoiceAdapter::new(&db);
    let anchor = ConversationalAnchor::new(&conversation.id, &conversation.title, 103);

    adapter
        .commit_turn(
            &anchor,
            &message(&conversation.id, "message-turn", "继续讨论"),
        )
        .expect("turn commits");
    let stored = db
        .get_conversation(&conversation.id)
        .expect("conversation loads");
    assert!(stored.messages.iter().any(|item| item.id == "message-turn"));
}
