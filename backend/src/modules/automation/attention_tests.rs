use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

mod observations;

use crate::{
    app::state::AppServer,
    integrations::secret::{InMemorySecretStore, SecretRef},
    safety::{
        grant::{GrantEffect, GrantResource, GrantSource, NetworkZone, SecurityGrant},
        ControlSession, PermissionId,
    },
    workspace::{Workspace, WorkspaceId},
};

const TOKEN: &str = "attention-watch-local-fixture-control-session";

struct Fixture {
    server: Arc<AppServer>,
    router: Router,
    workspace: Workspace,
    root: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("attention-{}", uuid::Uuid::new_v4()));
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("test.db"),
                root.to_str().unwrap(),
                ControlSession::new(TOKEN).unwrap(),
                Arc::new(InMemorySecretStore::new()),
            )
            .unwrap(),
        );
        let workspace = Workspace::new(
            WorkspaceId::generate(),
            "Watch fixture".into(),
            String::new(),
            Some(root.to_string_lossy().into_owned()),
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        let router = crate::app::router::build_router(server.clone());
        Self {
            server,
            router,
            workspace,
            root,
        }
    }

    async fn use_loopback_model(&self, address: std::net::SocketAddr) {
        let reference = SecretRef::new("attention.deadline.fixture");
        self.server
            .secret_store
            .put(&reference, secrecy::SecretString::from("local-model-only"))
            .await
            .unwrap();
        {
            let mut config = self.server.config.write();
            config.model.provider = "openai".into();
            config.model.name = "deadline-fixture".into();
            config.model.base_url = format!("http://{address}");
            config.model.api_key_ref = Some(reference);
            config.model.api_key_env.clear();
            config.sandbox.profile = crate::config::types::SandboxProfile::Open;
            config.permissions.mode = crate::config::types::PermissionMode::FullAccess;
        }
        self.server
            .db
            .create_grant(&SecurityGrant {
                id: "clock-http-grant".into(),
                subject_id: "local-user".into(),
                effect: GrantEffect::Allow,
                permission: PermissionId::NetworkRequest,
                resource: GrantResource::Network {
                    scheme: Some("http".into()),
                    host: "127.0.0.1".into(),
                    port: Some(address.port()),
                    methods: vec!["GET".into()],
                    zone: NetworkZone::Loopback,
                },
                source: GrantSource::User,
                created_at: 1,
                expires_at: None,
            })
            .unwrap();
    }

    fn input(&self, source: Value) -> Value {
        json!({
            "name":"关注状态",
            "workspace_id":self.workspace.id,
            "workflow_graph_id":"",
            "timezone":"UTC",
            "target":{
                "kind":"personal_assistant",
                "goal":"核对已保存目标的进展",
                "max_runs":2
            },
            "schedule":{
                "kind":"watch","source":source,
                "min_interval_secs":1,
                "expires_at":chrono::Utc::now().timestamp_millis()+180_000,
                "max_checks":8,"max_retries":2
            },
            "enabled":true
        })
    }

    async fn request(&self, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header(crate::safety::CONTROL_SESSION_HEADER, TOKEN)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let result = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)));
        (status, result)
    }

    async fn create(&self, source: Value) -> super::model::Automation {
        let (status, result) = self
            .request("POST", "/api/automations", self.input(source))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{result}");
        serde_json::from_value(result["automation"].clone()).unwrap()
    }

    fn get(&self, id: &str) -> super::model::Automation {
        self.server.db.get_automation(id).unwrap().unwrap()
    }

    async fn check(&self, item: &super::model::Automation) -> super::model::Automation {
        super::attention::check(&self.server, item, item.next_run_at.unwrap())
            .await
            .unwrap();
        self.get(&item.id)
    }
}

