use super::*;
use crate::db::ConversationRunStatus;

fn row(conversation: &str, id: &str, role: &str, content: &str, time: i64) -> MessageRow {
    MessageRow {
        id: id.into(),
        conversation_id: conversation.into(),
        role: role.into(),
        content: content.into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: time,
    }
}
fn append(db: &Database, message: MessageRow) {
    if message.role == "user" {
        db.accept_conversation_turn(&message).unwrap();
        db.finish_conversation_turn(&message.conversation_id, "completed")
            .unwrap();
        db.set_conversation_run_status(
            &message.conversation_id,
            ConversationRunStatus::Completed,
            None,
        )
        .unwrap();
    } else {
        db.add_message(&message).unwrap();
    }
}
fn tool_pair(db: &Database, conversation: &str, complete: bool, time: i64) {
    let mut assistant = row(conversation, "tool-call", "assistant", "", time);
    assistant.tool_calls=Some(serde_json::json!([{"id":"call","type":"function","function":{"name":"read_file","arguments":"{}"}}]).to_string());
    append(db, assistant);
    if complete {
        let mut tool = row(
            conversation,
            "tool-result",
            "tool",
            "actual tool result",
            time,
        );
        tool.tool_call_id = Some("call".into());
        tool.tool_name = Some("read_file".into());
        append(db, tool);
    }
}

#[test]
fn deleting_one_visible_answer_preserves_other_visible_messages_and_drops_only_internal_protocol() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let conversation = db.create_conversation("fixture").unwrap().id;
    append(&db, row(&conversation, "u1", "user", "first question", 1));
    tool_pair(&db, &conversation, true, 2);
    append(
        &db,
        row(&conversation, "a1", "assistant", "first answer", 3),
    );
    append(&db, row(&conversation, "u2", "user", "second question", 4));
    append(
        &db,
        row(&conversation, "a2", "assistant", "second answer", 5),
    );
    let changed = db
        .mutate_conversation_message(&conversation, "a1", false)
        .unwrap();
    assert_eq!(changed.removed_protocol_count, 2);
    let messages = db.get_conversation(&conversation).unwrap().messages;
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["u1", "u2", "a2"]
    );
    assert!(db
        .conversation_protocol_issue(&conversation)
        .unwrap()
        .is_none());
    assert_eq!(db.conversation_turns(&conversation).unwrap().len(), 2);
}

#[test]
fn deleting_a_user_does_not_delete_the_visible_answer_or_other_turns() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let conversation = db.create_conversation("fixture").unwrap().id;
    append(&db, row(&conversation, "u1", "user", "first question", 1));
    tool_pair(&db, &conversation, true, 2);
    append(
        &db,
        row(&conversation, "a1", "assistant", "first answer", 3),
    );
    append(&db, row(&conversation, "u2", "user", "second question", 4));
    db.mutate_conversation_message(&conversation, "u1", false)
        .unwrap();
    let messages = db.get_conversation(&conversation).unwrap().messages;
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a1", "u2"]
    );
    assert_eq!(db.conversation_turns(&conversation).unwrap().len(), 1);
}

#[test]
fn old_corrupt_prefix_requires_explicit_rollback_at_affected_user_and_is_not_silently_repaired() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let conversation = db.create_conversation("fixture").unwrap().id;
    append(
        &db,
        row(&conversation, "u1", "user", "original question", 1),
    );
    tool_pair(&db, &conversation, false, 2);
    append(
        &db,
        row(&conversation, "u2", "user", "repeated failed question", 3),
    );
    let issue = db
        .conversation_protocol_issue(&conversation)
        .unwrap()
        .unwrap();
    assert_eq!(issue.affected_user_message_id.as_deref(), Some("u1"));
    assert_eq!(
        db.mutate_conversation_message(&conversation, "u2", true)
            .err()
            .as_deref(),
        Some("history_protocol:u1")
    );
    assert_eq!(
        db.get_conversation(&conversation).unwrap().messages.len(),
        3
    );
    let rollback = db
        .mutate_conversation_message(&conversation, "u1", true)
        .unwrap();
    assert_eq!(rollback.draft_text.as_deref(), Some("original question"));
    assert!(db
        .get_conversation(&conversation)
        .unwrap()
        .messages
        .is_empty());
    assert!(db.conversation_turns(&conversation).unwrap().is_empty());
}

#[test]
fn rollback_preserves_other_fork_history_and_removes_stale_compaction() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let conversation = db.create_conversation("fixture").unwrap().id;
    append(&db, row(&conversation, "u1", "user", "question", 1));
    append(&db, row(&conversation, "a1", "assistant", "answer", 2));
    let branch = db
        .fork_message_canvas_conversation(&conversation, "a1")
        .unwrap();
    let branch_before = db.get_conversation(&branch).unwrap().messages.len();
    db.conn()
        .execute(
            "INSERT INTO settings(key,value) VALUES(?1,'{}')",
            [format!("conversation_compaction:{conversation}")],
        )
        .unwrap();
    db.mutate_conversation_message(&conversation, "u1", true)
        .unwrap();
    assert_eq!(
        db.get_conversation(&branch).unwrap().messages.len(),
        branch_before
    );
    assert!(db
        .load_conversation_compaction(&conversation)
        .unwrap()
        .is_none());
}
