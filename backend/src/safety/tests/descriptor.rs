use crate::safety::{
    describe_builtin_tool, DescriptorError, PermissionId, ResourceDescriptor, SideEffectKind,
};
use crate::tools::trait_def::RiskLevel;

fn permissions_for(tool: &str, args: serde_json::Value) -> Vec<PermissionId> {
    describe_builtin_tool(tool, &args)
        .unwrap()
        .requested_permissions
        .into_iter()
        .map(|requested| requested.permission)
        .collect()
}

#[test]
fn process_list_and_kill_have_different_permissions_and_risk() {
    let list = describe_builtin_tool("process", &serde_json::json!({"action": "list"})).unwrap();
    let kill = describe_builtin_tool("process", &serde_json::json!({"action": "kill", "pid": 42}))
        .unwrap();

    assert_eq!(
        list.requested_permissions[0].permission,
        PermissionId::ProcessInspect
    );
    assert_eq!(list.default_risk, RiskLevel::Low);
    assert_eq!(
        kill.requested_permissions[0].permission,
        PermissionId::ProcessControl
    );
    assert_eq!(kill.default_risk, RiskLevel::High);
}

#[test]
fn process_kill_requires_an_explicit_pid() {
    assert!(matches!(
        describe_builtin_tool("process", &serde_json::json!({"action": "kill"})),
        Err(DescriptorError::MissingArgument {
            argument: "pid",
            ..
        })
    ));
}

#[test]
fn mutating_http_methods_are_high_risk() {
    let get = describe_builtin_tool(
        "http_request",
        &serde_json::json!({"method": "GET", "url": "https://example.com"}),
    )
    .unwrap();
    let post = describe_builtin_tool(
        "http_request",
        &serde_json::json!({"method": "POST", "url": "https://example.com"}),
    )
    .unwrap();

    assert_eq!(get.default_risk, RiskLevel::Medium);
    assert_eq!(post.default_risk, RiskLevel::High);
}

#[test]
fn image_upscale_declares_file_and_network_side_effects() {
    let descriptor =
        describe_builtin_tool("upscale_image", &serde_json::json!({"path": "image.png"})).unwrap();
    let permissions: Vec<_> = descriptor
        .requested_permissions
        .iter()
        .map(|requested| requested.permission)
        .collect();

    assert_eq!(
        permissions,
        vec![
            PermissionId::FilesystemRead,
            PermissionId::FilesystemWrite,
            PermissionId::NetworkRequest,
        ]
    );
    assert!(descriptor
        .side_effects
        .contains(&SideEffectKind::FileMutation));
    assert!(descriptor
        .side_effects
        .contains(&SideEffectKind::NetworkEgress));
}

#[test]
fn desktop_observation_and_interaction_are_distinct() {
    assert_eq!(
        permissions_for("screenshot", serde_json::json!({})),
        vec![PermissionId::DesktopObserve]
    );
    assert_eq!(
        permissions_for(
            "ui_invoke",
            serde_json::json!({"selector": {"name": "Save"}})
        ),
        vec![PermissionId::DesktopInteract]
    );

    let generic_mouse = describe_builtin_tool(
        "mouse",
        &serde_json::json!({"action": "click", "x": 10, "y": 20}),
    )
    .unwrap();
    let invoke = describe_builtin_tool(
        "ui_invoke",
        &serde_json::json!({"selector": {"name": "Save"}}),
    )
    .unwrap();
    assert_eq!(generic_mouse.default_risk, RiskLevel::High);
    assert_eq!(invoke.default_risk, RiskLevel::High);
}

#[test]
fn descriptor_requires_action_specific_arguments() {
    assert!(matches!(
        describe_builtin_tool("bash", &serde_json::json!({})),
        Err(DescriptorError::MissingArgument {
            argument: "command",
            ..
        })
    ));
    assert!(matches!(
        describe_builtin_tool("process", &serde_json::json!({"action": "restart"})),
        Err(DescriptorError::InvalidAction { .. })
    ));
}

#[test]
fn unknown_tool_fails_closed() {
    assert!(matches!(
        describe_builtin_tool("unknown_tool", &serde_json::json!({})),
        Err(DescriptorError::UnknownTool(name)) if name == "unknown_tool"
    ));
}

#[test]
fn file_descriptor_keeps_runtime_target() {
    let descriptor = describe_builtin_tool(
        "write_file",
        &serde_json::json!({"path": "notes.txt", "content": "secret-free sample"}),
    )
    .unwrap();

    assert!(matches!(
        descriptor.resources.as_slice(),
        [ResourceDescriptor::File { path }] if path == "notes.txt"
    ));
}
