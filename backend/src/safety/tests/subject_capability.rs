use crate::safety::{
    Action, BuiltInRole, Capability, PermissionId, ResourceScope, SecuritySubject, SubjectType,
};

#[test]
fn local_user_subject_has_stable_identity() {
    let subject = SecuritySubject::local_user();

    assert_eq!(subject.subject_id, "local-user");
    assert_eq!(subject.subject_type, SubjectType::LocalUser);
    assert_eq!(subject.provider, "local");
    assert_eq!(subject.external_ref, None);
}

#[test]
fn built_in_roles_use_stable_wire_names() {
    let encoded = serde_json::to_value([
        BuiltInRole::Owner,
        BuiltInRole::Standard,
        BuiltInRole::Restricted,
    ])
    .unwrap();

    assert_eq!(
        encoded,
        serde_json::json!(["owner", "standard", "restricted"])
    );
    assert_eq!(BuiltInRole::ALL.len(), 3);
}

#[test]
fn permission_ids_expose_canonical_capability_and_action() {
    assert_eq!(PermissionId::FilesystemRead.as_str(), "filesystem.read");
    assert_eq!(
        PermissionId::FilesystemRead.capability(),
        Capability::Filesystem
    );
    assert_eq!(PermissionId::FilesystemRead.action(), Action::Read);

    assert_eq!(PermissionId::DesktopInteract.as_str(), "desktop.interact");
    assert_eq!(
        PermissionId::DesktopInteract.capability(),
        Capability::Desktop
    );
    assert_eq!(PermissionId::DesktopInteract.action(), Action::Interact);
}

#[test]
fn requested_permission_keeps_resource_scope() {
    let requested = PermissionId::FilesystemWrite.in_scope(ResourceScope::Workspace);

    assert_eq!(requested.permission, PermissionId::FilesystemWrite);
    assert_eq!(requested.scope, ResourceScope::Workspace);
}
