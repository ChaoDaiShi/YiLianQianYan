//! MCP grant validation, exact matching and persistence contracts.
use super::*;

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
        crate::safety::grant::GrantDecision::RequireApproval { .. }
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
        crate::safety::grant::model::resource_scope(PermissionId::McpInvoke),
        crate::safety::ResourceScope::McpServer
    );
}

#[test]
fn mcp_exact_matching_precedence_and_expiry() {
    use crate::safety::grant::GrantDecision::*;
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
