use super::*;

#[test]
fn branching_from_inherited_history_keeps_the_original_visible_group_anchor() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let root = db.create_conversation("Main path").unwrap().id;
    append(&db, &root, "original-u", "user", "First question", 1);
    append(&db, &root, "original-a", "assistant", "First answer", 2);
    append(
        &db,
        &root,
        "original-later",
        "user",
        "Keep this later question",
        3,
    );
    let branch = db
        .fork_message_canvas_conversation(&root, "original-a")
        .unwrap();
    append(&db, &branch, "branch-u", "user", "Branch question", 10);
    append(&db, &branch, "branch-a", "assistant", "Branch answer", 11);
    let nested = db
        .fork_message_canvas_conversation(&branch, "branch-a")
        .unwrap();
    let copied = db
        .get_conversation(&nested)
        .unwrap()
        .messages
        .into_iter()
        .find(|m| m.content == "First answer")
        .unwrap();
    assert_ne!(copied.id, "original-a");
    let alternate = db
        .fork_message_canvas_conversation(&nested, &copied.id)
        .unwrap();
    let overview = db.message_canvas_overview().unwrap();
    let entry = overview
        .conversations
        .iter()
        .find(|entry| entry.id == alternate)
        .unwrap();
    assert_eq!(entry.branch.as_ref().unwrap().parent_node_id, "original-a");
    assert!(overview
        .conversations
        .iter()
        .flat_map(|entry| &entry.nodes)
        .any(|node| node.id == entry.branch.as_ref().unwrap().parent_node_id));
    assert_eq!(db.get_conversation(&alternate).unwrap().messages.len(), 2);
    assert_eq!(db.get_conversation(&root).unwrap().messages.len(), 3);
    assert_eq!(db.get_conversation(&branch).unwrap().messages.len(), 4);
}
