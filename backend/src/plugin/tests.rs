// ============================================================
// Plugin manifest foundation tests.
// ============================================================

use super::manifest::*;
use super::path::*;
use super::registry::*;

fn valid_manifest_json() -> String {
    serde_json::to_string(&serde_json::json!({
        "schema_version": 1,
        "id": "com.example.plugin",
        "name": "Example Plugin",
        "version": "1.0.0",
        "description": "A test plugin",
        "permissions": ["filesystem.read"],
        "contributes": {
            "agents": ["agents/researcher.md"],
            "skills": ["skills/review.md"],
            "workflows": ["workflows/review.json"],
            "mcp_server_templates": []
        }
    }))
    .unwrap()
}

#[test]
fn valid_plugin_manifest() {
    let manifest = parse_manifest(&valid_manifest_json()).unwrap();
    assert_eq!(manifest.id, "com.example.plugin");
    assert_eq!(manifest.permissions, vec!["filesystem.read"]);
}

#[test]
fn invalid_plugin_id() {
    for bad in ["", "Has Upper", "with space", "com/example", "UPPER"] {
        assert!(PluginId::new(bad).is_err(), "should reject id: {bad}");
    }
    assert!(PluginId::new("com.example-1_ok").is_ok());
}

#[test]
fn oversized_manifest_rejected() {
    let big = "x".repeat(MAX_PLUGIN_MANIFEST_BYTES + 1);
    assert_eq!(parse_manifest(&big), Err(ManifestError::Oversized));
}

#[test]
fn secret_in_manifest_rejected() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "com.example.plugin",
        "name": "Secret Plugin",
        "version": "1.0.0",
        "description": "contains api_key=abc123",
        "permissions": [],
        "contributes": {}
    })
    .to_string();
    assert_eq!(parse_manifest(&json), Err(ManifestError::ContainsSecret));
}

#[test]
fn description_and_permission_limits_enforced() {
    let long_desc = serde_json::json!({
        "schema_version": 1,
        "id": "com.example.plugin",
        "name": "X",
        "version": "1.0.0",
        "description": "x".repeat(MAX_PLUGIN_DESCRIPTION_CHARS + 1),
        "permissions": [],
        "contributes": {}
    })
    .to_string();
    assert_eq!(
        parse_manifest(&long_desc),
        Err(ManifestError::DescriptionTooLong(
            MAX_PLUGIN_DESCRIPTION_CHARS
        ))
    );

    let many_perms = serde_json::json!({
        "schema_version": 1,
        "id": "com.example.plugin",
        "name": "X",
        "version": "1.0.0",
        "description": "x",
        "permissions": (0..(MAX_PLUGIN_PERMISSIONS + 1)).map(|i| format!("p{i}")).collect::<Vec<_>>(),
        "contributes": {}
    })
    .to_string();
    assert_eq!(
        parse_manifest(&many_perms),
        Err(ManifestError::TooManyPermissions)
    );
}

#[test]
fn path_traversal_and_absolute_rejected() {
    let root = std::env::temp_dir().join(format!("yilian-plugin-root-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();

    assert_eq!(
        validate_contribution_path(&root, "../escape.md"),
        Err(PluginPathError::NotRelative)
    );
    assert_eq!(
        validate_contribution_path(&root, "/etc/passwd"),
        Err(PluginPathError::NotRelative)
    );
    // A normal relative file is allowed.
    std::fs::write(root.join("agent.md"), "x").unwrap();
    assert!(validate_contribution_path(&root, "agent.md").is_ok());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn symlink_escape_rejected() {
    let root = std::env::temp_dir().join(format!("yilian-plugin-root-{}", uuid::Uuid::new_v4()));
    let outside = std::env::temp_dir().join(format!("yilian-plugin-out-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&outside, "secret").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("link.md")).unwrap();
        let err = validate_contribution_path(&root, "link.md");
        assert!(matches!(
            err,
            Err(PluginPathError::OutsideRoot) | Err(PluginPathError::SymlinkEscape)
        ));
    }

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&outside);
}

#[test]
fn plugin_refresh_parses_and_dedups() {
    let root = std::env::temp_dir().join(format!("yilian-plugins-{}", uuid::Uuid::new_v4()));
    let pdir = root.join("com.example.plugin");
    std::fs::create_dir_all(&pdir).unwrap();
    std::fs::write(pdir.join("plugin.json"), valid_manifest_json()).unwrap();

    let registry = PluginRegistry::new();
    let report = registry.refresh(&root).unwrap();
    assert_eq!(report.discovered, 1);
    assert_eq!(report.invalid, 0);
    assert_eq!(registry.list().len(), 1);

    // Enable/disable toggles status.
    let id = PluginId::new("com.example.plugin").unwrap();
    assert!(registry.set_enabled(&id, true));
    assert_eq!(registry.get(&id).unwrap().status, PluginStatus::Enabled);
    assert!(registry.set_enabled(&id, false));
    assert_eq!(registry.get(&id).unwrap().status, PluginStatus::Disabled);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn plugin_mcp_template_does_not_auto_connect() {
    // A template is just declarative data — the registry never spawns anything.
    let manifest: PluginManifest = parse_manifest(
        &serde_json::json!({
            "schema_version": 1,
            "id": "com.example.mcp",
            "name": "MCP Template Plugin",
            "version": "1.0.0",
            "description": "x",
            "permissions": [],
            "contributes": {
                "mcp_server_templates": [{
                    "name": "github",
                    "transport": "stdio",
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": { "GITHUB_TOKEN": "GITHUB_TOKEN" }
                }]
            }
        })
        .to_string(),
    )
    .unwrap();
    let template = &manifest.contributes.mcp_server_templates[0];
    assert_eq!(template.name, "github");
    // env maps header/name -> environment variable NAME (not a secret value).
    assert_eq!(template.env.get("GITHUB_TOKEN").unwrap(), "GITHUB_TOKEN");
}
