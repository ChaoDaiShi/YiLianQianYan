use super::*;

use crate::modules::task::application::execution_service::{dispatch_execution, executor_resolver};
use crate::task::{GraphRevision, TaskGraph, TaskWorldRuntime};
use axum::body::to_bytes;

fn server() -> Arc<AppServer> {
    let server = AppServer::new_with_control_session(
        std::path::Path::new(":memory:"),
        ".",
        crate::safety::ControlSession::new(uuid::Uuid::new_v4().to_string().repeat(2)).unwrap(),
    )
    .unwrap();
    {
        let mut config = server.config.write();
        config.model.api_key.clear();
        config.model.api_key_env.clear();
        config.model.api_key_ref = None;
    }
    Arc::new(server)
}

async fn body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn cancellation_and_pause_stop_real_workflow_after_inflight_model_returns() {
    for pause in [false, true] {
        let server = server();
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let provider = axum::Router::new().route("/chat/completions", axum::routing::post({
                let entered = entered.clone();
                let release = release.clone();
                move || { let entered = entered.clone(); let release = release.clone(); async move {
                    entered.notify_one();
                    release.notified().await;
                    Json(json!({"id":"blocking-model-test","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Local model result"}}]}))
                }}
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let provider_handle = tokio::spawn(async move {
            axum::serve(listener, provider).await.unwrap();
        });
        {
            let mut config = server.config.write();
            config.model.base_url = format!("http://{address}");
            config.model.api_key = "local-test-placeholder".into();
            config.model.invoke_timeout_ms = 5000;
        }
        server.db.create_workflow_graph(&crate::db::WorkflowGraphRecord {
                id: "cancellable".into(), name: "Cancellable".into(), description: String::new(), created_at: 1, updated_at: 1,
                definition: serde_json::from_value(json!({"schema_version":1,"entry_node_id":"model","nodes":[
                    {"id":"model","kind":"agent","config":{"type":"agent","prompt":"Return local test text"}},
                    {"id":"after","kind":"output","config":{"type":"output","template":null}}
                ],"edges":[{"from":"model","to":"after"}]})).unwrap(),
            }).unwrap();
        let id = TaskGraphId::new("cancel-graph").unwrap();
        let node_id = TaskNodeId::new("work").unwrap();
        server
            .task_world
            .create_graph(
                id.clone(),
                vec![TaskNode::new(
                    node_id.clone(),
                    TaskNodeKind::Work,
                    "Cancellable",
                    json!({"executor_ref":"workflow://cancellable"}),
                )
                .unwrap()],
                vec![],
                1,
            )
            .unwrap();
        let execution = server
            .task_world
            .start_execution_with_resolver(
                &id,
                &node_id,
                1,
                executor_resolver(&server, &id, &node_id).unwrap(),
                2,
            )
            .unwrap();
        let checkpoint = server.task_world.checkpoint(&id, 1, 2).unwrap();
        let running = tokio::spawn(dispatch_execution(
            server.clone(),
            id.clone(),
            execution.id.clone(),
        ));
        tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        assert!(server
            .task_world
            .cancel_execution(&id, &execution.id, 99, 3)
            .is_err());
        if pause {
            server.task_world.pause_task(&id, 4).unwrap();
        } else {
            server
                .task_world
                .cancel_execution(&id, &execution.id, 1, 4)
                .unwrap();
        }
        if pause {
            server.task_world.resume_task(&id, 5).unwrap();
        }
        let graph_before = server.task_world.get_graph(&id).unwrap();
        let history_before = serde_json::to_value(
            server
                .task_world
                .list_node_executions(&id, &node_id)
                .unwrap(),
        )
        .unwrap();
        let mut changed = graph_before.nodes[0].clone();
        changed.title = "Must remain blocked until provider settles".into();
        assert!(server.task_world.update_node(&id, changed, 1, 6).is_err());
        assert!(server.task_world.delete_node(&id, &node_id, 1, 6).is_err());
        assert!(server
            .task_world
            .restore(&id, checkpoint.id.as_str(), 1, 6)
            .is_err());
        assert!(server
            .task_world
            .prepare_rerun_from_node(&id, &node_id, 1, 6)
            .is_err());
        assert!(server
            .task_world
            .start_execution_with_resolver(
                &id,
                &node_id,
                1,
                executor_resolver(&server, &id, &node_id).unwrap(),
                6
            )
            .is_err());
        assert_eq!(server.task_world.get_graph(&id).unwrap(), graph_before);
        assert_eq!(
            serde_json::to_value(
                server
                    .task_world
                    .list_node_executions(&id, &node_id)
                    .unwrap()
            )
            .unwrap(),
            history_before
        );
        release.notify_one();
        let settled = tokio::time::timeout(std::time::Duration::from_secs(5), running)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(settled.status, crate::task::NodeExecutionStatus::Cancelled);
        assert!(server
            .task_world
            .execution_cancellation_token(&execution.id)
            .is_none());
        let runs = server
            .db
            .list_workflow_runs(&crate::db::WorkflowRunQuery::default())
            .unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(
            runs[0].run.status,
            crate::modules::workflow::WorkflowRunStatus::Cancelled
        );
        assert_ne!(
            runs[0].run.node_states[1].status,
            crate::modules::workflow::NodeRunStatus::Completed
        );
        assert_eq!(
            server
                .task_world
                .list_node_executions(&id, &node_id)
                .unwrap()
                .len(),
            1
        );
        server
            .task_world
            .prepare_rerun_from_node(&id, &node_id, 1, 7)
            .unwrap();
        let retry = server
            .task_world
            .start_execution_with_resolver(
                &id,
                &node_id,
                1,
                executor_resolver(&server, &id, &node_id).unwrap(),
                8,
            )
            .unwrap();
        assert_eq!(retry.attempt, 2);
        server
            .task_world
            .cancel_execution(&id, &retry.id, 1, 9)
            .unwrap();
        provider_handle.abort();
    }
}

