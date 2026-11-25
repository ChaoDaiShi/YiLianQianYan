use super::*;

fn profile(id: &str, group: &str, level: Level) -> Profile {
    Profile {
        description: String::new(),
        id: id.into(),
        name: id.into(),
        level,
        tool_group_ids: [group.to_owned()].into(),
        skill_ids: None,
        workflow_ids: BTreeSet::new(),
        plugin_ids: BTreeSet::new(),
        agent_id: None,
    }
}

#[test]
fn skill_selection_distinguishes_legacy_all_from_an_explicit_empty_list() {
    let legacy = serde_json::json!({
        "id": "profile", "name": "profile", "level": "user",
        "tool_group_ids": ["system-planning"], "workflow_ids": [], "plugin_ids": []
    });
    let profile: Profile = serde_json::from_value(legacy).unwrap();
    let mut selection = Selection {
        revision: 1,
        profile,
        custom_tools: BTreeSet::new(),
    };
    let mut skill = AvailableSkill {
        id: "research".into(),
        name: "研究".into(),
        plugin_id: None,
    };
    assert!(selection.allows_skill(&skill));
    selection.profile.skill_ids = Some(BTreeSet::new());
    assert!(!selection.allows_skill(&skill));
    selection.profile.skill_ids = Some([skill.id.clone()].into());
    assert!(selection.allows_skill(&skill));
    skill.plugin_id = Some("plugin".into());
    assert!(!selection.allows_skill(&skill));
    selection.profile.plugin_ids.insert("plugin".into());
    assert!(selection.allows_skill(&skill));
}

#[test]
fn harness_revisions_keep_the_original_tool_group_after_switch_and_edit() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let mut value = settings(&db).unwrap();
    value.groups.push(ToolGroup {
        id: "custom".into(),
        name: "文件组".into(),
        level: Level::User,
        tools: ["read_file".into()].into(),
    });
    value.profiles.push(profile("first", "custom", Level::User));
    value
        .profiles
        .push(profile("second", "system-browser", Level::System));
    value.active_id = "first".into();
    let mut value = save(&db, value).unwrap();
    let first = active(&db).unwrap().unwrap();
    assert!(first.allows_tool("read_file"));
    assert!(!first.allows_tool("write_file"));
    let mut manifest = ContextManifest::default();
    record(&mut manifest, Some(&first));

    value.active_id = "second".into();
    value.groups[0].tools = ["write_file".into()].into();
    let value = save(&db, value).unwrap();
    assert_eq!(value.revision, 2);
    let second = active(&db).unwrap().unwrap();
    assert!(second.allows_tool("open_url"));
    assert!(!second.allows_tool("read_file"));
    let original = from_manifest(&db, Some(&manifest)).unwrap().unwrap();
    assert_eq!(original.revision, 1);
    assert!(original.allows_tool("read_file"));
    assert!(!original.allows_tool("write_file"));
    assert!(save(&db, Settings::default()).is_err());
}

#[test]
fn system_harness_cannot_reference_a_user_tool_group() {
    let mut value = Settings::default();
    value.groups.push(ToolGroup {
        id: "user-tools".into(),
        name: "用户组".into(),
        level: Level::User,
        tools: ["glob".into()].into(),
    });
    value
        .profiles
        .push(profile("system-profile", "user-tools", Level::System));
    let tools = ["glob".into()].into();
    assert!(validate(
        &value,
        &tools,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new()
    )
    .is_err());
    value.profiles[0].level = Level::User;
    validate(
        &value,
        &tools,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
    )
    .unwrap();
}