#[tokio::test]
async fn attention_saved_file_watch_exposes_due_time_and_empty_observation_log() {
    let fixture = Fixture::new();
    let file = fixture.root.join("progress.txt");
    std::fs::write(&file, b"in progress").unwrap();
    let (_, result) = fixture
        .request(
            "POST",
            "/api/automations",
            fixture.input(json!({
                "kind":"file","path":file
            })),
        )
        .await;
    let item = &result["automation"];
    assert_eq!(item["attention"]["status"], "waiting", "{result}");
    assert!(item["next_run_at"].is_i64());
    let id = item["id"].as_str().unwrap();
    let (status, observations) = fixture
        .request(
            "GET",
            &format!("/api/automations/{id}/observations"),
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(observations["observations"], json!([]));
}

#[tokio::test]
async fn attention_http_uses_gateway_preserves_evidence_and_only_acts_on_new_samples() {
    use super::attention::{AttentionAction, AttentionStatus, ObservationStatus};

    let f = Fixture::new();
    let source = Arc::new(Mutex::new(("first", "Wed, 21 Oct 2015 07:28:00 GMT")));
    let reads = Arc::new(AtomicUsize::new(0));
    let model_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let executions = Arc::new(Mutex::new(Vec::<Value>::new()));
    let app = Router::new().route("/status", get({
        let source = source.clone();
        let reads = reads.clone();
        move || {
            reads.fetch_add(1, Ordering::SeqCst);
            let (body, timestamp) = *source.lock().unwrap();
            async move { ([("last-modified", timestamp)], body) }
        }
    })).route("/chat/completions", post({
        let model_calls = model_calls.clone();
        let executions = executions.clone();
        move |Json(body): Json<Value>| {
            let model_calls = model_calls.clone();
            let executions = executions.clone();
            async move {
                let usage = json!({"prompt_tokens":20,"completion_tokens":20,"total_tokens":40});
                assert_eq!(body["model"], "fixture-workspace-watch");
                if body["stream"] == true {
                    executions.lock().unwrap().push(body);
                    let chunk = json!({"id":"turn","choices":[{"index":0,"delta":{"role":"assistant","content":"已核对本轮进展"},"finish_reason":null}]});
                    let done = json!({"id":"turn","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":usage});
                    return ([("content-type","text/event-stream")], format!("data: {chunk}\n\ndata: {done}\n\ndata: [DONE]\n\n")).into_response();
                }
                let mut calls = model_calls.lock().unwrap();
                calls.push(body);
                let (action, seconds) = match calls.len() {
                    1 => ("defer", Some(10)),
                    2 => ("act", Some(2)),
                    3 => ("stop", None),
                    _ => panic!("unexpected additional model decision"),
                };
                let decision = json!({"action":action,"reason":"本地夹具来源判断","recheck_in_secs":seconds,"proposal":null});
                Json(json!({"id":"decision","choices":[{"index":0,"message":{"role":"assistant","content":decision.to_string()},"finish_reason":"stop"}],"usage":usage})).into_response()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let http = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let reference = SecretRef::new("attention.fixture.key");
    f.server
        .secret_store
        .put(
            &reference,
            secrecy::SecretString::from("fixture-only".to_owned()),
        )
        .await
        .unwrap();
    {
        let mut config = f.server.config.write();
        config.model.provider = "openai".into();
        config.model.name = "fixture-global-watch".into();
        config.model.base_url = origin.clone();
        config.model.api_key_ref = Some(reference);
        config.model.api_key_env.clear();
        config.sandbox.profile = crate::config::types::SandboxProfile::Open;
        config.permissions.mode = crate::config::types::PermissionMode::FullAccess;
    }
    f.server
        .db
        .create_llm_model(&crate::db::LlmModelInput {
            id: "watch-workspace-model".into(),
            provider: "openai".into(),
            label: "Workspace watch fixture".into(),
            model: "fixture-workspace-watch".into(),
            base_url: origin.clone(),
            api_format: "openai".into(),
            api_key_ref: "attention.fixture.key".into(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 600,
            invoke_timeout_ms: 10_000,
        })
        .unwrap();
    let workspace_settings = format!("/api/workspaces/{}/service-settings/chat", f.workspace.id);
    let (status, _) = f
        .request(
            "PUT",
            &workspace_settings,
            json!({
                "expected_revision":0, "override":{"source":"profile","id":"watch-workspace-model"}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    f.server
        .db
        .create_grant(&SecurityGrant {
            id: "attention-loopback-fixture".into(),
            subject_id: "local-user".into(),
            effect: GrantEffect::Allow,
            permission: PermissionId::NetworkRequest,
            resource: GrantResource::Network {
                scheme: Some("http".into()),
                host: "127.0.0.1".into(),
                port: Some(address.port()),
                methods: vec!["GET".into()],
                zone: NetworkZone::Loopback,
            },
            source: GrantSource::User,
            created_at: 1,
            expires_at: None,
        })
        .unwrap();
    let initial = f
        .create(json!({
            "kind":"http","url":format!("{origin}/status?private=never-log"),"send_preview":false
        }))
        .await;
    let first = f.check(&initial).await;
    assert_eq!(
        first.attention.as_ref().unwrap().status,
        AttentionStatus::Deferred,
        "{first:?}"
    );
    assert_eq!(
        first.attention.as_ref().unwrap().decision,
        Some(AttentionAction::Defer)
    );
    assert_eq!(
        f.server
            .db
            .list_automation_observations(&initial.id)
            .unwrap()
            .len(),
        1
    );
    let same = f.check(&first).await;
    assert_eq!(
        same.attention.as_ref().unwrap().status,
        AttentionStatus::Unchanged
    );
    assert!(same.next_run_at.unwrap() >= first.next_run_at.unwrap() + 20_000);
    assert_eq!(model_calls.lock().unwrap().len(), 1);
    assert_eq!(
        f.server
            .db
            .list_automation_observations(&initial.id)
            .unwrap()
            .len(),
        1
    );

    *source.lock().unwrap() = ("second", "Thu, 22 Oct 2015 07:28:00 GMT");
    let acted = f.check(&same).await;
    assert_eq!(
        acted.attention.as_ref().unwrap().decision,
        Some(AttentionAction::Act)
    );
    let item = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let items = f.server.db.automation_history(&initial.id).unwrap();
            if let Some(item) = items
                .first()
                .filter(|item| !matches!(item.status.as_str(), "dispatching" | "running"))
            {
                break item.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(item.trigger, "attention");
    assert_eq!(item.status, "completed", "{item:?}");
    let executions = executions.lock().unwrap();
    assert_eq!(executions.len(), 1);
    assert!(executions[0].to_string().contains("核对已保存目标的进展"));
    assert!(!executions[0].to_string().contains("never-log"));
    drop(executions);

    *source.lock().unwrap() = ("out of order", "Wed, 21 Oct 2015 07:28:00 GMT");
    let conflict = f.check(&acted).await;
    assert_eq!(
        conflict.attention.as_ref().unwrap().status,
        AttentionStatus::Conflict
    );
    assert_eq!(model_calls.lock().unwrap().len(), 2);
    *source.lock().unwrap() = ("third", "Fri, 23 Oct 2015 07:28:00 GMT");
    let stopped = f.check(&conflict).await;
    assert!(!stopped.enabled && stopped.next_run_at.is_none());
    assert_eq!(
        stopped.attention.as_ref().unwrap().decision,
        Some(AttentionAction::Stop)
    );
    assert_eq!(model_calls.lock().unwrap().len(), 3);
    let log = f
        .server
        .db
        .list_automation_observations(&initial.id)
        .unwrap();
    assert_eq!(log.len(), 4);
    assert!(matches!(log[1].status, ObservationStatus::Conflict));
    assert_eq!(
        log[1].previous_observation_id.as_deref(),
        Some(log[2].id.as_str())
    );
    assert!(log
        .iter()
        .all(|item| item.source_at.is_some() && item.source == format!("{origin}/status")));
    super::attention::check(&f.server, &conflict, conflict.next_run_at.unwrap())
        .await
        .unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 5);
    assert_eq!(
        f.server.db.automation_history(&initial.id).unwrap().len(),
        1
    );
    http.abort();
}

#[tokio::test]
async fn attention_file_claim_is_durable_and_user_pause_discards_a_late_result() {
    use super::attention::AttentionStatus;
    let f = Fixture::new();
    let source = f.root.join("watch.txt");
    std::fs::write(&source, b"fixture").unwrap();
    let original = f.create(json!({"kind":"file","path":source})).await;
    let now = original.next_run_at.unwrap();
    let pending = f
        .server
        .db
        .claim_attention_check(&original, now)
        .unwrap()
        .unwrap();
    assert!(f
        .server
        .db
        .claim_attention_check(&original, now)
        .unwrap()
        .is_none());
    assert_eq!(
        f.get(&original.id).attention.unwrap().status,
        AttentionStatus::Checking
    );
    let (status, _) = f
        .request(
            "POST",
            &format!("/api/automations/{}/enabled", original.id),
            json!({"enabled":false,"expected_revision":original.revision}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(f
        .server
        .db
        .settle_attention_check(
            &pending,
            pending.attention.clone().unwrap(),
            Some(now + 1000),
            None,
            None,
        )
        .unwrap()
        .is_none());
    let paused = f.get(&original.id);
    assert!(!paused.enabled && paused.next_run_at.is_none());
    assert_eq!(paused.attention.unwrap().status, AttentionStatus::Stopped);
}

#[tokio::test]
async fn attention_scheduler_collects_real_file_metadata_and_keeps_proposed_goal_for_selection() {
    use super::attention::{AttentionAction, AttentionStatus};
    let f = Fixture::new();
    let file = f.root.join("receipt.txt");
    std::fs::write(&file, b"local-text-that-must-stay-local").unwrap();
    let prompt = Arc::new(Mutex::new(None::<Value>));
    let route = Router::new().route(
        "/chat/completions",
        post({
            let prompt = prompt.clone();
            move |Json(body): Json<Value>| {
                prompt.lock().unwrap().replace(body);
                async {
                    let proposal = json!({
                        "action":"propose","reason":"来源提示需安排新的采集验收",
                        "recheck_in_secs": 90, "proposal":"核对附件后更新验收标准"
                    });
                    Json(json!({"id":"proposal","choices":[
                    {"index":0,"message":{"role":"assistant","content":proposal.to_string()},
                    "finish_reason":"stop"}
                ],"usage":{"prompt_tokens":20,"completion_tokens":20,"total_tokens":40}}))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture_model = tokio::spawn(async move { axum::serve(listener, route).await.unwrap() });
    let reference = SecretRef::new("attention.fixture.file");
    f.server
        .secret_store
        .put(
            &reference,
            secrecy::SecretString::from("fixture-only".to_owned()),
        )
        .await
        .unwrap();
    {
        let mut config = f.server.config.write();
        config.model.provider = "openai".into();
        config.model.name = "fixture-watch".into();
        config.model.base_url = format!("http://{address}");
        config.model.api_key_ref = Some(reference);
        config.model.api_key_env.clear();
        config.permissions.mode = crate::config::types::PermissionMode::FullAccess;
    }
    let initial = f.create(json!({"kind":"file","path":file})).await;
    let shutdown = tokio_util::sync::CancellationToken::new();
    let scheduler = super::start_scheduler(f.server.clone(), shutdown.clone());
    let state = tokio::time::timeout(std::time::Duration::from_secs(12), async {
        loop {
            let a = f.get(&initial.id);
            if let Some(state) = a.attention.filter(|s| {
                !matches!(
                    s.status,
                    AttentionStatus::Waiting | AttentionStatus::Checking
                )
            }) {
                break state;
            }
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
    })
    .await
    .unwrap();
    shutdown.cancel();
    scheduler.await.unwrap();
    assert_eq!(state.status, AttentionStatus::Proposed, "{state:?}");
    assert_eq!(state.decision, Some(AttentionAction::Propose));
    let proposal = state.proposal.unwrap();
    assert_eq!(proposal.goal, "核对附件后更新验收标准");
    assert!(f
        .server
        .db
        .automation_history(&initial.id)
        .unwrap()
        .is_empty());
    let logs = f
        .server
        .db
        .list_automation_observations(&initial.id)
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(proposal.observation_id, logs[0].id);
    assert_eq!(logs[0].source, file.to_string_lossy());
    assert!(logs[0].source_at.is_some() && logs[0].fingerprint.is_some());
    let sent = prompt.lock().unwrap().take().unwrap().to_string();
    assert!(sent.contains("receipt.txt") && !sent.contains("local-text-that-must-stay-local"));
    fixture_model.abort();
}
