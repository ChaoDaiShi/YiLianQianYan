use std::sync::{Arc, Mutex};

use axum::{http::StatusCode, routing::post, Json, Router};
use serde_json::{json, Value};

use crate::{
    app::state::AppServer,
    db::{
        model_pools::{ModelPoolInput, ModelPoolTier},
        LlmModelInput, WorkflowGraphRecord,
    },
    execution::{ExecutionContext, ExecutionId},
    integrations::secret::{InMemorySecretStore, SecretRef, SecretStore},
    modules::task::{
        build_task_orchestrator, AgentDefinition, AgentId, AgentSource, Task, TaskExecution,
        TaskExecutionId, TaskExecutionStatus, TaskId, TaskPlan, TaskPlanExecutor, TaskPlanStep,
        TaskPriority,
    },
    safety::ControlSession,
    workspace::{Workspace, WorkspaceId},
};

use super::workspace_chat_tests::{request, TOKEN};

struct Fixture {
    server: Arc<AppServer>,
    router: Router,
    workspace: WorkspaceId,
    calls: Arc<Mutex<Vec<String>>>,
    provider: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("scoped-task-model-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(InMemorySecretStore::new());
        let key = SecretRef::new("fixture.workspace-task");
        store
            .put(&key, secrecy::SecretString::from("loopback-model-only"))
            .await
            .unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("test.db"),
                root.to_str().unwrap(),
                ControlSession::new(TOKEN).unwrap(),
                store,
            )
            .unwrap(),
        );
        let workspace = WorkspaceId::generate();
        server
            .db
            .create_workspace(
                &Workspace::new(
                    workspace.clone(),
                    "Scoped tasks".into(),
                    String::new(),
                    Some(root.to_string_lossy().into_owned()),
                    1,
                )
                .unwrap(),
            )
            .unwrap();
        let router = crate::app::router::build_router(server.clone());
        for id in ["agent-inherits", "agent-explicit"] {
            let agent = AgentDefinition {
                id: AgentId::new(id).unwrap(),
                name: id.into(),
                description: String::new(),
                instructions: String::new(),
                allowed_tools: vec![],
                capabilities: vec![],
                model: (id == "agent-explicit").then(|| "explicit-model".into()),
                max_iterations: 1,
                enabled: true,
                source: AgentSource::Database,
            };
            server.db.create_agent_definition(&agent).unwrap();
        }
        server.db.create_workflow_graph(&WorkflowGraphRecord {
            id: "scope-nested".into(), name: "Scoped nested text".into(),
            description: String::new(), created_at: 1, updated_at: 1,
            definition: serde_json::from_value(json!({
                "schema_version":1,"entry_node_id":"text","nodes":[
                  {"id":"text","kind":"agent","config":{"type":"agent","prompt":"nested workspace marker"}}
                ],"edges":[]
            })).unwrap(),
        }).unwrap();
        let descriptors = server.build_capability_registry().await.list();
        let proof_capability = descriptors
            .iter()
            .find(|item| item.name == "Scoped nested text")
            .unwrap()
            .id
            .to_string();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        for id in [
            "global-model",
            "workspace-model",
            "pool-model",
            "explicit-model",
        ] {
            server
                .db
                .create_llm_model(&LlmModelInput {
                    id: id.into(),
                    provider: "fixture".into(),
                    label: id.into(),
                    model: id.into(),
                    base_url: format!("http://{address}"),
                    api_format: "openai".into(),
                    api_key_ref: key.key.clone(),
                    api_key_env: String::new(),
                    temperature: 0.0,
                    max_tokens: 600,
                    invoke_timeout_ms: 5000,
                })
                .unwrap();
        }
        server.db.activate_llm_model("global-model").unwrap();
        {
            let mut cfg = server.config.write();
            cfg.model.provider = "fixture".into();
            cfg.model.name = "fallback-global-model".into();
            cfg.model.base_url = format!("http://{address}");
            cfg.model.api_key_ref = Some(key);
        }
        let calls = Arc::new(Mutex::new(vec![]));
        let observed = calls.clone();
        let provider = tokio::spawn(async move {
            let provider = Router::new().route("/chat/completions", post(move |Json(body): Json<Value>| {
                let observed = observed.clone();
                let proof_capability = proof_capability.clone();
                async move {
                    let model = body["model"].as_str().unwrap().to_owned();
                    observed.lock().unwrap().push(model.clone());
                    let system = body["messages"][0]["content"].as_str().unwrap_or("");
                        let content = if system.contains("关注决策") {
                            json!({"action":"propose","reason":"Source changed","recheck_in_secs":90,"proposal":"Review task evidence"}).to_string()
                        } else if system.contains("严谨的桌面任务规划器") {
                        json!({
                            "deliverables":[{"id":"report","title":"Report","mime_type":"text/plain"}],
                            "criteria":[{"id":"exists","description":"Report exists","required":true,"check":{"kind":"artifact_exists","deliverable_id":"report"}}],
                            "candidate":{"schema_version":1,"nodes":[{
                                "id":"draft","title":"Draft","instruction":"Create report",
                                "capability_id":proof_capability,"literal_input":{},"bindings":[]
                            }],"edges":[],"deliverable_nodes":[{"deliverable_id":"report","node_id":"draft"}]}
                        }).to_string()
                    } else if system.contains("你是任务规划器") {
                        json!({"schema_version":1,"summary":"Planned on workspace model","steps":[
                            {"id":"draft","title":"Draft","instruction":"Use inherited model","executor":{
                                "type":"agent","agent_id":"agent-inherits"
                            }}
                        ]}).to_string()
                    } else {
                        model
                    };
                    Json(json!({"id":"fixture-workspace-task","choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":20,"total_tokens":40}}))
                }
            }));
            axum::serve(listener, provider).await.unwrap();
        });
        Self {
            server,
            router,
            workspace,
            calls,
            provider,
        }
    }

    async fn override_chat(&self, source: &str, id: &str, revision: u64) {
        let path = format!("/api/workspaces/{}/service-settings/chat", self.workspace);
        let (status, saved) = request(
            &self.router,
            "PUT",
            &path,
            json!({
                "expected_revision":revision,"override":{"source":source,"id":id}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{saved}");
    }

    fn create_pool(&self) {
        self.server
            .db
            .save_model_pool(
                "scoped-task-pool",
                ModelPoolInput {
                    label: "Scoped task pool".into(),
                    service_kind: "chat".into(),
                    enabled: true,
                    attempt_timeout_ms: 5000,
                    total_timeout_ms: 6000,
                    expected_revision: None,
                    tiers: vec![ModelPoolTier {
                        id: "first".into(),
                        label: "First".into(),
                        profile_ids: vec!["pool-model".into()],
                    }],
                },
            )
            .unwrap();
    }

    async fn run_task(&self, executor: Option<TaskPlanExecutor>) {
        let task = Task::new(
            TaskId::generate(),
            self.workspace.clone(),
            "Scoped execution".into(),
            String::new(),
            TaskPriority::Normal,
            None,
            None,
            1,
        )
        .unwrap();
        let execution = TaskExecution::new(
            TaskExecutionId::generate(),
            task.id.clone(),
            ExecutionContext::new(
                ExecutionId::generate(),
                "local-user",
                "task-fixture",
                None,
                1,
            ),
            1,
            1,
        );
        self.server.db.create_task(&task).unwrap();
        self.server.db.create_task_execution(&execution).unwrap();
        if let Some(executor) = executor {
            self.server
                .db
                .create_task_plan(
                    &uuid::Uuid::new_v4().to_string(),
                    &task.id,
                    &execution.id,
                    &TaskPlan {
                        schema_version: 1,
                        summary: "Scope fixture".into(),
                        steps: vec![TaskPlanStep {
                            id: "step".into(),
                            title: "Step".into(),
                            instruction: "Run scoped".into(),
                            executor,
                        }],
                    },
                    1,
                )
                .unwrap();
        }
        build_task_orchestrator(&self.server)
            .await
            .run(
                &task.id,
                &execution.id,
                &tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        let saved = self
            .server
            .db
            .get_task_execution(&execution.id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, TaskExecutionStatus::Completed, "{saved:?}");
    }

    async fn draft(&self) {
        let (status, result) = request(
            &self.router,
            "POST",
            "/api/task-world/proofloop/drafts",
            json!({
                "workspace_id":self.workspace,"user_goal":"Create a report in scope"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{result}");
        assert_eq!(result["contract"]["workspace_id"], self.workspace.as_str());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.provider.abort();
    }
}

#[tokio::test]
async fn workspace_task_model_orchestrator_uses_profile_and_pool_with_explicit_agent_priority() {
    let f = Fixture::new().await;
    f.override_chat("profile", "workspace-model", 0).await;
    f.run_task(None).await;
    assert_eq!(
        *f.calls.lock().unwrap(),
        ["workspace-model", "workspace-model"]
    );
    f.create_pool();
    f.override_chat("pool", "scoped-task-pool", 1).await;
    f.run_task(Some(TaskPlanExecutor::Workflow {
        workflow_graph_id: "scope-nested".into(),
    }))
    .await;
    f.run_task(Some(TaskPlanExecutor::Agent {
        agent_id: AgentId::new("agent-explicit").unwrap(),
    }))
    .await;
    assert_eq!(
        *f.calls.lock().unwrap(),
        [
            "workspace-model",
            "workspace-model",
            "pool-model",
            "explicit-model"
        ]
    );
}

#[tokio::test]
async fn workspace_task_model_proofloop_draft_uses_configured_profile_and_pool() {
    let f = Fixture::new().await;
    f.override_chat("profile", "workspace-model", 0).await;
    f.draft().await;
    f.create_pool();
    f.override_chat("pool", "scoped-task-pool", 1).await;
    f.draft().await;
    assert_eq!(*f.calls.lock().unwrap(), ["workspace-model", "pool-model"]);
}

#[tokio::test]
async fn workspace_task_model_attention_source_survives_goal_edits_and_creates_one_draft() {
    let f = Fixture::new().await;
    let workspace = f.server.db.get_workspace(&f.workspace).unwrap().unwrap();
    let file =
        std::path::Path::new(workspace.root_path.as_ref().unwrap()).join("attention-source.txt");
    std::fs::write(&file, "fixture evidence").unwrap();
    f.server.config.write().permissions.mode = crate::config::types::PermissionMode::FullAccess;
    let (status, saved) = request(&f.router, "POST", "/api/automations", json!({
        "name": "Watch source", "workspace_id": f.workspace, "workflow_graph_id": "", "timezone": "UTC",
        "schedule": {"kind":"watch","source":{"kind":"file","path":file},"min_interval_secs":5,
            "expires_at": chrono::Utc::now().timestamp_millis() + 180_000, "max_checks":5,"max_retries":1},
        "target":{"kind":"personal_assistant","goal":"Watch fixture","max_runs":1},
        "enabled":true,
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let watch_id = saved["automation"]["id"].as_str().unwrap();
    let shutdown = tokio_util::sync::CancellationToken::new();
    let scheduler = crate::modules::automation::start_scheduler(f.server.clone(), shutdown.clone());
    let observation = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let watch = f.server.db.get_automation(watch_id).unwrap().unwrap();
            if let Some(proposal) = watch.attention.unwrap().proposal {
                break proposal.observation_id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    shutdown.cancel();
    scheduler.await.unwrap();
    let source = json!({"automation_id":watch_id,"observation_id":observation});
    let create = |workspace: &str, goal: &str, source: Value| {
        json!({
            "workspace_id":workspace,"user_goal":goal,"source":source,
        })
    };
    let path = "/api/task-world/proofloop/drafts";
    let (status, first) = request(
        &f.router,
        "POST",
        path,
        create(f.workspace.as_str(), "My edited goal", source.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(first["source"], source);
    assert_eq!(first["contract"]["user_goal"], "My edited goal");
    let (status, repeated) = request(
        &f.router,
        "POST",
        path,
        create(
            f.workspace.as_str(),
            "Different edited text",
            source.clone(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{repeated}");
    assert_eq!(repeated["draft_id"], first["draft_id"]);
    assert_eq!(repeated["contract"]["user_goal"], "My edited goal");
    let (status, _) = request(
        &f.router,
        "POST",
        path,
        create("invalid-scope", "Forged scope", source),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, existing) = request(
        &f.router,
        "GET",
        &format!("{path}/attention/{watch_id}/{observation}"),
        Value::Null,
    )
    .await;
    assert_eq!(existing["draft"]["draft_id"], first["draft_id"]);
    assert_eq!(f.calls.lock().unwrap().len(), 2);
}
