// ============================================================
// Grant model + evaluator tests.
// ============================================================

use std::path::PathBuf;

use super::model::{
    validate_grant, GrantEffect, GrantResource, NetworkZone, ProcessGrantScope, SecurityGrant,
};
use super::GrantEvaluator;
use crate::safety::{PermissionId, ResourceDescriptor};

fn grant(permission: PermissionId, effect: GrantEffect, resource: GrantResource) -> SecurityGrant {
    SecurityGrant {
        id: uuid::Uuid::new_v4().to_string(),
        subject_id: "local-user".to_string(),
        effect,
        permission,
        resource,
        source: super::model::GrantSource::User,
        created_at: 0,
        expires_at: None,
    }
}

fn fs_grant(permission: PermissionId, root: &str, effect: GrantEffect) -> SecurityGrant {
    grant(
        permission,
        effect,
        GrantResource::Filesystem {
            root: root.to_string(),
            recursive: true,
        },
    )
}

fn net_grant(
    host: &str,
    methods: &[&str],
    zone: NetworkZone,
    effect: GrantEffect,
) -> SecurityGrant {
    grant(
        PermissionId::NetworkRequest,
        effect,
        GrantResource::Network {
            scheme: None,
            host: host.to_string(),
            port: None,
            methods: methods.iter().map(|s| s.to_string()).collect(),
            zone,
        },
    )
}

