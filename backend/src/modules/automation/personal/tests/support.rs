use super::*;

pub(super) struct Fixture {
    pub(super) server: Arc<AppServer>,
    pub(super) root: PathBuf,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yilian-personal-assistant-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("state.db"),
                root.to_str().unwrap(),
                ControlSession::new("personal-assistant-controlled-test-session-000000000000")
                    .unwrap(),
                Arc::new(InMemorySecretStore::new()),
            )
            .unwrap(),
        );
        Self { server, root }
    }
    pub(super) fn save(&self, writes: bool, runs: u32) -> Automation {
        let workspace = crate::workspace::Workspace::new(
            crate::workspace::WorkspaceId::generate(),
            "Personal fixture".into(),
            String::new(),
            Some(self.root.to_string_lossy().into_owned()),
            1,
        )
        .unwrap();
        self.server.db.create_workspace(&workspace).unwrap();
        super::super::super::save(
            &self.server,
            None,
            AutomationInput {
                expected_revision: None,
                name: "Local report".into(),
                workflow_graph_id: String::new(),
                target: Some(AutomationTarget::PersonalAssistant {
                    goal: "Read notes.txt, create report.md and verify it".into(),
                    conversation_id: "forged-conversation".into(),
                    root_path: "forged-root".into(),
                    allow_file_writes: writes,
                    allow_file_discovery: false,
                    max_runs: runs,
                }),
                workspace_id: workspace.id.to_string(),
                timezone: "UTC".into(),
                schedule: Schedule::Daily {
                    time: "09:00".into(),
                },
                enabled: true,
            },
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
pub(super) fn conversation(a: &Automation) -> &str {
    match a.target.as_ref().unwrap() {
        AutomationTarget::PersonalAssistant {
            conversation_id, ..
        } => conversation_id,
        _ => unreachable!(),
    }
}
pub(super) fn claim(f: &Fixture, a: &Automation) -> Occurrence {
    let mut item = occurrence(a, chrono::Utc::now().timestamp_millis(), true, false);
    assert!(f
        .server
        .db
        .claim_automation_occurrence(a, a.next_run_at, &mut item, true)
        .unwrap());
    item.conversation_id = Some(conversation(a).into());
    item.run_id = None;
    f.server.db.update_automation_occurrence(&item).unwrap();
    item
}

pub(super) fn discovery_delegation(f: &Fixture) -> Automation {
    let mut a = f.save(true, 5);
    if let Some(AutomationTarget::PersonalAssistant {
        allow_file_discovery,
        ..
    }) = &mut a.target
    {
        *allow_file_discovery = true;
    }
    a.revision += 1;
    f.server.db.save_automation(&a, false).unwrap();
    a
}

pub(super) fn read_grant(
    f: &Fixture,
    path: &std::path::Path,
    recursive: bool,
    effect: crate::safety::grant::GrantEffect,
) -> String {
    use crate::safety::{
        grant::{GrantResource, GrantSource, SecurityGrant},
        PermissionId,
    };
    let id = uuid::Uuid::new_v4().to_string();
    f.server
        .db
        .create_grant(&SecurityGrant {
            id: id.clone(),
            subject_id: "local-user".into(),
            effect,
            permission: PermissionId::FilesystemRead,
            resource: GrantResource::Filesystem {
                root: path.to_string_lossy().into_owned(),
                recursive,
            },
            source: GrantSource::User,
            created_at: 0,
            expires_at: None,
        })
        .unwrap();
    id
}

pub(super) fn management_request(a: &Automation) -> super::super::control::StartInput {
    super::super::control::StartInput {
        expected_revision: a.revision,
        request_id: uuid::Uuid::new_v4().to_string(),
        reviewed_occurrence_id: None,
        acknowledged: false,
    }
}

pub(super) async fn management_http(
    f: &Fixture,
    method: Method,
    path: String,
    body: Value,
) -> (axum::http::StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(
            crate::safety::CONTROL_SESSION_HEADER,
            "personal-assistant-controlled-test-session-000000000000",
        )
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = crate::api::build_router(f.server.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
    )
}

pub(super) fn file_delegation(f: &Fixture) -> Automation {
    std::fs::write(f.root.join("notes.txt"), "initial").unwrap();
    let a = f.save(false, 10);
    super::super::super::save(
        &f.server,
        Some(&a.id),
        AutomationInput {
            expected_revision: Some(a.revision),
            name: a.name,
            workflow_graph_id: a.workflow_graph_id,
            target: a.target,
            workspace_id: a.workspace_id,
            timezone: a.timezone,
            schedule: Schedule::FileChanged {
                path: f.root.join("notes.txt").to_string_lossy().into_owned(),
            },
            enabled: true,
        },
    )
    .unwrap()
}

pub(super) async fn model(
    f: &Fixture,
    mode: usize,
) -> (tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let root = f.root.clone();
    let router = Router::new().route("/chat/completions", post(move |Json(body): Json<Value>| {
        let index = counter.fetch_add(1, Ordering::SeqCst); let root = root.clone();
        async move {
            let names = body["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap()).collect::<Vec<_>>();
            assert!(names.contains(&"read_file")); assert!(!names.contains(&"bash")); assert!(!names.contains(&"qq_send_private"));
            if mode == 2 || (mode == 7 && index > 0) { tokio::time::sleep(Duration::from_secs(30)).await; }
            if mode == 3 { return (axum::http::StatusCode::BAD_REQUEST, [("content-type", "text/event-stream")], "controlled failure".into()); }
            let call = if mode == 7 && index == 0 { Some(("grep",json!({"pattern":"NO_MATCH_VALUE", "path":root.join("scan")}))) } else if mode == 6 {
                assert!(names.contains(&"glob")); assert!(names.contains(&"grep"));
                match index {
                    0 => Some(("glob", json!({"pattern":"source/**/*.md"}))),
                    1 => { assert!(body["messages"].to_string().contains("brief-alpha.md")); Some(("read_file", json!({"path":root.join("source/brief-alpha.md")}))) },
                    2 => Some(("read_file", json!({"path":root.join("source/nested/brief-beta.md")}))),
                    3 => Some(("write_file", json!({"path":root.join("discovery-report.md"), "content":"# Report\nAlpha milestone; Beta dependency.\n"}))),
                    4 => Some(("read_file", json!({"path":root.join("discovery-report.md")}))),
                    _ => None,
                }
            } else if mode == 4 && index == 0 { Some(("write_file", json!({"path":root.join("pending.md"),"content":"pending write"}))) }
                else if mode == 1 && index == 0 { Some(("ask_user", json!({"title":"What should the report include?","kind":"text","allow_text":true}))) }
                else if mode == 0 && index == 0 { Some(("read_file", json!({"path":root.join("notes.txt")}))) }
                else if mode == 0 && index == 1 { Some(("write_file", json!({"path":root.join("report.md"),"content":"# Report\nControlled notes summarized.\n"}))) }
                else if mode == 0 && index == 2 { Some(("read_file", json!({"path":root.join("report.md")}))) } else { None };
            let (delta, finish) = if let Some((name,args)) = call { (json!({"role":"assistant","tool_calls":[{"index":0,"id":format!("personal-call-{index}"),"type":"function","function":{"name":name,"arguments":args.to_string()}}]}), "tool_calls") } else { (json!({"content":"本轮已完成，报告已核验。"}), "stop") };
            let chunk = json!({"id":"personal-fixture","choices":[{"index":0,"delta":delta,"finish_reason":null}]}).to_string();
            let done = json!({"id":"personal-fixture","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":20,"completion_tokens":20,"total_tokens":40}}).to_string();
            (axum::http::StatusCode::OK, [("content-type", "text/event-stream")], format!("data: {chunk}\n\ndata: {done}\n\ndata: [DONE]\n\n"))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let reference = SecretRef::new("personal.controlled.fixture");
    f.server
        .secret_store
        .put(
            &reference,
            secrecy::SecretString::from("controlled-fixture".to_string()),
        )
        .await
        .unwrap();
    let mut config = f.server.config.write();
    config.model.base_url = origin;
    config.model.provider = "openai".into();
    config.model.name = "controlled".into();
    config.model.api_key_ref = Some(reference);
    config.model.api_key_env = String::new();
    (handle, calls)
}

pub(super) async fn wait_status(f: &Fixture, a: &Automation, status: &str) -> Occurrence {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let item = attempts(&f.server, &a.id)
                .unwrap()
                .into_iter()
                .find(|item| item.status != "skipped")
                .unwrap();
            if item.status == status {
                return item;
            }
            if status != "failed" && item.status == "failed" {
                panic!("unexpected failure: {:?}", item.error);
            }
            if item.status == "waiting_approval" && status != "waiting_approval" {
                let approval = f
                    .server
                    .approval_store
                    .pending_for(conversation(a))
                    .unwrap();
                let request = Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/approvals/{}/approve", approval.approval_id))
                    .header(
                        crate::safety::CONTROL_SESSION_HEADER,
                        "personal-assistant-controlled-test-session-000000000000",
                    )
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"conversation_id":conversation(a)}).to_string(),
                    ))
                    .unwrap();
                let response = crate::api::build_router(f.server.clone())
                    .oneshot(request)
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::OK);
                to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "timeout waiting {status}: {:?}",
            activity(&f.server, &a.id).unwrap()
        )
    })
}
