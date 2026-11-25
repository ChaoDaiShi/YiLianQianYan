use super::*;
use crate::integrations::secret::InMemorySecretStore;
use crate::modules::task::{ExecutorResolver, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind};
use crate::modules::workflow::{
    WorkflowEdgeDefinition, WorkflowGraphDefinition, WorkflowNodeConfig, WorkflowNodeDefinition,
    WorkflowNodeId, WorkflowNodeKind, WORKFLOW_GRAPH_SCHEMA_VERSION,
};
use crate::safety::ControlSession;
use serde_json::json;

struct Fixture {
    server: Arc<AppServer>,
    root: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yilian-integration-workflow-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("state.db"),
                root.to_str().unwrap(),
                ControlSession::generate(),
                Arc::new(InMemorySecretStore::new()),
            )
            .unwrap(),
        );
        server.config.write().permissions.mode = crate::config::types::PermissionMode::FullApproval;
        Self { server, root }
    }
    fn save_workflow(&self) {
        let definition = WorkflowGraphDefinition {
            schema_version: WORKFLOW_GRAPH_SCHEMA_VERSION,
            entry_node_id: WorkflowNodeId::new("write").unwrap(),
            nodes: vec![
                WorkflowNodeDefinition {
                    id: WorkflowNodeId::new("write").unwrap(),
                    kind: WorkflowNodeKind::Tool,
                    config: WorkflowNodeConfig::Tool {
                        tool_name: "write_file".into(),
                        arguments: json!({"path":self.root.join("result.txt").to_str().unwrap(),"content":"one controlled write"}),
                    },
                },
                WorkflowNodeDefinition {
                    id: WorkflowNodeId::new("done").unwrap(),
                    kind: WorkflowNodeKind::Output,
                    config: WorkflowNodeConfig::Output { template: None },
                },
            ],
            edges: vec![WorkflowEdgeDefinition {
                from: WorkflowNodeId::new("write").unwrap(),
                to: WorkflowNodeId::new("done").unwrap(),
            }],
        };
        self.server
            .db
            .create_workflow_graph(&crate::db::WorkflowGraphRecord {
                id: "controlled-workflow".into(),
                name: "Controlled workflow".into(),
                description: String::new(),
                definition,
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
    }
    fn save_parent(&self) -> NodeExecution {
        self.save_workflow();
        let graph = TaskGraphId::new("parent-graph").unwrap();
        let node = TaskNodeId::new("parent-node").unwrap();
        self.server
            .task_world
            .create_graph(
                graph.clone(),
                vec![TaskNode::new(
                    node.clone(),
                    TaskNodeKind::Work,
                    "Write an authorized local report",
                    json!({"executor_ref":"workflow://controlled-workflow"}),
                )
                .unwrap()
                .with_retry_policy(crate::modules::task::RetryPolicy::new(3).unwrap())],
                vec![],
                1,
            )
            .unwrap();
        self.server
            .task_world
            .start_execution_with_resolver(
                &graph,
                &node,
                1,
                ExecutorResolver::new().with_workflow("controlled-workflow"),
                2,
            )
            .unwrap()
    }
    async fn legacy_waiting(
        &self,
    ) -> (
        crate::modules::task::model::Task,
        crate::modules::task::model::TaskExecution,
        PendingApproval,
    ) {
        use crate::modules::task::model::{
            Task, TaskExecution, TaskExecutionId, TaskId, TaskPriority,
        };
        self.save_workflow();
        let workspace = crate::workspace::Workspace::new(
            crate::workspace::WorkspaceId::generate(),
            "Legacy controlled workspace".into(),
            String::new(),
            Some(self.root.to_str().unwrap().into()),
            1,
        )
        .unwrap();
        self.server.db.create_workspace(&workspace).unwrap();
        let task = Task::new(
            TaskId::generate(),
            workspace.id,
            "Legacy workflow task".into(),
            String::new(),
            TaskPriority::Normal,
            Some("controlled-workflow".into()),
            None,
            1,
        )
        .unwrap();
        self.server.db.create_task(&task).unwrap();
        let execution = TaskExecution::new(
            TaskExecutionId::generate(),
            task.id.clone(),
            crate::execution::ExecutionContext::new(
                crate::execution::ExecutionId::generate(),
                "local-user",
                "task-runner",
                None,
                1,
            ),
            1,
            1,
        );
        self.server.db.create_task_execution(&execution).unwrap();
        let orchestrator =
            crate::modules::task::orchestrator::build_task_orchestrator(&self.server).await;
        orchestrator
            .run(&task.id, &execution.id, &CancellationToken::new())
            .await
            .unwrap();
        let task = self.server.db.get_task(&task.id).unwrap().unwrap();
        let execution = self
            .server
            .db
            .get_task_execution(&execution.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            task.status,
            crate::modules::task::model::TaskStatus::WaitingApproval,
            "legacy execution diagnostic: {:?}",
            execution.error
        );
        assert!(!self.root.join("result.txt").exists());
        let approvals = self.server.approval_store.list_pending();
        assert_eq!(approvals.len(), 1);
        (task, execution, approvals[0].clone())
    }
    async fn waiting(&self) -> (NodeExecution, PendingApproval) {
        let parent = self.save_parent();
        let result = crate::modules::task::application::execution_service::dispatch_execution(
            Arc::clone(&self.server),
            parent.graph_id.clone(),
            parent.id.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            result.status,
            NodeExecutionStatus::WaitingApproval,
            "{:?}",
            result
        );
        assert!(!self.root.join("result.txt").exists());
        let approvals = self.server.approval_store.list_pending();
        assert_eq!(approvals.len(), 1);
        assert_eq!(
            result.approval_ref.as_deref(),
            Some(approvals[0].approval_id.as_str())
        );
        (result, approvals[0].clone())
    }
}

#[path = "task_parent_tests/legacy_tasks.rs"]
mod legacy_tasks;
#[path = "task_parent_tests/recovery.rs"]
mod recovery;
#[path = "task_parent_tests/task_harness.rs"]
mod task_harness;
