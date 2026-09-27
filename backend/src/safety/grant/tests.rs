// ============================================================
// Grant model + evaluator tests.
// ============================================================

use std::path::PathBuf;
use std::sync::Arc;

use super::model::{
    parse_network_target, validate_grant, GrantEffect, GrantResource, NetworkZone,
    ProcessGrantScope, SecurityGrant,
};
use super::GrantEvaluator;
use crate::safety::{PermissionId, ResourceDescriptor};

#[test]
fn mcp_missing_grant_requires_approval() {
    assert!(matches!(
        GrantEvaluator::empty(".").evaluate(
            PermissionId::McpInvoke,
            &ResourceDescriptor::Mcp {
                server_id: "A".into(),
                tool_name: "publish".into()
            },
            0
        ),
        super::GrantDecision::RequireApproval { .. }
    ));
}

#[test]
fn mcp_json_roundtrip() {
    let value = serde_json::json!({"type":"mcp","server_id":"A","tool_name":"publish"});
    let resource: GrantResource =
        serde_json::from_value(value.clone()).expect("MCP grant must deserialize");
    assert_eq!(serde_json::to_value(resource).unwrap(), value);
}

fn mcp_resource(server: &str, tool: Option<&str>) -> GrantResource {
    serde_json::from_value(serde_json::json!({"type":"mcp","server_id":server,"tool_name":tool}))
        .unwrap()
}

#[test]
fn mcp_validation_and_scope() {
    for tool in [None, Some("publish")] {
        let resource = mcp_resource("A", tool);
        assert!(validate_grant(PermissionId::McpInvoke, &resource).is_ok());
        for permission in [
            PermissionId::FilesystemRead,
            PermissionId::NetworkRequest,
            PermissionId::ProcessControl,
            PermissionId::ShellExecute,
        ] {
            assert!(validate_grant(permission, &resource).is_err());
        }
    }
    for invalid in [
        "".to_string(),
        " ".into(),
        "*".into(),
        "A*".into(),
        "a\nb".into(),
        "\u{7f}".into(),
        "a".repeat(257),
        "界".repeat(86),
    ] {
        assert!(validate_grant(PermissionId::McpInvoke, &mcp_resource(&invalid, None)).is_err());
        assert!(
            validate_grant(PermissionId::McpInvoke, &mcp_resource("A", Some(&invalid))).is_err()
        );
    }
    assert!(validate_grant(
        PermissionId::McpInvoke,
        &mcp_resource(&"a".repeat(256), Some(&"b".repeat(256)))
    )
    .is_ok());
    assert!(validate_grant(
        PermissionId::McpInvoke,
        &GrantResource::Shell {
            host_escape_acknowledged: true
        }
    )
    .is_err());
    assert_eq!(
        super::model::resource_scope(PermissionId::McpInvoke),
        crate::safety::ResourceScope::McpServer
    );
}

#[test]
fn mcp_exact_matching_precedence_and_expiry() {
    use super::GrantDecision::*;
    let exact = grant(
        PermissionId::McpInvoke,
        GrantEffect::Allow,
        mcp_resource("A", Some("publish")),
    );
    let desc = |s: &str, t: &str| ResourceDescriptor::Mcp {
        server_id: s.into(),
        tool_name: t.into(),
    };
    let evaluator = GrantEvaluator::new(vec![exact.clone()], ".");
    assert_eq!(
        evaluator.evaluate(PermissionId::McpInvoke, &desc("A", "publish"), 1),
        Allow
    );
    for (s, t) in [
        ("B", "publish"),
        ("a", "publish"),
        ("AA", "publish"),
        ("A", "Publish"),
        ("A", "read"),
        ("A", "publish-more"),
    ] {
        assert!(matches!(
            evaluator.evaluate(PermissionId::McpInvoke, &desc(s, t), 1),
            RequireApproval { .. }
        ));
    }
    let broad = grant(
        PermissionId::McpInvoke,
        GrantEffect::Allow,
        mcp_resource("A", None),
    );
    let deny = grant(
        PermissionId::McpInvoke,
        GrantEffect::Deny,
        mcp_resource("A", Some("publish")),
    );
    for grants in [
        vec![broad.clone(), deny.clone()],
        vec![deny.clone(), broad.clone()],
    ] {
        let evaluator = GrantEvaluator::new(grants, ".");
        assert_eq!(
            evaluator.evaluate(PermissionId::McpInvoke, &desc("A", "read"), 1),
            Allow
        );
        let result =
            evaluator.evaluate_with_evidence(PermissionId::McpInvoke, &desc("A", "publish"), 1);
        assert!(matches!(result.decision, Deny { .. }));
        assert_eq!(result.matched_grant_id, Some(deny.id.clone()));
    }
    let mut expired = exact;
    expired.expires_at = Some(1);
    assert!(matches!(
        GrantEvaluator::new(vec![expired], ".").evaluate(
            PermissionId::McpInvoke,
            &desc("A", "publish"),
            1
        ),
        RequireApproval { .. }
    ));
}

