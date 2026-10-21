use super::*;

#[test]
fn former_frame_and_relative_message_coordinates_project_to_absolute_message_positions() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let source = db.create_conversation("Layout source").unwrap().id;
    append(&db, &source, "layout-u", "user", "Question", 1);
    append(&db, &source, "layout-a", "assistant", "Answer", 2);
    let old = ConnectorLayout {
        nodes: vec![
            super::super::connectors::ConnectorPosition {
                id: source.clone(),
                x: 500.,
                y: 700.,
            },
            super::super::connectors::ConnectorPosition {
                id: "layout-u".into(),
                x: 34.,
                y: 58.,
            },
        ],
        viewport: super::super::connectors::ConnectorViewport {
            x: 10.,
            y: 20.,
            zoom: 0.8,
        },
    };
    db.save_message_canvas_layout(&old).unwrap();
    let projected = db.message_canvas_overview().unwrap().layout;
    assert_eq!(projected.nodes.len(), 2);
    assert_eq!(projected.nodes[0].id, "layout-u");
    assert_eq!((projected.nodes[0].x, projected.nodes[0].y), (534., 758.));
    assert_eq!((projected.nodes[1].x, projected.nodes[1].y), (524., 896.));
    assert_eq!(projected.viewport.zoom, 0.8);
    db.save_message_canvas_layout(&projected).unwrap();
    let canonical = db.message_canvas_overview().unwrap().layout;
    assert_eq!(canonical.nodes[0].x, 534.);
    assert_eq!(canonical.nodes[1].y, 896.);
    assert!(canonical.nodes.iter().all(|point| point.id != source));
}

#[test]
fn canvas_exposes_only_input_and_output_while_protocol_process_and_reasoning_survive_in_history() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let source = db.create_conversation("Process source").unwrap().id;
    append(&db, &source, "process-u", "user", "Question", 1);
    db.add_message(&MessageRow {
        id: "process-call".into(),
        conversation_id: source.clone(),
        role: "assistant".into(),
        content: "tool phase text must not masquerade as an output".into(),
        tool_calls: Some(
            r#"[{"id":"call","type":"function","function":{"name":"read_file","arguments":"{}"}}]"#
                .into(),
        ),
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: 2,
    })
    .unwrap();
    db.add_message(&MessageRow {
        id: "process-result".into(),
        conversation_id: source.clone(),
        role: "tool".into(),
        content: "actual tool result".into(),
        tool_calls: None,
        tool_call_id: Some("call".into()),
        tool_name: Some("read_file".into()),
        tool_result: None,
        created_at: 3,
    })
    .unwrap();
    append(&db, &source, "process-a", "assistant", "Answer", 4);
    db.conn().execute("INSERT INTO settings(key,value) VALUES('chat_turn_reasoning:process-u','actual provider reasoning')", []).unwrap();
    let overview = db.message_canvas_overview().unwrap();
    let nodes = &overview.conversations[0].nodes;
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["process-u", "process-a"]
    );
    assert_eq!(nodes[1].role, "assistant");
    assert_eq!(nodes[1].parent_id.as_deref(), Some("process-u"));
    let history = db.get_conversation(&source).unwrap();
    assert_eq!(history.messages.len(), 4);
    assert_eq!(history.messages[1].id, "process-call");
    assert_eq!(history.messages[2].id, "process-result");
    assert_eq!(
        nodes[0].reasoning.as_deref(),
        Some("actual provider reasoning")
    );
    let fork = db
        .fork_message_canvas_conversation(&source, "process-a")
        .unwrap();
    assert_eq!(db.get_conversation(&fork).unwrap().messages.len(), 4);
    append(&db, &fork, "branch-visible-u", "user", "Branch question", 5);
    append(
        &db,
        &fork,
        "branch-visible-a",
        "assistant",
        "Branch answer",
        6,
    );
    let fork_overview = db.message_canvas_overview().unwrap();
    let fork_nodes = &fork_overview
        .conversations
        .iter()
        .find(|conversation| conversation.id == fork)
        .unwrap()
        .nodes;
    assert_eq!(fork_nodes.len(), 2);
    assert_eq!(fork_nodes[0].parent_id.as_deref(), Some("process-a"));
    let copied_user = db.get_conversation(&fork).unwrap().messages[0].id.clone();
    let copied_reasoning: String = db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [format!("chat_turn_reasoning:{copied_user}")],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(copied_reasoning, "actual provider reasoning");
}