#[tokio::test]
async fn review_restore_executes_restored_definitions_and_keeps_archived_attempts() {
    let server = server();
    for workflow in ["primary", "alternate"] {
        server.db.create_workflow_graph(&crate::db::WorkflowGraphRecord {
                id: workflow.into(), name: workflow.into(), description: String::new(), created_at: 1, updated_at: 1,
                definition: serde_json::from_value(json!({"schema_version":1,"entry_node_id":"out","nodes":[{"id":"out","kind":"output","config":{"type":"output","template":null}}],"edges":[]})).unwrap(),
            }).unwrap();
    }
    let id = TaskGraphId::new("restored-execution").unwrap();
    let a = TaskNodeId::new("a").unwrap();
    let b = TaskNodeId::new("b").unwrap();
    let c = TaskNodeId::new("c").unwrap();
    let nodes = [&a, &b].into_iter().map(|node_id| TaskNode::new(node_id.clone(), TaskNodeKind::Work, format!("Original {node_id}"), json!({"executor_ref":"workflow://primary", "instruction":format!("original-{node_id}")})).unwrap()).collect();
    server
        .task_world
        .create_graph(id.clone(), nodes, vec![], 1)
        .unwrap();
    let checkpoint = server.task_world.checkpoint(&id, 1, 2).unwrap();
    let mut view = server.task_world.get_canvas_view(&id).unwrap();
    view.viewport.x = 42.0;
    let view_revision = server
        .task_world
        .save_canvas_view(&id, view.clone(), view.view_revision, 2)
        .unwrap()
        .view_revision;
    server
        .task_world
        .update_node(
            &id,
            TaskNode::new(
                a.clone(),
                TaskNodeKind::Work,
                "Changed",
                json!({"executor_ref":"workflow://alternate","instruction":"changed-A"}),
            )
            .unwrap(),
            1,
            3,
        )
        .unwrap();
    server.task_world.delete_node(&id, &b, 2, 4).unwrap();
    server
        .task_world
        .add_node(
            &id,
            TaskNode::new(
                c.clone(),
                TaskNodeKind::Work,
                "Archived",
                json!({"executor_ref":"workflow://alternate"}),
            )
            .unwrap(),
            3,
            5,
        )
        .unwrap();
    let archived = server
        .task_world
        .start_execution_with_resolver(&id, &c, 4, executor_resolver(&server, &id, &c).unwrap(), 6)
        .unwrap();
    dispatch_execution(server.clone(), id.clone(), archived.id.clone())
        .await
        .unwrap();
    let restored = server
        .task_world
        .restore(&id, checkpoint.id.as_str(), 4, 7)
        .unwrap();
    assert_eq!(restored.revision.value(), 5);
    assert!(restored.node(&c).is_none());
    assert_eq!(
        server
            .task_world
            .get_canvas_view(&id)
            .unwrap()
            .view_revision,
        view_revision
    );
    for node in [&b, &a] {
        let attempt = server
            .task_world
            .start_execution_with_resolver(
                &id,
                node,
                5,
                executor_resolver(&server, &id, node).unwrap(),
                8,
            )
            .unwrap();
        assert_eq!(attempt.context.instructions, format!("original-{node}"));
        assert_eq!(
            attempt.executor_ref.as_ref().unwrap().as_str(),
            "workflow://primary"
        );
        let done = dispatch_execution(server.clone(), id.clone(), attempt.id)
            .await
            .unwrap();
        assert_eq!(done.status, crate::task::NodeExecutionStatus::Succeeded);
        assert_eq!(done.output.unwrap()["workflow_graph_id"], "primary");
    }
    let reloaded =
        TaskWorldRuntime::new(&server.db, crate::shared::event::EventHub::new(16)).unwrap();
    assert_eq!(reloaded.get_graph(&id).unwrap(), restored);
    assert_eq!(
        reloaded.list_node_executions(&id, &c).unwrap()[0].id,
        archived.id
    );
    assert_eq!(reloaded.list_node_executions(&id, &b).unwrap().len(), 1);
}

