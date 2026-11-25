use super::*;

#[test]
fn personal_assistant_discovery_defaults_off_and_needs_recursive_grants() {
    use crate::safety::{grant::GrantEffect, PermissionId};
    let old: AutomationTarget = serde_json::from_value(
        json!({"kind":"personal_assistant", "goal":"local goal", "max_runs":5}),
    )
    .unwrap();
    assert!(matches!(
        old,
        AutomationTarget::PersonalAssistant {
            allow_file_discovery: false,
            ..
        }
    ));
    let f = Fixture::new();
    let off = f.save(false, 5);
    let item = claim(&f, &off);
    let root = std::fs::canonicalize(&f.root).unwrap();
    let registry =
        scoped_tools::registry(&f.server, &off, &item.id, root.to_str().unwrap(), false).unwrap();
    assert!(registry.get("glob").is_none());
    assert!(registry.get("grep").is_none());
    // Another isolated fixture opts in, independently of file write permission.
    let f = Fixture::new();
    let a = discovery_delegation(&f);
    let item = claim(&f, &a);
    for grant in f.server.db.list_grants("local-user").unwrap() {
        if grant.permission == PermissionId::FilesystemRead {
            f.server.db.delete_grant(&grant.id).unwrap();
        }
    }
    let root = std::fs::canonicalize(&f.root).unwrap();
    let registry =
        scoped_tools::registry(&f.server, &a, &item.id, root.to_str().unwrap(), false).unwrap();
    assert!(registry.get("write_file").is_none());
    let tool = registry.get("glob").unwrap();
    assert!(tool
        .security_descriptor(&json!({"pattern":"**/*.md"}))
        .is_err());
    let grant = read_grant(&f, &root, false, GrantEffect::Allow);
    assert!(tool
        .security_descriptor(&json!({"pattern":"**/*.md"}))
        .is_err());
    f.server.db.delete_grant(&grant).unwrap();
    read_grant(&f, &root, true, GrantEffect::Allow);
    let descriptor = tool
        .security_descriptor(&json!({"pattern":"**/*.md"}))
        .unwrap();
    assert_eq!(
        descriptor.resources,
        vec![crate::safety::ResourceDescriptor::File {
            path: root.to_string_lossy().into_owned()
        }]
    );
}

#[tokio::test]
async fn personal_assistant_discovery_honors_descendant_deny_approve_and_stopped_snapshots() {
    use crate::safety::grant::GrantEffect;
    let f = Fixture::new();
    let a = discovery_delegation(&f);
    let item = claim(&f, &a);
    std::fs::create_dir_all(f.root.join("safe")).unwrap();
    std::fs::write(f.root.join("secret.txt"), "restricted private text").unwrap();
    let root = std::fs::canonicalize(&f.root).unwrap();
    read_grant(&f, &root, true, GrantEffect::Allow);
    let registry =
        scoped_tools::registry(&f.server, &a, &item.id, root.to_str().unwrap(), true).unwrap();
    let tool = registry.get("grep").unwrap();
    for effect in [GrantEffect::Deny, GrantEffect::Approve] {
        let id = read_grant(&f, &f.root.join("secret.txt"), false, effect);
        assert!(tool
            .security_descriptor(&json!({"pattern":"private"}))
            .is_err());
        assert!(tool
            .security_descriptor(&json!({"pattern":"private", "path":f.root.join("safe")}))
            .is_ok());
        f.server.db.delete_grant(&id).unwrap();
    }
    assert!(tool
        .security_descriptor(&json!({"pattern":"private", "path":f.root.join("..")}))
        .is_err());
    stop(&f.server, &a.id).await.unwrap();
    assert!(tool
        .security_descriptor(&json!({"pattern":"private"}))
        .is_err());
}

