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
fn visible_gui_launches_bind_exact_network_and_desktop_targets() {
    let url = describe_builtin_tool(
        "open_url",
        &serde_json::json!({"url": "https://www.bilibili.com/"}),
    )
    .unwrap();
    assert_eq!(
        url.requested_permissions
            .iter()
            .map(|requested| requested.permission)
            .collect::<Vec<_>>(),
        vec![PermissionId::NetworkRequest, PermissionId::DesktopInteract]
    );
    assert_eq!(url.default_risk, RiskLevel::High);
    assert!(matches!(
        url.resources.as_slice(),
        [
            ResourceDescriptor::Network { url, method },
            ResourceDescriptor::Desktop { action, target: Some(target) },
        ] if url == "https://www.bilibili.com/"
            && method == "GET"
            && action == "open_url"
            && target == url
    ));

    let app = describe_builtin_tool(
        "open_application",
        &serde_json::json!({"application": "QQ"}),
    )
    .unwrap();
    assert_eq!(
        app.requested_permissions
            .iter()
            .map(|requested| requested.permission)
            .collect::<Vec<_>>(),
        vec![PermissionId::DesktopInteract]
    );
    assert_eq!(app.default_risk, RiskLevel::High);
    assert!(matches!(
        app.resources.as_slice(),
        [ResourceDescriptor::Desktop { action, target: Some(target) }]
            if action == "open_application" && target == "QQ"
    ));
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
    assert_eq!(descriptor.default_risk, RiskLevel::High);
    assert!(matches!(
        descriptor.resources.as_slice(),
        [
            ResourceDescriptor::File { path: input },
            ResourceDescriptor::File { path: output },
            ResourceDescriptor::Network { url, method },
            ResourceDescriptor::NetworkFromResponse {
                source: status_source,
                target_template: status_template,
                method: status_method,
            },
            ResourceDescriptor::NetworkFromResponse {
                source: download_source,
                target_template: download_template,
                method: download_method,
            },
        ] if input == "image.png"
            && output == "image_upscaled.png"
            && url == "https://bigjpg.com/api/task/"
            && method == "POST"
            && status_source == "bigjpg.task_id"
            && status_template == "https://bigjpg.com/api/task/{value}"
            && status_method == "GET"
            && download_source == "bigjpg.task_result.url"
            && download_template == "{value}"
            && download_method == "GET"
    ));
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

#[test]
fn descriptor_identity_mismatch_fails_validation() {
    let descriptor = describe_builtin_tool(
        "write_file",
        &serde_json::json!({"path": "notes.txt", "content": "sample"}),
    )
    .unwrap();

    assert!(descriptor.validate_for_tool("read_file").is_err());
}

#[test]
fn descriptor_permissions_and_resources_must_match_the_tool_profile() {
    let descriptor =
        describe_builtin_tool("read_file", &serde_json::json!({"path": "README.md"})).unwrap();

    let mut wrong_permission = descriptor.clone();
    wrong_permission.requested_permissions =
        vec![PermissionId::ShellExecute.in_scope(crate::safety::ResourceScope::ShellCommand)];
    assert!(wrong_permission.validate_for_tool("read_file").is_err());

    let mut wrong_resource = descriptor;
    wrong_resource.resources = vec![ResourceDescriptor::Shell {
        command: "pwd".to_string(),
        working_directory: None,
    }];
    assert!(wrong_resource.validate_for_tool("read_file").is_err());
}

#[test]
fn descriptor_risk_cannot_be_lower_than_the_builtin_profile() {
    let mut descriptor = describe_builtin_tool(
        "mouse",
        &serde_json::json!({"action": "click", "x": 10, "y": 20}),
    )
    .unwrap();
    descriptor.default_risk = RiskLevel::Low;

    assert!(descriptor.validate_for_tool("mouse").is_err());
}
