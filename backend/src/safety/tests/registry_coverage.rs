use crate::tools::registry::ToolRegistry;

fn sample_args(tool_name: &str) -> serde_json::Value {
    match tool_name {
        "bash" => serde_json::json!({"command": "pwd"}),
        "read_file" => serde_json::json!({"path": "README.md"}),
        "write_file" => serde_json::json!({"path": "notes.txt", "content": "sample"}),
        "edit_file" => serde_json::json!({
            "path": "notes.txt",
            "old_text": "before",
            "new_text": "after"
        }),
        "grep" => serde_json::json!({"pattern": "SafetyPolicy", "path": "."}),
        "glob" => serde_json::json!({"pattern": "**/*.rs", "path": "."}),
        "http_request" => serde_json::json!({"url": "https://example.com"}),
        "load_skill" => serde_json::json!({"name": "example"}),
        "write_todos" => serde_json::json!({"todos": []}),
        "process" => serde_json::json!({"action": "list"}),
        "mouse" => serde_json::json!({"action": "move", "x": 1, "y": 1}),
        "keyboard" => serde_json::json!({"action": "press", "key": "enter"}),
        "screenshot" => serde_json::json!({}),
        "upscale_image" => serde_json::json!({"path": "image.png"}),
        "windows_list" => serde_json::json!({"limit": 10}),
        "ui_inspect" => serde_json::json!({"max_depth": 2, "max_nodes": 10}),
        "ui_find" => serde_json::json!({"selector": {"name": "Save"}}),
        "windows_focus" => serde_json::json!({"name": "Notepad"}),
        "ui_invoke" => serde_json::json!({"selector": {"name": "Save"}}),
        "ui_set_value" => serde_json::json!({
            "selector": {"automation_id": "editor"},
            "value": "sample"
        }),
        other => panic!("add security sample arguments for newly registered tool {other}"),
    }
}

#[test]
fn every_default_registered_tool_has_valid_security_metadata() {
    let registry = ToolRegistry::with_defaults(".");

    for tool in registry.all() {
        let args = sample_args(tool.name());
        let descriptor = tool
            .security_descriptor(&args)
            .unwrap_or_else(|error| panic!("tool {}: {error}", tool.name()));

        descriptor
            .validate()
            .unwrap_or_else(|error| panic!("tool {}: {error}", tool.name()));
        assert_eq!(descriptor.tool_name, tool.name());
    }
}
