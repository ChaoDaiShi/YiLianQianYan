//! Execution lifecycle: cancellation, pause, dispatch, rerun and attempt
//! history over a real task world.

use super::super::*;
use super::*;

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
        assert_eq!(
            settled.status,
            crate::modules::task::NodeExecutionStatus::Cancelled
        );
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
        crate::modules::task::NodeExecutionStatus::Succeeded
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
        crate::modules::task::NodeExecutionStatus::Cancelled
    );
    assert_eq!(
        detail.checkpoints[0].checkpoint_id,
        checkpoint.id.to_string()
    );
    let graph: TaskGraph = reloaded.get_graph(&id).unwrap();
    assert_eq!(graph.revision.value(), 2);
}
