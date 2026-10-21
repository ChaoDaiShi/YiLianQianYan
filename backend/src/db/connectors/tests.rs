use super::*;
use std::path::Path;
#[test]
fn connector_canvases_keep_layouts_independent_and_bindings_survive_deletion() {
    let path = std::env::temp_dir().join(format!("connector-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    db.initialize_connectors_schema().unwrap();
    db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('a','Agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    db.conn().execute("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('c','Conversation',1,1)",[]).unwrap();
    let a = db.create_connector_canvas("A", "a").unwrap();
    let b = db.create_connector_canvas("B", "a").unwrap();
    let c = db.create_connector_canvas("C", "a").unwrap();
    let layout = ConnectorLayout {
        nodes: vec![ConnectorPosition {
            id: "agent:a".into(),
            x: 93.,
            y: 41.,
        }],
        viewport: ConnectorViewport {
            x: 23.,
            y: 42.,
            zoom: 1.5,
        },
    };
    db.save_connector_layout(&a, &layout).unwrap();
    let binding = db
        .save_connector_binding(
            "a",
            &ConnectorBinding {
                id: String::new(),
                kind: "conversation_agent".into(),
                source_id: "conversation:c".into(),
                target_id: "agent:a".into(),
                service_kind: None,
                model_source: None,
            },
        )
        .unwrap();
    assert_eq!(
        db.connector_agent_for_conversation("c").unwrap().as_deref(),
        Some("a")
    );
    assert_eq!(
        db.save_connector_binding("a", &binding).unwrap().id,
        binding.id
    );
    assert!(db.delete_connector_canvas(&b).unwrap());
    assert!(db.connector_canvas(&b).unwrap().is_none());
    assert_eq!(
        db.connector_canvas(&a).unwrap().unwrap().2.viewport.zoom,
        1.5
    );
    drop(db);
    let db = Database::new(&path).unwrap();
    db.initialize_connectors_schema().unwrap();
    assert_eq!(db.list_connector_canvases(None).unwrap().len(), 2);
    assert_eq!(db.connector_canvas(&a).unwrap().unwrap().2.nodes[0].x, 93.);
    db.delete_connector_canvas(&a).unwrap();
    db.delete_connector_canvas(&c).unwrap();
    assert!(db.list_connector_canvases(None).unwrap().is_empty());
    assert_eq!(db.resolve_connector_bindings("a").unwrap().len(), 1);
    assert!(db
        .connector_database_resources()
        .unwrap()
        .iter()
        .any(|r| r.id == "conversation:c"));
    drop(db);
    let db = Database::new(&path).unwrap();
    assert!(db.list_connector_canvases(None).unwrap().is_empty());
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn connector_pool_bindings_use_existing_model_pool_selection() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('a','Agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    let pool = crate::db::model_pools::ModelPool {
        id: "pool-a".into(),
        label: "Pool".into(),
        service_kind: "chat".into(),
        enabled: true,
        revision: 1,
        attempt_timeout_ms: 10,
        total_timeout_ms: 10,
        tiers: vec![],
    };
    db.conn()
        .execute(
            "INSERT INTO model_pools VALUES(?1,1,?2)",
            params![pool.id, serde_json::to_string(&pool).unwrap()],
        )
        .unwrap();
    let binding = ConnectorBinding {
        id: String::new(),
        kind: "agent_model".into(),
        source_id: "agent:a".into(),
        target_id: "model:pool-a".into(),
        service_kind: Some("chat".into()),
        model_source: Some("pool".into()),
    };
    let saved = db.save_connector_binding("a", &binding).unwrap();
    assert_eq!(
        db.get_model_pool_binding("a").unwrap().as_deref(),
        Some("pool-a")
    );
    assert_eq!(db.resolve_connector_bindings("a").unwrap()[0].id, saved.id);
    assert_eq!(
        db.connector_runtime_bindings("a")
            .unwrap()
            .model_pool_id
            .as_deref(),
        Some("pool-a")
    );
    assert!(db.delete_connector_binding("a", &saved.id).unwrap());
    assert!(db.get_model_pool_binding("a").unwrap().is_none());
    assert!(db.resolve_connector_bindings("a").unwrap().is_empty());
}
#[test]
fn connector_service_pools_remain_independent_for_one_agent() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('a','Agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    let mut saved = Vec::new();
    for kind in ["chat", "vision", "image"] {
        let id = format!("{kind}-pool");
        let pool = crate::db::model_pools::ModelPool {
            id: id.clone(),
            label: kind.into(),
            service_kind: kind.into(),
            enabled: true,
            revision: 1,
            attempt_timeout_ms: 10,
            total_timeout_ms: 10,
            tiers: vec![],
        };
        db.conn()
            .execute(
                "INSERT INTO model_pools VALUES(?1,1,?2)",
                params![id, serde_json::to_string(&pool).unwrap()],
            )
            .unwrap();
        saved.push(
            db.save_connector_binding(
                "a",
                &ConnectorBinding {
                    id: String::new(),
                    kind: "agent_model".into(),
                    source_id: "agent:a".into(),
                    target_id: format!("model:{id}"),
                    service_kind: Some(kind.into()),
                    model_source: Some("pool".into()),
                },
            )
            .unwrap(),
        );
    }
    let bindings = db.resolve_connector_bindings("a").unwrap();
    assert_eq!(bindings.len(), 3);
    assert_eq!(
        db.get_model_pool_binding("a").unwrap().as_deref(),
        Some("chat-pool")
    );
    assert_eq!(
        db.connector_runtime_bindings("a")
            .unwrap()
            .model_pool_id
            .as_deref(),
        Some("chat-pool")
    );
    db.delete_connector_binding("a", &saved[2].id).unwrap();
    let bindings = db.resolve_connector_bindings("a").unwrap();
    assert_eq!(bindings.len(), 2);
    assert!(bindings
        .iter()
        .any(|b| b.service_kind.as_deref() == Some("vision")));
    assert_eq!(
        db.get_model_pool_binding("a").unwrap().as_deref(),
        Some("chat-pool")
    );
}

#[test]
fn connector_resource_delete_removes_only_its_bindings_layout_and_hidden_id() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('a','Agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    let canvas = db.create_connector_canvas("A", "a").unwrap();
    let memory = db
        .create_memory(&crate::db::CreateMemoryRequest {
            content: "selected".into(),
            category: "fact".into(),
            source: "manual".into(),
            source_conversation_id: None,
            metadata: None,
        })
        .unwrap();
    let node = format!("memory:{}", memory.id);
    db.save_connector_binding(
        "a",
        &ConnectorBinding {
            id: String::new(),
            kind: "agent_memory".into(),
            source_id: "agent:a".into(),
            target_id: node.clone(),
            service_kind: None,
            model_source: None,
        },
    )
    .unwrap();
    db.save_connector_layout(
        &canvas,
        &ConnectorLayout {
            nodes: vec![
                ConnectorPosition {
                    id: node.clone(),
                    x: 5.,
                    y: 6.,
                },
                ConnectorPosition {
                    id: "agent:a".into(),
                    x: 9.,
                    y: 10.,
                },
            ],
            ..Default::default()
        },
    )
    .unwrap();
    db.save_connector_display(
        &canvas,
        &ConnectorDisplay {
            hidden_node_ids: vec![node],
            ..Default::default()
        },
    )
    .unwrap();
    db.delete_memory(&memory.id).unwrap();
    assert!(db.resolve_connector_bindings("a").unwrap().is_empty());
    let (_, display, layout) = db.connector_canvas(&canvas).unwrap().unwrap();
    assert!(display.hidden_node_ids.is_empty());
    assert_eq!(layout.nodes.len(), 1);
    assert_eq!(layout.nodes[0].id, "agent:a");
}
#[test]
fn connector_layout_rejects_out_of_range_zoom() {
    let db = Database::new(Path::new(":memory:")).unwrap();
    let mut layout = ConnectorLayout::default();
    layout.viewport.zoom = 3.;
    assert!(db.save_connector_layout("missing", &layout).is_err());
}