#[tokio::test]
async fn missing_model_credentials_never_persist_a_partial_plan() {
    let server = server();
    let response = create_graph(
        State(server.clone()),
        Json(CreateGraphRequest {
            id: "missing-model".into(),
            goal: Some("Organize an editable plan".into()),
            nodes: vec![],
            edges: vec![],
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body(response).await["error"], "planner_unavailable");
    assert!(server.task_world.list_graphs().is_empty());
}

#[tokio::test]
async fn configured_llm_path_validates_before_persisting() {
    let server = server();
    let provider = axum::Router::new().route("/chat/completions", axum::routing::post(|Json(request): Json<Value>| async move {
            assert!(request.get("tools").is_none());
            let input: Value = serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
            let content = if input["goal"] == "malformed" { "not a graph".into() } else {
                json!({"schema_version":1,"id":input["graph_id"],"revision":1,"nodes":[{
                    "id":"draft","kind":"work","title":"Editable task","input":{"instruction":"Organize source material","acceptance_criteria":[]},"retry_policy":{"max_attempts":1}
                }],"edges":[]}).to_string()
            };
            Json(json!({"id":"local-provider-test","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":content}}]}))
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, provider).await.unwrap();
    });
    {
        let mut config = server.config.write();
        config.model.base_url = format!("http://{address}");
        config.model.api_key = "local-test-placeholder".into();
        config.model.invoke_timeout_ms = 2000;
    }
    for (id, goal, expected) in [
        ("bad", "malformed", StatusCode::UNPROCESSABLE_ENTITY),
        ("planned", "organize", StatusCode::CREATED),
    ] {
        let response = create_graph(
            State(server.clone()),
            Json(CreateGraphRequest {
                id: id.into(),
                goal: Some(goal.into()),
                nodes: vec![],
                edges: vec![],
            }),
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    let reloaded =
        TaskWorldRuntime::new(&server.db, crate::shared::event::EventHub::new(16)).unwrap();
    assert_eq!(reloaded.list_graphs().len(), 1);
    assert_eq!(reloaded.list_graphs()[0].id.as_str(), "planned");
    handle.abort();
}

#[tokio::test]
async fn active_model_profile_drives_planning_and_review() {
    use crate::db::LlmModelInput;
    use crate::integrations::secret::SecretRef;
    use secrecy::SecretString;

    let server = server();
    let provider = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|Json(request): Json<Value>| async move {
                assert!(request.get("tools").is_none());
                let system = request["messages"][0]["content"].as_str().unwrap_or_default();
                let content = if system.contains("TaskGraph 审查器") {
                    json!({
                        "summary":"图结构清晰",
                        "suggestions":[]
                    })
                    .to_string()
                } else {
                    let input: Value = serde_json::from_str(
                        request["messages"][1]["content"].as_str().unwrap(),
                    )
                    .unwrap();
                    json!({
                        "schema_version":1,
                        "id":input["graph_id"],
                        "revision":1,
                        "nodes":[{
                            "id":"draft",
                            "kind":"work",
                            "title":"Editable task",
                            "input":{"instruction":"Use the active model profile","acceptance_criteria":[]},
                            "retry_policy":{"max_attempts":1}
                        }],
                        "edges":[]
                    })
                    .to_string()
                };
                Json(json!({
                    "id":"active-profile-test",
                    "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":content}}]
                }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, provider).await.unwrap();
    });

    let secret_ref = SecretRef::new("llm.rc2-active-profile");
    server
        .secret_store
        .put(
            &secret_ref,
            SecretString::from("test-placeholder".to_string()),
        )
        .await
        .unwrap();
    server
        .db
        .create_llm_model(&LlmModelInput {
            id: "rc2-active".into(),
            provider: "custom".into(),
            label: "RC2 active".into(),
            model: "local-test-model".into(),
            base_url: format!("http://{address}"),
            api_format: "openai".into(),
            api_key_ref: secret_ref.key.clone(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 1024,
            invoke_timeout_ms: 2_000,
        })
        .unwrap();
    server
        .db
        .set_llm_model_verification("rc2-active", Some(1), None)
        .unwrap();
    server.db.activate_llm_model("rc2-active").unwrap();

    let planned = create_graph(
        State(server.clone()),
        Json(CreateGraphRequest {
            id: "active-profile-graph".into(),
            goal: Some("Use the selected provider".into()),
            nodes: vec![],
            edges: vec![],
        }),
    )
    .await;
    assert_eq!(planned.status(), StatusCode::CREATED);

    let reviewed = review_graph(
        State(server.clone()),
        Path("active-profile-graph".into()),
        Json(ReviewGraphRequest {
            expected_revision: 1,
        }),
    )
    .await;
    assert_eq!(reviewed.status(), StatusCode::OK);
    assert_eq!(body(reviewed).await["reviewed_revision"], 1);

    handle.abort();
}