#[test]
fn message_canvas_overview_reads_unbound_history_without_creating_graphs_or_bindings() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let conversation = db.create_conversation("Homepage conversation").unwrap().id;
    append(&db, &conversation, "overview-user", "user", "Question", 1);
    append(
        &db,
        &conversation,
        "overview-answer",
        "assistant",
        "Answer",
        2,
    );
    let overview = db.message_canvas_overview().unwrap();
    assert_eq!(overview.conversations.len(), 1);
    assert_eq!(overview.conversations[0].id, conversation);
    assert_eq!(overview.conversations[0].nodes.len(), 2);
    assert_eq!(
        overview.conversations[0].nodes[1].parent_id.as_deref(),
        Some("overview-user")
    );
    for table in [
        "connector_canvases",
        "connector_bindings",
        "connector_message_graphs",
        "message_canvas_branches",
    ] {
        let count: i64 = db
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "read-only overview mutated {table}");
    }
}

#[test]
fn overview_branch_preserves_protocol_history_and_unbound_execution_without_cloned_visible_nodes() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let source = db.create_conversation("Source").unwrap().id;
    append(&db, &source, "overview-u1", "user", "Question", 1);
    append(&db, &source, "overview-a1", "assistant", "Answer", 2);
    append(&db, &source, "overview-u2", "user", "Not in branch", 3);
    db.conn().execute("INSERT INTO settings(key,value) VALUES('chat_reasoning:overview-a1','retained reasoning')", []).unwrap();
    db.conn()
        .execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2)",
            params![
                format!("chat_options:{source}"),
                r#"{"model_profile_id":"source-profile","thinking_mode":"standard"}"#
            ],
        )
        .unwrap();
    assert!(db
        .fork_message_canvas_conversation(&source, "overview-u1")
        .is_err());
    let branch = db
        .fork_message_canvas_conversation(&source, "overview-a1")
        .unwrap();
    assert!(db
        .connector_agent_for_conversation(&source)
        .unwrap()
        .is_none());
    assert!(db
        .connector_agent_for_conversation(&branch)
        .unwrap()
        .is_none());
    assert_eq!(db.get_conversation(&branch).unwrap().messages.len(), 2);
    assert_eq!(
        db.load_chat_options(&branch)
            .unwrap()
            .model_profile_id
            .as_deref(),
        Some("source-profile")
    );
    assert_eq!(
        db.load_chat_messages(&branch).unwrap()[1]
            .reasoning_content
            .as_deref(),
        Some("retained reasoning")
    );
    append(
        &db,
        &branch,
        "overview-branch-u",
        "user",
        "Branch question",
        4,
    );
    append(
        &db,
        &branch,
        "overview-branch-a",
        "assistant",
        "Branch answer",
        5,
    );
    let nested = db
        .fork_message_canvas_conversation(&branch, "overview-branch-a")
        .unwrap();
    assert_eq!(db.get_conversation(&nested).unwrap().messages.len(), 4);
    let overview = db.message_canvas_overview().unwrap();
    let child = overview
        .conversations
        .iter()
        .find(|group| group.id == branch)
        .unwrap();
    assert_eq!(child.nodes.len(), 2);
    assert_eq!(child.nodes[0].parent_id.as_deref(), Some("overview-a1"));
    assert_eq!(child.branch.as_ref().unwrap().parent_node_id, "overview-a1");
    assert_eq!(db.get_conversation(&source).unwrap().messages.len(), 3);
}

