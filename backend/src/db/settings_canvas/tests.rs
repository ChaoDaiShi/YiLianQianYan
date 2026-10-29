use super::*;

fn canvas(db: &Database, name: &str) -> String {
    db.conn().execute("INSERT OR IGNORE INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('settings-agent','Settings agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    db.create_connector_canvas(name, "settings-agent").unwrap()
}

fn saved_state() -> SettingsCanvasState {
    SettingsCanvasState {
        expanded_group_ids: vec!["voice".into()],
        connected_node_ids: vec!["tts".into(), "sandbox".into()],
        nodes: vec![SettingsCanvasPosition {
            id: "tts".into(),
            x: -240.,
            y: 128.,
        }],
        viewport: SettingsCanvasViewport {
            x: 32.,
            y: -16.,
            zoom: 1.25,
        },
    }
}

#[test]
fn settings_canvas_scopes_reopen_independently_without_changing_runtime_settings() {
    let path = std::env::temp_dir().join(format!("settings-canvas-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let a = canvas(&db, "A");
    let b = canvas(&db, "B");
    let config = crate::config::types::AppConfig::default();
    db.save_settings(&config).unwrap();
    let original_config = serde_json::to_value(db.get_settings().unwrap()).unwrap();
    let count = || {
        db.conn()
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get::<_, i64>(0))
            .unwrap()
    };
    let initial_count = count();
    assert_eq!(
        db.settings_canvas_state(None).unwrap(),
        Some(SettingsCanvasState::default())
    );
    assert_eq!(
        db.settings_canvas_state(Some(&a)).unwrap(),
        Some(SettingsCanvasState::default())
    );
    assert_eq!(initial_count, count(), "GET must not persist defaults");

    let global = saved_state();
    let mut scoped = saved_state();
    scoped.connected_node_ids = vec![];
    scoped.expanded_group_ids = vec!["security".into()];
    scoped.viewport.zoom = 0.5;
    assert!(db.save_settings_canvas_state(None, &global).unwrap());
    assert!(db.save_settings_canvas_state(Some(&a), &scoped).unwrap());
    assert_eq!(
        serde_json::to_value(db.get_settings().unwrap()).unwrap(),
        original_config
    );
    assert!(db
        .resolve_connector_bindings("settings-agent")
        .unwrap()
        .is_empty());
    drop(db);

    let db = Database::new(&path).unwrap();
    assert_eq!(db.settings_canvas_state(None).unwrap(), Some(global));
    assert_eq!(db.settings_canvas_state(Some(&a)).unwrap(), Some(scoped));
    assert_eq!(
        db.settings_canvas_state(Some(&b)).unwrap(),
        Some(SettingsCanvasState::default())
    );
    assert!(db.delete_connector_canvas(&a).unwrap());
    assert_eq!(db.settings_canvas_state(Some(&a)).unwrap(), None);
    assert!(!db
        .save_settings_canvas_state(Some(&a), &saved_state())
        .unwrap());
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key=?1",
                [key(Some(&a))],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn settings_canvas_rejects_unknown_duplicate_nonfinite_and_out_of_range_layout() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let valid = saved_state();
    db.save_settings_canvas_state(None, &valid).unwrap();
    let mut invalid_states = Vec::new();
    let mut state = valid.clone();
    state.connected_node_ids.push("unknown".into());
    invalid_states.push(state);
    let mut state = valid.clone();
    state.expanded_group_ids.push("unknown".into());
    invalid_states.push(state);
    let mut state = valid.clone();
    state.expanded_group_ids.push("voice".into());
    invalid_states.push(state);
    let mut state = valid.clone();
    state.connected_node_ids.push("tts".into());
    invalid_states.push(state);
    let mut state = valid.clone();
    state.nodes.push(state.nodes[0].clone());
    invalid_states.push(state);
    let mut state = valid.clone();
    state.nodes[0].id = "unknown".into();
    invalid_states.push(state);
    let mut state = valid.clone();
    state.nodes[0].x = f64::NAN;
    invalid_states.push(state);
    for zoom in [0.24, 2.01, f64::INFINITY] {
        let mut state = valid.clone();
        state.viewport.zoom = zoom;
        invalid_states.push(state);
    }
    let mut state = valid.clone();
    state.viewport.y = f64::NEG_INFINITY;
    invalid_states.push(state);
    for state in invalid_states {
        assert!(db.save_settings_canvas_state(None, &state).is_err());
        assert_eq!(db.settings_canvas_state(None).unwrap(), Some(valid.clone()));
    }
    for zoom in [0.25, 2.] {
        let mut boundary = valid.clone();
        boundary.viewport.zoom = zoom;
        assert!(boundary.validate().is_ok());
    }
}

#[test]
fn settings_canvas_catalog_is_complete_and_put_accepts_only_ui_state() {
    let presets = settings_canvas_presets().unwrap();
    let ids: HashSet<_> = presets.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(presets.len(), 15);
    assert_eq!(ids.len(), 15);
    assert!(ids.contains("model-pools"));
    for preset in &presets {
        assert!(!preset.label.is_empty() && !preset.description.is_empty());
        assert!(["left", "right", "top", "bottom"].contains(&preset.side.as_str()));
    }
    SettingsCanvasState::default().validate().unwrap();
    let mut body = serde_json::to_value(saved_state()).unwrap();
    body["api_key"] = "must-not-be-a-canvas-field".into();
    assert!(serde_json::from_value::<SettingsCanvasState>(body).is_err());
}

#[test]
fn legacy_settings_layout_defaults_to_collapsed_and_category_positions_are_ui_only() {
    let old = serde_json::json!({
        "connected_node_ids":["model","tts"],
        "nodes":[{"id":"model","x":600.,"y":300.}],
        "viewport":{"x":0.,"y":0.,"zoom":1.}
    });
    let mut state: SettingsCanvasState = serde_json::from_value(old).unwrap();
    assert!(state.expanded_group_ids.is_empty());
    state.expanded_group_ids = vec!["models".into(), "voice".into()];
    state.nodes.push(SettingsCanvasPosition {
        id: "group:models".into(),
        x: 300.,
        y: 200.,
    });
    state.validate().unwrap();
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    db.save_settings_canvas_state(None, &state).unwrap();
    assert_eq!(db.settings_canvas_state(None).unwrap(), Some(state.clone()));
    state.connected_node_ids.push("group:models".into());
    assert!(state.validate().is_err());
}
