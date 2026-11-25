use super::*;

fn agent_owner() -> crate::execution::ExecutionOwner {
    crate::execution::ExecutionOwner::Conversation {
        conversation_id: "agent-conversation".into(),
    }
}
#[tokio::test]
async fn visible_runtime_revocation_rejects_queued_agent_before_effect_and_keeps_human_browsing() {
    let runtime = std::sync::Arc::new(BrowserRuntime::new());
    let host = host(true, false);
    runtime.install_host(host.clone()).unwrap();
    let mut humans = vec![];
    for index in 0..4 {
        let runtime = runtime.clone();
        humans.push(tokio::spawn(async move {
            runtime
                .execute(
                    // Independent reads occupy all slots; same-tab mutations
                    // now wait for the preceding mutation's settled receipt.
                    request(&format!("human-{index}"), BrowserAction::Observe),
                    CancellationToken::new(),
                )
                .await
        }));
    }
    while host.calls.load(Ordering::SeqCst) < 4 {
        tokio::task::yield_now().await;
    }
    let agent = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let mut request = request("agent-queued", BrowserAction::Fill);
            request.owner = Some(agent_owner());
            runtime.execute(request, CancellationToken::new()).await
        }
    });
    while !runtime.operations.lock().contains_key("agent-queued") {
        tokio::task::yield_now().await;
    }
    runtime.invalidate_authorizations();
    let result = tokio::time::timeout(std::time::Duration::from_millis(200), agent)
        .await
        .expect("Queued agent must wake on revocation")
        .unwrap();
    assert_eq!(
        result.unwrap_err().code,
        "permission_revoked_before_execution"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 4);
    assert!(
        humans.iter().all(|task| !task.is_finished()),
        "Human browsing must not be cancelled by agent permission edits"
    );
    for task in humans {
        task.abort();
        let _ = task.await;
    }
}
#[tokio::test]
async fn visible_runtime_revocation_during_mutation_preserves_unknown_and_never_replays() {
    let runtime = std::sync::Arc::new(BrowserRuntime::new());
    let host = host(true, false);
    runtime.install_host(host.clone()).unwrap();
    let mut pending = request("agent-inflight", BrowserAction::Click);
    pending.owner = Some(agent_owner());
    let original = pending.clone();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.execute(pending, CancellationToken::new()).await }
    });
    while host.calls.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    runtime.invalidate_authorizations();
    let result = tokio::time::timeout(std::time::Duration::from_millis(200), task)
        .await
        .expect("In-flight agent must cancel on revocation")
        .unwrap()
        .unwrap();
    assert_eq!(result.state, "result_unknown");
    assert_eq!(runtime.unknown_operations().len(), 1);
    assert_eq!(
        runtime
            .execute(original, CancellationToken::new())
            .await
            .unwrap()
            .state,
        "result_unknown"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn visible_browser_grant_api_success_invalidates_but_missing_delete_does_not() {
    let root = std::env::temp_dir().join(format!("browser-grant-epoch-{}", uuid::Uuid::new_v4()));
    let server = std::sync::Arc::new(
        crate::AppServer::new_with_control_session_store_and_browser(
            &root.join("state.db"),
            root.to_str().unwrap(),
            crate::safety::ControlSession::new("browser-grant-epoch-test-000000000000000000000")
                .unwrap(),
            std::sync::Arc::new(crate::integrations::secret::InMemorySecretStore::new()),
            std::sync::Arc::new(BrowserRuntime::for_local_fixture(root.join("bundle"))),
        )
        .unwrap(),
    );
    let epoch = server.browser_runtime.authorization_epoch();
    let created = crate::api::security_grants::create_grant(
        axum::extract::State(server.clone()),
        axum::Json(crate::api::security_grants::CreateGrantRequest {
            permission_id: "filesystem.read".into(),
            effect: "allow".into(),
            resource: crate::safety::grant::GrantResource::Filesystem {
                root: root.to_string_lossy().into_owned(),
                recursive: true,
            },
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(server.browser_runtime.authorization_epoch(), epoch + 1);
    let id = created["id"].as_str().unwrap().to_owned();
    let _ = crate::api::security_grants::delete_grant(
        axum::extract::State(server.clone()),
        axum::extract::Path(id.clone()),
    )
    .await
    .unwrap();
    assert_eq!(server.browser_runtime.authorization_epoch(), epoch + 2);
    assert!(crate::api::security_grants::delete_grant(
        axum::extract::State(server.clone()),
        axum::extract::Path(id)
    )
    .await
    .is_err());
    assert_eq!(server.browser_runtime.authorization_epoch(), epoch + 2);
}
#[tokio::test]
async fn visible_browser_settings_only_security_changes_invalidate() {
    let root =
        std::env::temp_dir().join(format!("browser-settings-epoch-{}", uuid::Uuid::new_v4()));
    let server = crate::AppServer::new_with_control_session_store_and_browser(
        &root.join("state.db"),
        root.to_str().unwrap(),
        crate::safety::ControlSession::new("browser-settings-epoch-test-000000000000000000")
            .unwrap(),
        std::sync::Arc::new(crate::integrations::secret::InMemorySecretStore::new()),
        std::sync::Arc::new(BrowserRuntime::for_local_fixture(root.join("bundle"))),
    )
    .unwrap();
    let epoch = server.browser_runtime.authorization_epoch();
    let mut config = server.config.read().clone();
    config.permissions.mode = crate::config::types::PermissionMode::FullApproval;
    let result = crate::modules::settings::application::update::apply(&server, config, None)
        .await
        .1
         .0;
    assert_eq!(result["status"], "saved");
    assert_eq!(server.browser_runtime.authorization_epoch(), epoch + 1);
    let mut unrelated = server.config.read().clone();
    unrelated.agent.name.push_str(" fixture");
    let result = crate::modules::settings::application::update::apply(&server, unrelated, None)
        .await
        .1
         .0;
    assert_eq!(result["status"], "saved");
    assert_eq!(server.browser_runtime.authorization_epoch(), epoch + 1);
}

const CONTROL_TOKEN: &str = "browser-policy-epoch-test-00000000000000000000000";

fn policy_server() -> std::sync::Arc<crate::AppServer> {
    let root = std::env::temp_dir().join(format!("browser-policy-{}", uuid::Uuid::new_v4()));
    std::sync::Arc::new(
        crate::AppServer::new_with_control_session_store_and_browser(
            std::path::Path::new(":memory:"),
            root.to_str().unwrap(),
            crate::safety::ControlSession::new(CONTROL_TOKEN).unwrap(),
            std::sync::Arc::new(crate::integrations::secret::InMemorySecretStore::new()),
            std::sync::Arc::new(BrowserRuntime::for_local_fixture(root.join("bundle"))),
        )
        .unwrap(),
    )
}

async fn policy_request(
    server: &std::sync::Arc<crate::AppServer>,
    method: axum::http::Method,
    path: &str,
    payload: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    let response = crate::api::build_router(server.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header(crate::safety::CONTROL_SESSION_HEADER, CONTROL_TOKEN)
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn visible_browser_access_rules_api_revokes_existing_authorization() {
    let server = policy_server();
    let host = host(false, false);
    server.browser_runtime.install_host(host.clone()).unwrap();
    let epoch = server.browser_runtime.authorization_epoch();
    let path = "/api/security/access-rules";
    let rules = json!({"rules":[{
        "id":"no-browser-writes", "scope":"default", "subject_id":null,
        "permission_id":"desktop.interact", "resource":null, "mode":"deny"
    }]});
    let (status, saved) = policy_request(&server, axum::http::Method::PUT, path, rules).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{saved}");
    assert_eq!(
        server
            .browser_runtime
            .execute_authorized(
                request("agent-before-rules", BrowserAction::Click),
                CancellationToken::new(),
                Some(epoch),
            )
            .await
            .unwrap_err()
            .code,
        "permission_revoked_before_execution"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
    let changed_epoch = server.browser_runtime.authorization_epoch();
    let (status, unchanged) = policy_request(
        &server,
        axum::http::Method::PUT,
        path,
        json!({"rules": saved["rules"]}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(unchanged, saved);
    assert_eq!(server.browser_runtime.authorization_epoch(), changed_epoch);

    server
        .db
        .conn()
        .execute_batch(
            "CREATE TRIGGER reject_rule_save BEFORE DELETE ON security_access_rule_scopes
         BEGIN SELECT RAISE(ABORT, 'fixture write failure'); END;",
        )
        .unwrap();
    let (status, _) =
        policy_request(&server, axum::http::Method::PUT, path, json!({"rules":[]})).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(server.browser_runtime.authorization_epoch(), changed_epoch);
    assert_eq!(
        policy_request(&server, axum::http::Method::GET, path, json!(null)).await,
        (axum::http::StatusCode::OK, saved)
    );
}

#[tokio::test]
async fn visible_browser_approval_mode_api_revokes_existing_authorization() {
    use crate::config::types::PermissionMode;

    let server = policy_server();
    let host = host(false, false);
    server.browser_runtime.install_host(host.clone()).unwrap();
    let epoch = server.browser_runtime.authorization_epoch();
    let path = "/api/security/approval-mode";
    let mode = if server.config.read().permissions.mode == PermissionMode::FullApproval {
        PermissionMode::PartialApproval
    } else {
        PermissionMode::FullApproval
    };
    let change = json!({"mode":mode});
    let (status, saved) =
        policy_request(&server, axum::http::Method::PUT, path, change.clone()).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{saved}");
    assert_eq!(saved, change);
    assert_eq!(
        server
            .browser_runtime
            .execute_authorized(
                request("agent-before-mode", BrowserAction::Click),
                CancellationToken::new(),
                Some(epoch),
            )
            .await
            .unwrap_err()
            .code,
        "permission_revoked_before_execution"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
    let changed_epoch = server.browser_runtime.authorization_epoch();
    assert_eq!(
        policy_request(&server, axum::http::Method::PUT, path, change.clone()).await,
        (axum::http::StatusCode::OK, change.clone())
    );
    assert_eq!(server.browser_runtime.authorization_epoch(), changed_epoch);

    server
        .db
        .conn()
        .execute_batch(
            "CREATE TRIGGER reject_mode_save BEFORE INSERT ON settings WHEN NEW.key='app_config'
         BEGIN SELECT RAISE(ABORT, 'fixture write failure'); END;",
        )
        .unwrap();
    let (status, _) = policy_request(
        &server,
        axum::http::Method::PUT,
        path,
        json!({"mode":"full_access"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(server.browser_runtime.authorization_epoch(), changed_epoch);
    assert_eq!(
        policy_request(&server, axum::http::Method::GET, path, json!(null)).await,
        (axum::http::StatusCode::OK, change)
    );
    assert_eq!(server.db.get_settings().unwrap().permissions.mode, mode);
}

#[tokio::test]
async fn visible_browser_security_subject_api_projects_the_active_role() {
    let server = policy_server();
    let path = "/api/security/subject";
    let epoch = server.browser_runtime.authorization_epoch();
    for (role, level, label) in [
        ("owner", "super", "超级级"),
        ("standard", "user", "用户级"),
        ("restricted", "yilian", "忆涟级"),
    ] {
        server
            .db
            .conn()
            .execute(
                "UPDATE security_role_bindings SET role_key=?1 WHERE subject_id='local-user'",
                [role],
            )
            .unwrap();
        let (status, body) =
            policy_request(&server, axum::http::Method::GET, path, json!(null)).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(
            body,
            json!({"subject_id":"local-user", "role":role, "level":level, "label":label})
        );
    }
    server
        .db
        .conn()
        .execute(
            "UPDATE security_role_bindings SET revoked_at=1 WHERE subject_id='local-user'",
            [],
        )
        .unwrap();
    let (status, body) = policy_request(&server, axum::http::Method::GET, path, json!(null)).await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "role_binding_unavailable");
    assert_eq!(server.browser_runtime.authorization_epoch(), epoch);
}