#[tokio::test]
async fn real_workflow_harness_edit_rerun_checkpoint_and_attempt_history() {
    let server = server();
    let definition = serde_json::from_value(json!({
            "schema_version":1,"entry_node_id":"out","nodes":[{"id":"out","kind":"output","config":{"type":"output","template":null}}],"edges":[]
        })).unwrap();
    server
        .db
        .create_workflow_graph(&crate::db::WorkflowGraphRecord {
            id: "local-output".into(),
            name: "Local output".into(),
            description: "Test of the actual WorkflowRunner".into(),
            definition,
            created_at: 1,
            updated_at: 1,
        })
        .unwrap();
    let id = TaskGraphId::new("harness-flow").unwrap();
    let node_id = TaskNodeId::new("run").unwrap();
    let node = TaskNode::new(node_id.clone(), TaskNodeKind::Work, "Run workflow", json!({"executor_ref":"workflow://local-output","instruction":"Execute configured workflow"})).unwrap();
    server
        .task_world
        .create_graph(
            id.clone(),
            vec![
                node.clone(),
                TaskNode::new(
                    TaskNodeId::new("editable").unwrap(),
                    TaskNodeKind::Work,
                    "Editable only",
                    json!({}),
                )
                .unwrap(),
            ],
            vec![],
            1,
        )
        .unwrap();
    let checkpoint = server.task_world.checkpoint(&id, 1, 2).unwrap();
    let resolver = executor_resolver(&server, &id, &node_id).unwrap();
    let first = server
        .task_world
        .start_execution_with_resolver(&id, &node_id, 1, resolver, 3)
        .unwrap();
    let completed = dispatch_execution(server.clone(), id.clone(), first.id.clone())
        .await
        .unwrap();
    assert_eq!(
        completed.status,
        crate::task::NodeExecutionStatus::Succeeded
    );
    assert_eq!(
        completed.output.as_ref().unwrap()["workflow_graph_id"],
        "local-output"
    );
    let mut edited = node;
    edited.title = "Edited workflow task".into();
    let graph = server.task_world.update_node(&id, edited, 1, 4).unwrap();
    assert_eq!(graph.revision, GraphRevision::new(2).unwrap());
    assert!(server
        .task_world
        .rerun_from_node(&id, &node_id, 1, 5)
        .is_err());
    let response = rerun(
        State(server.clone()),
        Path(id.to_string()),
        Json(RerunRequest {
            expected_revision: 2,
            node_id: node_id.clone(),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(server
        .task_world
        .list_node_executions(&id, &TaskNodeId::new("editable").unwrap())
        .unwrap()
        .is_empty());
    let second = server
        .task_world
        .start_execution_with_resolver(
            &id,
            &node_id,
            2,
            executor_resolver(&server, &id, &node_id).unwrap(),
            7,
        )
        .unwrap();
    assert_ne!(first.id, second.id);
    server
        .task_world
        .cancel_execution(&id, &second.id, 2, 8)
        .unwrap();
    let reloaded =
        TaskWorldRuntime::new(&server.db, crate::shared::event::EventHub::new(16)).unwrap();
    let detail = reloaded.get_graph_detail(&id).unwrap();
    assert_eq!(detail.nodes[0].execution_history.len(), 2);
    assert_eq!(
        detail.nodes[0].execution_history[1].status,
        crate::task::NodeExecutionStatus::Cancelled
    );
    assert_eq!(
        detail.checkpoints[0].checkpoint_id,
        checkpoint.id.to_string()
    );
    let graph: TaskGraph = reloaded.get_graph(&id).unwrap();
    assert_eq!(graph.revision.value(), 2);
}