fn tmp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("yilian-grant-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

// ── Model validation ──

#[test]
fn validate_grant_rejects_bad_combos() {
    assert!(validate_grant(
        PermissionId::FilesystemRead,
        &GrantResource::Filesystem {
            root: ".".into(),
            recursive: true
        }
    )
    .is_ok());
    assert!(validate_grant(
        PermissionId::NetworkRequest,
        &GrantResource::Filesystem {
            root: ".".into(),
            recursive: true
        }
    )
    .is_err());
    assert!(validate_grant(
        PermissionId::NetworkRequest,
        &GrantResource::Network {
            scheme: None,
            host: "x".into(),
            port: None,
            methods: vec![],
            zone: NetworkZone::Public
        }
    )
    .is_err()); // empty methods rejected
}

// ── Filesystem ──

#[test]
fn filesystem_read_within_grant_is_allowed() {
    let root = tmp_root("fs-allow");
    let evaluator = GrantEvaluator::new(
        vec![fs_grant(
            PermissionId::FilesystemRead,
            root.to_str().unwrap(),
            GrantEffect::Allow,
        )],
        &root,
    );
    let desc = ResourceDescriptor::File {
        path: root.join("a.txt").to_string_lossy().to_string(),
    };
    // target doesn't exist, but parent (root) resolves; resolve_write_target handles it.
    assert!(matches!(
        evaluator.evaluate(PermissionId::FilesystemRead, &desc, 0),
        crate::safety::grant::GrantDecision::Allow
    ));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn filesystem_read_outside_grant_is_approval() {
    let root = tmp_root("fs-outside");
    let outside = tmp_root("fs-outside-target");
    let evaluator = GrantEvaluator::new(
        vec![fs_grant(
            PermissionId::FilesystemRead,
            root.to_str().unwrap(),
            GrantEffect::Allow,
        )],
        &root,
    );
    let desc = ResourceDescriptor::File {
        path: outside.join("a.txt").to_string_lossy().to_string(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::FilesystemRead, &desc, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn explicit_deny_wins_over_allow() {
    let root = tmp_root("fs-deny");
    let evaluator = GrantEvaluator::new(
        vec![
            fs_grant(
                PermissionId::FilesystemWrite,
                root.to_str().unwrap(),
                GrantEffect::Allow,
            ),
            fs_grant(
                PermissionId::FilesystemWrite,
                root.to_str().unwrap(),
                GrantEffect::Deny,
            ),
        ],
        &root,
    );
    let desc = ResourceDescriptor::File {
        path: root.join("a.txt").to_string_lossy().to_string(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::FilesystemWrite, &desc, 0),
        crate::safety::grant::GrantDecision::Deny { .. }
    ));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn expired_grant_is_ignored() {
    let root = tmp_root("fs-expired");
    let mut g = fs_grant(
        PermissionId::FilesystemRead,
        root.to_str().unwrap(),
        GrantEffect::Allow,
    );
    g.expires_at = Some(100);
    let evaluator = GrantEvaluator::new(vec![g], &root);
    let desc = ResourceDescriptor::File {
        path: root.join("a.txt").to_string_lossy().to_string(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::FilesystemRead, &desc, 200),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
    std::fs::remove_dir_all(&root).ok();
}

// ── Network ──

#[test]
fn network_exact_host_match() {
    let evaluator = GrantEvaluator::new(
        vec![net_grant(
            "api.example.com",
            &["GET"],
            NetworkZone::Public,
            GrantEffect::Allow,
        )],
        PathBuf::from("."),
    );
    let ok = ResourceDescriptor::Network {
        url: "https://api.example.com/v1".into(),
        method: "GET".into(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::NetworkRequest, &ok, 0),
        crate::safety::grant::GrantDecision::Allow
    ));
    let wrong = ResourceDescriptor::Network {
        url: "https://other.example.com/v1".into(),
        method: "GET".into(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::NetworkRequest, &wrong, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
}

#[test]
fn network_wildcard_label_boundary() {
    let evaluator = GrantEvaluator::new(
        vec![net_grant(
            "*.example.com",
            &["GET"],
            NetworkZone::Public,
            GrantEffect::Allow,
        )],
        PathBuf::from("."),
    );
    let ok = ResourceDescriptor::Network {
        url: "https://a.example.com".into(),
        method: "GET".into(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::NetworkRequest, &ok, 0),
        crate::safety::grant::GrantDecision::Allow
    ));
    let evil = ResourceDescriptor::Network {
        url: "https://evil-example.com".into(),
        method: "GET".into(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::NetworkRequest, &evil, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
}

#[test]
fn network_method_mismatch() {
    let evaluator = GrantEvaluator::new(
        vec![net_grant(
            "api.example.com",
            &["GET"],
            NetworkZone::Public,
            GrantEffect::Allow,
        )],
        PathBuf::from("."),
    );
    let post = ResourceDescriptor::Network {
        url: "https://api.example.com".into(),
        method: "POST".into(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::NetworkRequest, &post, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
}

#[test]
fn public_zone_blocks_loopback_and_private() {
    let evaluator = GrantEvaluator::new(
        vec![net_grant(
            "127.0.0.1",
            &["GET"],
            NetworkZone::Public,
            GrantEffect::Allow,
        )],
        PathBuf::from("."),
    );
    let loopback = ResourceDescriptor::Network {
        url: "http://127.0.0.1:8080".into(),
        method: "GET".into(),
    };
    // Even an explicit "allow" grant with Public zone must not match loopback.
    assert!(matches!(
        evaluator.evaluate(PermissionId::NetworkRequest, &loopback, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
}

#[test]
fn loopback_zone_allows_loopback() {
    let evaluator = GrantEvaluator::new(
        vec![net_grant(
            "127.0.0.1",
            &["GET"],
            NetworkZone::Loopback,
            GrantEffect::Allow,
        )],
        PathBuf::from("."),
    );
    let loopback = ResourceDescriptor::Network {
        url: "http://127.0.0.1:8080".into(),
        method: "GET".into(),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::NetworkRequest, &loopback, 0),
        crate::safety::grant::GrantDecision::Allow
    ));
}

// ── Process ──

#[test]
fn process_kill_managed_children_allow() {
    let evaluator = GrantEvaluator::new(
        vec![grant(
            PermissionId::ProcessControl,
            GrantEffect::Allow,
            GrantResource::Process {
                scope: ProcessGrantScope::ManagedChildren,
            },
        )],
        PathBuf::from("."),
    );
    let desc = ResourceDescriptor::Process {
        action: "kill".into(),
        pid: Some(123),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::ProcessControl, &desc, 0),
        crate::safety::grant::GrantDecision::Allow
    ));
}

#[test]
fn process_kill_unknown_pid_requires_grant() {
    let evaluator = GrantEvaluator::empty(PathBuf::from("."));
    let desc = ResourceDescriptor::Process {
        action: "kill".into(),
        pid: Some(999),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::ProcessControl, &desc, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
}

// ── Shell ──

#[test]
fn shell_requires_explicit_acknowledgement() {
    let evaluator = GrantEvaluator::new(
        vec![grant(
            PermissionId::ShellExecute,
            GrantEffect::Allow,
            GrantResource::Shell {
                host_escape_acknowledged: false,
            },
        )],
        PathBuf::from("."),
    );
    let desc = ResourceDescriptor::Shell {
        command: "echo hi".into(),
        working_directory: None,
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::ShellExecute, &desc, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
    ));
}

// ── Non-enforced permission ──

#[test]
fn process_inspect_is_not_grant_enforced() {
    let evaluator = GrantEvaluator::empty(PathBuf::from("."));
    let desc = ResourceDescriptor::Process {
        action: "list".into(),
        pid: None,
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::ProcessInspect, &desc, 0),
        crate::safety::grant::GrantDecision::Allow
    ));
}