#[test]
fn mcp_persistent_roundtrip_and_legacy_resources() {
    let root = tmp_root("mcp-roundtrip");
    let path = root.join("grants.db");
    let resources = vec![
        mcp_resource("A", Some("publish")),
        mcp_resource("A", None),
        GrantResource::Filesystem {
            root: ".".into(),
            recursive: true,
        },
        GrantResource::Network {
            scheme: None,
            host: "example.com".into(),
            port: None,
            methods: vec!["GET".into()],
            zone: NetworkZone::Public,
        },
        GrantResource::Process {
            scope: ProcessGrantScope::ManagedChildren,
        },
        GrantResource::Shell {
            host_escape_acknowledged: true,
        },
    ];
    let permissions = [
        PermissionId::McpInvoke,
        PermissionId::McpInvoke,
        PermissionId::FilesystemRead,
        PermissionId::NetworkRequest,
        PermissionId::ProcessControl,
        PermissionId::ShellExecute,
    ];
    let grants: Vec<_> = resources
        .into_iter()
        .zip(permissions)
        .map(|(r, p)| grant(p, GrantEffect::Allow, r))
        .collect();
    {
        let db = crate::db::Database::new(&path).unwrap();
        for g in &grants {
            db.create_grant(g).unwrap();
        }
    }
    {
        let db = crate::db::Database::new(&path).unwrap();
        let listed = db.list_grants("local-user").unwrap();
        for g in &grants {
            assert_eq!(db.get_grant(&g.id).unwrap().as_ref(), Some(g));
            assert!(listed.contains(g));
            db.delete_grant_for_subject(&g.id, "local-user").unwrap();
        }
        assert!(db.list_grants("local-user").unwrap().is_empty());
    }
    std::fs::remove_dir_all(root).unwrap();
}

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

#[test]
fn validate_grant_rejects_empty_filesystem_root() {
    assert!(validate_grant(
        PermissionId::FilesystemRead,
        &GrantResource::Filesystem {
            root: "  ".into(),
            recursive: true,
        }
    )
    .is_err());
}

#[test]
fn validate_grant_rejects_legacy_process_scopes_for_v08() {
    for scope in [
        ProcessGrantScope::ExplicitPid { pid: 1234 },
        ProcessGrantScope::AllHostProcesses,
    ] {
        assert!(validate_grant(
            PermissionId::ProcessControl,
            &GrantResource::Process { scope }
        )
        .is_err());
    }
    assert!(validate_grant(
        PermissionId::ProcessControl,
        &GrantResource::Process {
            scope: ProcessGrantScope::ManagedChildren,
        }
    )
    .is_ok());
}

#[test]
fn validate_grant_rejects_wildcard_all_invalid_scheme_and_method() {
    for resource in [
        GrantResource::Network {
            scheme: Some("ftp".into()),
            host: "example.com".into(),
            port: None,
            methods: vec!["GET".into()],
            zone: NetworkZone::Public,
        },
        GrantResource::Network {
            scheme: Some("https".into()),
            host: "*".into(),
            port: None,
            methods: vec!["GET".into()],
            zone: NetworkZone::Public,
        },
        GrantResource::Network {
            scheme: Some("https".into()),
            host: "example.com".into(),
            port: None,
            methods: vec!["TRACE".into()],
            zone: NetworkZone::Public,
        },
    ] {
        assert!(validate_grant(PermissionId::NetworkRequest, &resource).is_err());
    }
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
fn network_target_uses_url_parser_and_ignores_query_for_identity() {
    let target = parse_network_target("https://api.example.com:443/v1?token=secret").unwrap();
    assert_eq!(target.scheme, "https");
    assert_eq!(target.host, "api.example.com");
    assert_eq!(target.port, Some(443));
    assert!(parse_network_target("https://user:pass@example.com").is_err());
}

#[test]
fn network_target_rejects_invalid_scheme_and_wildcard_all() {
    assert!(parse_network_target("file:///etc/passwd").is_err());
    assert!(parse_network_target("https://*").is_err());
}

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
    let registry = Arc::new(crate::isolation::ManagedProcessRegistry::new());
    let pid = std::process::id();
    registry.record(pid, None, "test-child".to_string());
    let evaluator = GrantEvaluator::new(
        vec![grant(
            PermissionId::ProcessControl,
            GrantEffect::Allow,
            GrantResource::Process {
                scope: ProcessGrantScope::ManagedChildren,
            },
        )],
        PathBuf::from("."),
    )
    .with_registry(registry);
    let desc = ResourceDescriptor::Process {
        action: "kill".into(),
        pid: Some(pid),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::ProcessControl, &desc, 0),
        crate::safety::grant::GrantDecision::Allow
    ));
}

#[test]
fn process_kill_managed_children_rejects_unknown_pid() {
    let registry = Arc::new(crate::isolation::ManagedProcessRegistry::new());
    // pid 999 is NOT tracked → ManagedChildren must fail closed.
    let evaluator = GrantEvaluator::new(
        vec![grant(
            PermissionId::ProcessControl,
            GrantEffect::Allow,
            GrantResource::Process {
                scope: ProcessGrantScope::ManagedChildren,
            },
        )],
        PathBuf::from("."),
    )
    .with_registry(registry);
    let desc = ResourceDescriptor::Process {
        action: "kill".into(),
        pid: Some(999),
    };
    assert!(matches!(
        evaluator.evaluate(PermissionId::ProcessControl, &desc, 0),
        crate::safety::grant::GrantDecision::RequireApproval { .. }
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