#[test]
fn grouped_layout_and_message_positions_survive_database_reopen() {
    let path = std::env::temp_dir().join(format!("message-overview-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let layout = ConnectorLayout {
        nodes: vec![
            super::super::connectors::ConnectorPosition {
                id: "conversation-id".into(),
                x: 400.,
                y: 80.,
            },
            super::super::connectors::ConnectorPosition {
                id: "message-id".into(),
                x: 24.,
                y: 190.,
            },
        ],
        viewport: super::super::connectors::ConnectorViewport {
            x: 70.,
            y: -200.,
            zoom: 0.7,
        },
    };
    db.save_message_canvas_layout(&layout).unwrap();
    drop(db);
    let db = Database::new(&path).unwrap();
    let saved = db.message_canvas_overview().unwrap().layout;
    assert_eq!(saved.nodes.len(), 2);
    assert_eq!(saved.nodes[0].x, 400.);
    assert_eq!(saved.nodes[1].y, 190.);
    assert_eq!(saved.viewport.zoom, 0.7);
}

#[test]
fn legacy_branches_keep_group_ancestry_after_saved_canvas_is_deleted() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let (canvas, source) = seed(&db);
    append(&db, &source, "legacy-u", "user", "Question", 1);
    append(&db, &source, "legacy-a", "assistant", "Answer", 2);
    db.create_connector_message_graph(&canvas, &source).unwrap();
    let branch = db
        .fork_connector_message(&canvas, &source, Some("legacy-a"))
        .unwrap();
    append(&db, &branch, "legacy-child-u", "user", "Branch", 3);
    db.delete_connector_canvas(&canvas).unwrap();
    let overview = db.message_canvas_overview().unwrap();
    let child = overview
        .conversations
        .iter()
        .find(|group| group.id == branch)
        .unwrap();
    assert_eq!(child.nodes.len(), 1);
    assert_eq!(child.nodes[0].parent_id.as_deref(), Some("legacy-a"));
    assert_eq!(
        db.connector_agent_for_conversation(&branch)
            .unwrap()
            .as_deref(),
        Some("branch-agent")
    );
}

fn seed(db: &Database) -> (String, String) {
    db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('branch-agent','Agent','','','[]','[]',10,1,'database',1,1)",[]).unwrap();
    let canvas = db
        .create_connector_canvas("Branches", "branch-agent")
        .unwrap();
    let root = db.create_conversation("Source").unwrap().id;
    (canvas, root)
}

