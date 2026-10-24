use super::*;

#[test]
fn channel_robot_private_and_group_ids_have_distinct_sessions() {
    let server = server();
    let channel = configured(&server);
    let private = InboundMessage {
        event_id: "private-event".into(),
        external_message_id: "1".into(),
        peer_type: "private".into(),
        peer_id: "200".into(),
        sender_id: "200".into(),
        content: "private".into(),
        context_token: None,
    };
    let group = InboundMessage {
        event_id: "group-event".into(),
        external_message_id: "2".into(),
        peer_type: "group".into(),
        peer_id: "200".into(),
        sender_id: "201".into(),
        content: "group".into(),
        context_token: None,
    };
    let first = server
        .db
        .accept_channel_message(&channel, &private)
        .unwrap()
        .0;
    let second = server
        .db
        .accept_channel_message(&channel, &group)
        .unwrap()
        .0;
    assert_ne!(first.conversation_id, second.conversation_id);
    assert_eq!(second.peer_id, "200");
    assert_eq!(second.sender_id, "201");
    assert_eq!(second.peer_type, "group");
    assert_eq!(server.db.channel_sessions(&channel.id).unwrap().len(), 2);
    assert!(
        !server
            .db
            .accept_channel_message(&channel, &group)
            .unwrap()
            .1
    );
}
#[test]
fn channel_robot_policy_is_persisted_and_target_types_do_not_overlap() {
    let server = server();
    let channel = configured(&server);
    let mut policy = server.db.robot_policy(&channel.id).unwrap();
    assert!(policy.allows_send("private", "300"));
    assert!(!policy.allows_send("group", "300"));
    policy.send_private_ids.clear();
    server.db.save_robot_policy(&policy).unwrap();
    assert!(!server
        .db
        .robot_policy(&channel.id)
        .unwrap()
        .allows_send("private", "300"));
}
#[test]
fn channel_robot_migration_preserves_existing_private_session_and_origin_trigger() {
    let server = server();
    let channel = configured(&server);
    let inbound = InboundMessage {
        event_id: "before".into(),
        external_message_id: "1".into(),
        peer_type: "private".into(),
        peer_id: "200".into(),
        sender_id: "200".into(),
        content: "before migration".into(),
        context_token: None,
    };
    let original = server
        .db
        .accept_channel_message(&channel, &inbound)
        .unwrap()
        .0;
    server.db.conn().execute_batch("DROP TRIGGER channel_session_origin;
      CREATE TABLE old_sessions(id TEXT PRIMARY KEY,channel_id TEXT NOT NULL REFERENCES external_channels(id),peer_id TEXT NOT NULL,conversation_id TEXT NOT NULL REFERENCES conversations(id),created_at INTEGER NOT NULL,UNIQUE(channel_id,peer_id));
      INSERT INTO old_sessions SELECT id,channel_id,peer_id,conversation_id,created_at FROM channel_sessions;
      DROP TABLE channel_sessions;ALTER TABLE old_sessions RENAME TO channel_sessions;
      CREATE INDEX channel_sessions_conversation ON channel_sessions(conversation_id);
      CREATE TRIGGER channel_session_origin AFTER INSERT ON channel_sessions BEGIN INSERT OR REPLACE INTO settings(key,value) SELECT 'conversation_origin:'||NEW.conversation_id,json_object('kind','channel','provider',json_extract(payload,'$.provider'),'channel_name',json_extract(payload,'$.name')) FROM external_channels WHERE id=NEW.channel_id;END;
      ALTER TABLE channel_messages DROP COLUMN peer_type;ALTER TABLE channel_messages DROP COLUMN peer_id;
      DELETE FROM schema_migrations WHERE version=1063;").unwrap();
    server.db.initialize_channel_peers().unwrap();
    let stored = server
        .db
        .channel_message(&channel.id, &original.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.peer_type, "private");
    assert_eq!(stored.peer_id, "200");
    assert_eq!(stored.conversation_id, original.conversation_id);
    let next = InboundMessage {
        event_id: "after".into(),
        external_message_id: "2".into(),
        peer_type: "group".into(),
        peer_id: "200".into(),
        sender_id: "201".into(),
        content: "after migration".into(),
        context_token: None,
    };
    let grouped = server.db.accept_channel_message(&channel, &next).unwrap().0;
    assert_ne!(stored.conversation_id, grouped.conversation_id);
    let origins = server.db.conversation_origins().unwrap();
    assert_eq!(origins[&grouped.conversation_id]["kind"], "channel");
}

#[test]
fn channel_robot_atomic_action_claim_and_stopped_state_cannot_be_replayed() {
    let server = server();
    let channel = configured(&server);
    let conv = server.db.create_conversation("atomic action").unwrap();
    let mut run = RobotRun {
        account_id: "100".into(),
        id: "atomic-run".into(),
        channel_id: channel.id.clone(),
        mission_id: None,
        conversation_id: conv.id.clone(),
        user_message_id: None,
        origin: "desktop".into(),
        owner_id: None,
        goal: "atomic test".into(),
        preset_id: None,
        status: "running".into(),
        answer: None,
        error: None,
        created_at: 1,
        updated_at: 1,
    };
    server.db.create_robot_run(&run, None).unwrap();
    let action = RobotAction {
        account_id: "100".into(),
        id: "one-call".into(),
        run_id: Some(run.id.clone()),
        channel_id: channel.id.clone(),
        tool_name: "qq_send_private".into(),
        peer_type: Some("private".into()),
        peer_id: Some("300".into()),
        arguments: json!({"text":"test"}),
        result: None,
        status: "dispatching".into(),
        platform_message_id: None,
        error: None,
        created_at: 1,
        updated_at: 1,
    };
    let attempts = (0..4)
        .map(|_| {
            let db = server.db.clone_connection();
            let action = action.clone();
            std::thread::spawn(move || db.claim_robot_action(&action).unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        attempts
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|value| *value)
            .count(),
        1
    );
    assert!(server
        .db
        .robot_ambiguous_send(&run.id, "private", "300")
        .unwrap());
    run.status = "stopped".into();
    assert!(server.db.save_active_robot_run(&run).unwrap());
    run.status = "completed".into();
    assert!(!server.db.save_active_robot_run(&run).unwrap());
    let mut next = action.clone();
    next.id = "next-call".into();
    assert!(server.db.claim_robot_action(&next).is_err());
    assert_eq!(
        server.db.robot_run(&run.id).unwrap().unwrap().status,
        "stopped"
    );
    server.db.delete_conversation(&conv.id).unwrap();
    assert!(server.db.robot_run(&run.id).unwrap().is_none());
    assert!(server.db.robot_policy(&channel.id).unwrap().enabled);
}