#[tokio::test]
async fn personal_assistant_discovery_uses_gateway_and_rechecks_revoked_permission() {
    use crate::safety::grant::GrantEffect;
    let f = Fixture::new();
    let mut a = discovery_delegation(&f);
    let item = claim(&f, &a);
    std::fs::create_dir_all(f.root.join("input")).unwrap();
    std::fs::write(f.root.join("input/alpha.md"), "Alpha milestone").unwrap();
    let root = std::fs::canonicalize(&f.root).unwrap();
    read_grant(&f, &root, true, GrantEffect::Allow);
    let registry = Arc::new(
        scoped_tools::registry(&f.server, &a, &item.id, root.to_str().unwrap(), true).unwrap(),
    );
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        f.server.config.read().sandbox.clone(),
        root.to_str().unwrap(),
        registry.clone(),
    )
    .with_db(Arc::new(f.server.db.clone_connection()));
    let request = SecurityExecutionRequest {
        conversation_id: conversation(&a).into(),
        tool_call_id: "discovery-glob".into(),
        tool_name: "glob".into(),
        arguments: json!({"pattern":"input/*.md"}),
        subject: SecuritySubject::local_user(),
    };
    let outcome = gateway
        .execute_approved(&request, RiskLevel::Low)
        .await
        .unwrap();
    assert!(
        matches!(outcome, crate::safety::execution_gateway::SecurityExecutionOutcome::Executed { tool_result, .. } if tool_result.ok && tool_result.content.contains("alpha.md"))
    );
    // Simulate authority revocation beneath a stale runtime snapshot.
    if let Some(AutomationTarget::PersonalAssistant {
        allow_file_discovery,
        ..
    }) = &mut a.target
    {
        *allow_file_discovery = false;
    }
    a.revision += 1;
    f.server.db.save_automation(&a, false).unwrap();
    assert!(registry
        .get("glob")
        .unwrap()
        .security_descriptor(&json!({"pattern":"**/*"}))
        .is_err());
}

#[test]
fn personal_assistant_discovery_resolves_relative_grants_as_the_gateway_does() {
    use crate::safety::grant::GrantEffect;
    let f = Fixture::new();
    let selected = f.root.join("delegated/safe");
    std::fs::create_dir_all(&selected).unwrap();
    std::fs::write(selected.join("restricted.txt"), "private").unwrap();
    read_grant(&f, &f.root.join("delegated"), true, GrantEffect::Allow);
    let selected = std::fs::canonicalize(selected).unwrap();
    assert!(super::super::search_authority::check(&f.server, &selected).is_ok());
    read_grant(
        &f,
        std::path::Path::new("delegated/safe/restricted.txt"),
        false,
        GrantEffect::Deny,
    );
    assert!(super::super::search_authority::check(&f.server, &selected).is_err());
}

#[tokio::test]
async fn personal_assistant_discovers_two_inputs_creates_verifies_report_with_existing_budget() {
    use crate::safety::grant::GrantEffect;
    let f = Fixture::new();
    let a = discovery_delegation(&f);
    std::fs::create_dir_all(f.root.join("source/nested")).unwrap();
    std::fs::write(f.root.join("source/brief-alpha.md"), "Alpha milestone").unwrap();
    std::fs::write(
        f.root.join("source/nested/brief-beta.md"),
        "Beta dependency",
    )
    .unwrap();
    read_grant(&f, &f.root, true, GrantEffect::Allow);
    let (mock, calls) = model(&f, 6).await;
    let item = super::super::super::run_now(&f.server, &a.id)
        .await
        .unwrap();
    assert_eq!(item.conversation_id.as_deref(), Some(conversation(&a)));
    let result = wait_status(&f, &a, "completed").await;
    assert!(result.report.is_some());
    assert_eq!(
        std::fs::read_to_string(f.root.join("discovery-report.md")).unwrap(),
        "# Report\nAlpha milestone; Beta dependency.\n"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    let stored = f.server.db.get_conversation(conversation(&a)).unwrap();
    assert!(stored
        .execution_history
        .iter()
        .any(|entry| entry.name == "glob"));
    mock.abort();
}