fn append(db: &Database, conversation: &str, id: &str, role: &str, content: &str, time: i64) {
    db.add_message(&MessageRow {
        id: id.into(),
        conversation_id: conversation.into(),
        role: role.into(),
        content: content.into(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_result: None,
        created_at: time,
    })
    .unwrap();
}

#[test]
fn sibling_and_nested_branches_copy_only_ancestors_and_survive_reopen() {
    let path = std::env::temp_dir().join(format!("connector-message-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let (canvas, root) = seed(&db);
    append(&db, &root, "u1", "user", "root question", 100);
    append(&db, &root, "a1", "assistant", "root answer", 101);
    append(&db, &root, "u2", "user", "source sibling excluded", 102);
    append(
        &db,
        &root,
        "a2",
        "assistant",
        "source sibling answer excluded",
        103,
    );
    let source_before = db.get_conversation(&root).unwrap().messages;
    db.create_connector_message_graph(&canvas, &root).unwrap();
    assert!(db
        .fork_connector_message(&canvas, &root, Some("u1"))
        .is_err());
    assert!(db.fork_connector_message(&canvas, &root, None).is_err());
    let left = db
        .fork_connector_message(&canvas, &root, Some("a1"))
        .unwrap();
    let right = db
        .fork_connector_message(&canvas, &root, Some("a1"))
        .unwrap();
    for fork in [&left, &right] {
        let copied = db.get_conversation(fork).unwrap().messages;
        assert_eq!(
            copied
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>(),
            vec!["root question", "root answer"]
        );
        assert!(copied.iter().all(|m| m.id != "u1" && m.id != "a1"));
        assert_eq!(
            db.connector_agent_for_conversation(fork)
                .unwrap()
                .as_deref(),
            Some("branch-agent")
        );
    }
    append(&db, &left, "left-u", "user", "left question", 104);
    append(&db, &left, "left-a", "assistant", "left answer", 105);
    append(&db, &right, "right-u", "user", "right question", 106);
    append(&db, &right, "right-a", "assistant", "right answer", 107);
    let nested = db
        .fork_connector_message(&canvas, &root, Some("left-a"))
        .unwrap();
    let sources = db.connector_message_sources(&canvas).unwrap();
    assert_eq!(sources.conversations.len(), 1);
    assert_eq!(sources.conversations[0].id, root);
    assert_eq!(sources.graphs.len(), 1);
    assert_eq!(sources.graphs[0].conversation_id, root);
    assert!(db.connector_message_sources("missing-canvas").is_err());
    assert_eq!(
        db.get_conversation(&nested)
            .unwrap()
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        vec![
            "root question",
            "root answer",
            "left question",
            "left answer"
        ]
    );
    db.select_connector_message(&canvas, &root, Some("left-a"))
        .unwrap();
    let mut layout = ConnectorLayout::default();
    layout.viewport.x = 83.;
    db.save_connector_message_layout(&canvas, &root, &layout)
        .unwrap();
    db.set_conversation_run_status(
        &right,
        super::super::ConversationRunStatus::Failed,
        Some("provider rejected"),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&db.get_conversation(&root).unwrap().messages).unwrap(),
        serde_json::to_value(source_before).unwrap()
    );
    drop(db);
    let db = Database::new(&path).unwrap();
    let graph = db.connector_message_graph(&canvas, &root).unwrap();
    assert_eq!(graph.nodes.len(), 8);
    assert_eq!(graph.layout.viewport.x, 83.);
    assert_eq!(graph.selected_node_id.as_deref(), Some("left-a"));
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|n| n.id == "left-u")
            .unwrap()
            .parent_id
            .as_deref(),
        Some("a1")
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|n| n.id == "right-u")
            .unwrap()
            .parent_id
            .as_deref(),
        Some("a1")
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|n| n.id == "right-a")
            .unwrap()
            .status,
        "failed"
    );
    db.delete_connector_canvas(&canvas).unwrap();
    assert!(db.connector_message_graph(&canvas, &root).is_err());
    assert!(db.get_conversation(&left).is_ok());
    assert!(db.get_conversation(&root).is_ok());
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn protocol_prefix_is_complete_and_parent_deletion_prunes_descendant_metadata() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let (canvas, root) = seed(&db);
    append(&db, &root, "user", "user", "question", 100);
    let mut call=MessageRow { id:"call".into(),conversation_id:root.clone(),role:"assistant".into(),content:String::new(),tool_calls:Some(r#"[{"id":"call-1","type":"function","function":{"name":"read_file","arguments":"{}"}}]"#.into()),tool_call_id:None,tool_name:None,tool_result:None,created_at:101 };
    db.add_message(&call).unwrap();
    call.id = "tool".into();
    call.role = "tool".into();
    call.tool_calls = None;
    call.tool_call_id = Some("call-1".into());
    call.content = "result".into();
    call.created_at = 102;
    db.add_message(&call).unwrap();
    append(&db, &root, "answer", "assistant", "answer", 103);
    let graph = db.create_connector_message_graph(&canvas, &root).unwrap();
    assert_eq!(graph.nodes.len(), 2);
    let fork = db
        .fork_connector_message(&canvas, &root, Some("answer"))
        .unwrap();
    let prefix = db.get_conversation(&fork).unwrap().messages;
    assert_eq!(prefix.len(), 4);
    assert_eq!(prefix[2].tool_call_id.as_deref(), Some("call-1"));
    assert!(prefix[1].tool_calls.as_ref().unwrap().contains("call-1"));
    append(&db, &fork, "fork-u", "user", "branch", 105);
    append(&db, &fork, "fork-a", "assistant", "branch answer", 106);
    let nested = db
        .fork_connector_message(&canvas, &root, Some("fork-a"))
        .unwrap();
    db.select_connector_message(&canvas, &root, Some("fork-a"))
        .unwrap();
    db.conn()
        .execute("DELETE FROM messages WHERE id='answer'", [])
        .unwrap();
    let graph = db.connector_message_graph(&canvas, &root).unwrap();
    assert!(graph.selected_node_id.is_none());
    assert!(graph
        .runs
        .iter()
        .all(|r| r.conversation_id != fork && r.conversation_id != nested));
    assert!(db.get_conversation(&fork).is_ok());
}
