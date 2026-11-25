use super::*;
use crate::{
    modules::capability::import_store::Package,
    safety::{ControlSession, CONTROL_SESSION_HEADER},
};
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request},
};
use tower::ServiceExt;

async fn request(
    server: Arc<AppServer>,
    method: Method,
    path: &str,
    value: Value,
    authorized: bool,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if authorized {
        builder = builder.header(CONTROL_SESSION_HEADER, "a".repeat(64));
    }
    let response = crate::api::build_router(server)
        .oneshot(builder.body(Body::from(value.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
fn fixture(values: BTreeMap<String, Value>) -> (Arc<AppServer>, u64) {
    fixture_with_service(values, json!({}))
}

fn fixture_with_service(
    values: BTreeMap<String, Value>,
    additions: Value,
) -> (Arc<AppServer>, u64) {
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new("a".repeat(64)).unwrap(),
            Arc::new(crate::integrations::secret::InMemorySecretStore::new()),
        )
        .unwrap(),
    );
    let fields = values
        .iter()
        .map(|(key, value)| json!({"key":key,"label":key,"kind":"text","default":value}))
        .collect::<Vec<_>>();
    let manifest = json!({"schema_version":1,"id":"fixture.qq","name":"QQ fixture","version":"1.0.0","permissions":["process.manage"],"contributes":{
        "services":["services/fixture.json"],
        "settings":[{"id":"runtime","title":"Runtime","fields":fields}],
        "features":[{"id":"qq","title":"QQ 工作台","target":{"kind":"service","contribution":"services/fixture.json"}}]
    }});
    let mut descriptor = json!({"id":"napcat","title":"NapCat fixture","settings":"runtime","executable_key":"executable","working_directory_key":"cwd","argument_keys":["arg0","arg1","arg2","arg3"],"web_url_key":"web","channel_id_key":"channel","channel_provider":"onebot11"});
    for (name, value) in additions.as_object().unwrap() {
        descriptor[name] = value.clone();
    }
    let mut package = Package {
        id: "fixture.qq".into(),
        name: "QQ fixture".into(),
        version: "1.0.0".into(),
        kind: "plugin".into(),
        source: "fixture".into(),
        permissions: vec!["process.manage".into()],
        content_hash: String::new(),
        file_encodings: BTreeMap::new(),
        files: BTreeMap::from([
            ("plugin.json".into(), manifest.to_string()),
            ("services/fixture.json".into(), descriptor.to_string()),
        ]),
    };
    import_store::validate_package(&mut package).unwrap();
    let preview = import_store::inspect(&server.db, package, 1).unwrap();
    let installed = import_store::confirm(&server.db, &preview.preview_id, 2).unwrap();
    (server, installed.revision)
}
fn values() -> BTreeMap<String, Value> {
    [
        "executable",
        "cwd",
        "arg0",
        "arg1",
        "arg2",
        "arg3",
        "web",
        "channel",
    ]
    .into_iter()
    .map(|k| (k.into(), json!("")))
    .collect()
}
const PATH: &str = "/api/plugins/fixture.qq/services/napcat";

mod cloud_credentials;

#[cfg(windows)]
mod windows_fixture;

#[test]
fn uninstall_revokes_only_owned_mcp_permissions_and_keeps_saved_settings() {
    use crate::{
        db::access_rules::{AccessMode, AccessRule},
        safety::{
            grant::{GrantEffect, GrantResource, GrantSource, SecurityGrant},
            PermissionId,
        },
    };
    let (server, revision) =
        fixture_with_service(values(), json!({"health_path":"/health","mcp_path":"/mcp"}));
    let enabled =
        crate::api::capabilities::change_import_owned(&server, "fixture.qq", revision, "enable")
            .unwrap();
    let owned = managed_mcp_id("fixture.qq", "napcat");
    let mcp = |server_id| GrantResource::Mcp {
        server_id,
        tool_name: None,
    };
    let grants = [
        ("owned", mcp(owned.clone())),
        ("other", mcp("unrelated-server".into())),
        ("global", GrantResource::Any),
    ];
    for (id, resource) in grants {
        server
            .db
            .create_grant(&SecurityGrant {
                id: id.into(),
                subject_id: "local-user".into(),
                effect: GrantEffect::Allow,
                permission: PermissionId::McpInvoke,
                resource,
                source: GrantSource::User,
                created_at: 1,
                expires_at: None,
            })
            .unwrap();
    }
    let rules = [
        ("owned-rule", Some(mcp(owned.clone()))),
        ("other-rule", Some(mcp("unrelated-server".into()))),
        ("global-rule", None),
    ]
    .into_iter()
    .map(|(id, resource)| AccessRule {
        id: id.into(),
        scope: if resource.is_some() {
            "resource"
        } else {
            "default"
        }
        .into(),
        subject_id: None,
        permission_id: PermissionId::McpInvoke,
        resource,
        mode: AccessMode::Full,
    })
    .collect::<Vec<_>>();
    server.db.replace_access_rules(&rules).unwrap();
    let saved = serde_json::to_string(&values()).unwrap();
    server
        .db
        .conn()
        .execute(
            "INSERT INTO settings(key,value) VALUES('plugin_config:fixture.qq:runtime',?1)",
            [&saved],
        )
        .unwrap();

    server.db.conn().execute_batch("CREATE TRIGGER fail_plugin_uninstall BEFORE UPDATE ON settings WHEN new.key='v1.managed_imports.v1' BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    assert!(crate::api::capabilities::change_import_owned(
        &server,
        "fixture.qq",
        enabled.revision,
        "uninstall"
    )
    .is_err());
    assert!(server.db.get_grant("owned").unwrap().is_some());
    assert!(server.db.get_grant("owned-rule").unwrap().is_some());
    server
        .db
        .conn()
        .execute_batch("DROP TRIGGER fail_plugin_uninstall")
        .unwrap();

    let removed = crate::api::capabilities::change_import_owned(
        &server,
        "fixture.qq",
        enabled.revision,
        "uninstall",
    )
    .unwrap();
    assert!(!removed.installed);
    let preview = import_store::inspect(&server.db, enabled.package, 500).unwrap();
    let reinstalled = import_store::confirm(&server.db, &preview.preview_id, 501).unwrap();
    assert!(reinstalled.installed);
    assert_eq!(reinstalled.resources["services/fixture.json"], owned);
    assert!(server.db.get_grant("owned").unwrap().is_none());
    assert!(server.db.get_grant("owned-rule").unwrap().is_none());
    for id in ["global", "global-rule", "other", "other-rule"] {
        assert!(server.db.get_grant(id).unwrap().is_some(), "{id}");
    }
    let persisted: String = server
        .db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key='plugin_config:fixture.qq:runtime'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(persisted, saved);
}

#[cfg(windows)]
#[tokio::test]
async fn ensure_requests_the_exact_launch_before_creating_a_process() {
    let exe = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let cwd = std::env::temp_dir();
    let mut config = values();
    config.insert("executable".into(), json!(exe));
    config.insert("cwd".into(), json!(cwd));
    let (server, revision) = fixture(config);
    crate::api::capabilities::change_import_owned(&server, "fixture.qq", revision, "enable")
        .unwrap();
    let (code, result) = request(
        server.clone(),
        Method::POST,
        &format!("{PATH}/ensure"),
        json!({}),
        true,
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(result["code"], "launch_confirmation_required");
    assert_eq!(result["launch"]["executable"], json!(exe));
    assert_eq!(
        server
            .plugin_services
            .status("fixture.qq", "napcat")
            .unwrap()
            .state,
        "stopped"
    );
}

#[tokio::test]
async fn installed_and_enabled_service_never_starts_implicitly_and_requires_control_session() {
    let (server, revision) = fixture(values());
    assert_eq!(
        request(server.clone(), Method::GET, PATH, Value::Null, true)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    crate::api::capabilities::change_import_owned(&server, "fixture.qq", revision, "enable")
        .unwrap();
    assert_eq!(
        request(server.clone(), Method::GET, PATH, Value::Null, false)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (code, status) = request(server.clone(), Method::GET, PATH, Value::Null, true).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(status["state"], "stopped");
    assert!(status["launch"].is_null());
    assert!(status["configuration_error"].is_string());
    let (_, features) = request(
        server,
        Method::GET,
        "/api/plugins/features",
        Value::Null,
        true,
    )
    .await;
    assert_eq!(features["features"][0]["target"]["kind"], "service");
}

#[tokio::test]
async fn service_package_is_reported_as_managed_after_enable() {
    let (server, revision) = fixture(values());
    crate::api::capabilities::change_import_owned(&server, "fixture.qq", revision, "enable")
        .unwrap();
    let (_, result) = request(
        server,
        Method::GET,
        "/api/capabilities/imports",
        Value::Null,
        true,
    )
    .await;
    assert_eq!(result["imports"][0]["runtime_status"], "service_managed");
}

#[test]
fn service_descriptor_rejects_missing_text_setting() {
    let mut manifest = serde_json::json!({"schema_version":1,"id":"fixture","name":"x","version":"1","permissions":["process.manage"],"contributes":{"services":["service.json"],"settings":[{"id":"x","title":"x","fields":[]}]}});
    let service = json!({"id":"napcat","title":"x","settings":"x","executable_key":"exe","working_directory_key":"cwd","web_url_key":"web"});
    let manifest = crate::plugin::parse_manifest(&manifest.take().to_string()).unwrap();
    assert!(ServiceDefinition::parse(&service.to_string(), &manifest).is_err());
}

#[cfg(windows)]
const TREE_WORKER: &str = "api::plugin_services::tests::managed_tree_worker";

#[cfg(windows)]
#[test]
#[ignore = "started by the managed process-tree lifecycle test"]
fn managed_tree_worker() {
    // Run only in the per-test directory created by the lifecycle test.
    assert!(std::path::Path::new("tree-fixture").is_file());
    if std::env::var_os("YILIAN_TREE_FIXTURE_LEAF").is_some() {
        std::fs::write("child.pending", std::process::id().to_string()).unwrap();
        std::fs::rename("child.pending", "child.txt").unwrap();
        std::thread::sleep(Duration::from_secs(120));
        return;
    }
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    crate::utils::process::hide_std_command_window(&mut command);
    let status = command
        .args(["--ignored", "--exact", TREE_WORKER])
        .env("YILIAN_TREE_FIXTURE_LEAF", "1")
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(windows)]
#[tokio::test]
async fn service_start_confirmation_stop_disable_and_shutdown_own_the_process_tree() {
    let temp = std::env::temp_dir().join(format!("yilian-plugin-service-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp).unwrap();
    let exe = std::env::current_exe().unwrap();
    let child_file = temp.join("child.txt");
    std::fs::write(temp.join("tree-fixture"), "").unwrap();
    let mut config = values();
    for (key, value) in [
        ("executable", exe.to_string_lossy().into_owned()),
        ("cwd", temp.to_string_lossy().into_owned()),
        ("arg0", "--ignored".into()),
        ("arg1", "--exact".into()),
        ("arg2", TREE_WORKER.into()),
        ("arg3", "--nocapture".into()),
    ] {
        config.insert(key.into(), json!(value));
    }
    let (server, revision) = fixture(config);
    let enabled =
        crate::api::capabilities::change_import_owned(&server, "fixture.qq", revision, "enable")
            .unwrap();
    let (_, status) = request(server.clone(), Method::GET, PATH, Value::Null, true).await;
    let launch = status["launch"].clone();
    let mut changed = launch.clone();
    changed["args"][0] = json!("-Changed");
    assert_eq!(
        request(
            server.clone(),
            Method::POST,
            &format!("{PATH}/start"),
            json!({"launch":changed}),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    for action in ["stop", "disable", "shutdown"] {
        if child_file.exists() {
            std::fs::remove_file(&child_file).unwrap();
        }
        let (code, started) = request(
            server.clone(),
            Method::POST,
            &format!("{PATH}/start"),
            json!({"launch":launch}),
            true,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{started}");
        assert_eq!(started["state"], "running");
        assert_eq!(
            request(
                server.clone(),
                Method::POST,
                &format!("{PATH}/start"),
                json!({"launch":launch}),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let child_pid: u32 = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                match std::fs::read_to_string(&child_file) {
                    Ok(pid) => break pid.trim().parse().expect("fixture published a valid PID"),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        let state = server
                            .plugin_services
                            .status("fixture.qq", "napcat")
                            .unwrap();
                        assert_eq!(
                            state.state, "running",
                            "{action}: fixture exited before readiness, exit code {:?}",
                            state.exit_code
                        );
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    Err(error) => panic!("{action}: could not read fixture readiness: {error}"),
                }
            }
        })
        .await
        .expect("managed descendant did not publish its PID");
        assert_ne!(started["pid"].as_u64().unwrap(), u64::from(child_pid));
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        let child = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, child_pid) };
        assert!(!child.is_null());
        match action {
            "stop" => {
                assert_eq!(
                    request(
                        server.clone(),
                        Method::POST,
                        &format!("{PATH}/stop"),
                        Value::Null,
                        true
                    )
                    .await
                    .0,
                    StatusCode::OK
                );
            }
            "disable" => {
                let disabled = crate::api::capabilities::change_import_owned(
                    &server,
                    "fixture.qq",
                    enabled.revision,
                    "disable",
                )
                .unwrap();
                crate::api::capabilities::change_import_owned(
                    &server,
                    "fixture.qq",
                    disabled.revision,
                    "enable",
                )
                .unwrap();
            }
            _ => server.plugin_services.shutdown().unwrap(),
        }
        assert_eq!(
            unsafe { WaitForSingleObject(child, 3000) },
            WAIT_OBJECT_0,
            "{action} must stop the descendant"
        );
        unsafe {
            CloseHandle(child);
        }
        assert_ne!(
            server
                .plugin_services
                .status("fixture.qq", "napcat")
                .unwrap()
                .state,
            "running"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}
