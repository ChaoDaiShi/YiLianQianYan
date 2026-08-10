use crate::safety::{BuiltInRole, GrantMode, PermissionId, RolePolicy};

#[test]
fn role_matrix_matches_the_approved_specification() {
    use BuiltInRole::{Owner, Restricted, Standard};
    use GrantMode::{Allow, Deny, RequireApproval};
    use PermissionId::*;

    let expected = [
        (FilesystemRead, [Allow, Allow, Allow]),
        (FilesystemWrite, [Allow, Allow, Deny]),
        (ShellExecute, [RequireApproval, RequireApproval, Deny]),
        (ProcessInspect, [Allow, Allow, Allow]),
        (ProcessControl, [RequireApproval, RequireApproval, Deny]),
        (NetworkRequest, [Allow, Allow, Deny]),
        (DesktopObserve, [Allow, Allow, RequireApproval]),
        (DesktopInteract, [Allow, RequireApproval, Deny]),
        (SkillLoad, [Allow, Allow, Allow]),
        (AgentPlan, [Allow, Allow, Allow]),
    ];

    for (permission, grants) in expected {
        assert_eq!(RolePolicy::grant(Owner, permission), grants[0]);
        assert_eq!(RolePolicy::grant(Standard, permission), grants[1]);
        assert_eq!(RolePolicy::grant(Restricted, permission), grants[2]);
    }

    assert_eq!(PermissionId::ALL.len(), expected.len());
}
