//! Planning, review and model-profile selection.

use super::super::*;
use super::*;

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
        .start_execution_with_resolver(
            &id,
            &c,
            4,
            executor_resolver(&server, &id, &c).await.unwrap(),
            6,
        )
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
                executor_resolver(&server, &id, node).await.unwrap(),
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
        assert_eq!(
            done.status,
            crate::modules::task::NodeExecutionStatus::Succeeded
        );
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
