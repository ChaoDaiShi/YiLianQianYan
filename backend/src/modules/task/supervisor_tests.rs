use super::task_graph::*;
use super::task_supervisor::*;

use serde_json::json;

fn graph_id(raw: &str) -> TaskGraphId {
    TaskGraphId::new(raw).expect("valid graph id")
}

fn node_id(raw: &str) -> TaskNodeId {
    TaskNodeId::new(raw).expect("valid node id")
}

fn node(raw: &str) -> TaskNode {
    TaskNode::new(
        node_id(raw),
        TaskNodeKind::Work,
        format!("node {raw}"),
        json!({ "source": raw }),
    )
    .expect("valid task node")
}

fn focus_node(raw: &str) -> TaskNode {
    TaskNode::new(
        node_id(raw),
        TaskNodeKind::Work,
        format!("focus node {raw}"),
        json!({
            "executor_ref": "command://desktop.app.focus",
            "command_binding": {
                "command": "desktop.app.focus",
                "args": {"app_id": "app:code.exe"}
            }
        }),
    )
    .expect("valid focus task node")
}

fn edge(from: &str, to: &str) -> TaskEdge {
    TaskEdge::new(node_id(from), node_id(to))
}

fn graph(nodes: Vec<TaskNode>, edges: Vec<TaskEdge>) -> TaskGraph {
    TaskGraph::new(graph_id("graph-1"), GraphRevision::initial(), nodes, edges)
        .expect("valid task graph")
}

#[test]
fn supervisor_roots_are_runnable_in_definition_order() {
    let supervisor = TaskSupervisor::new(
        graph(
            vec![node("root-b"), node("root-a"), node("child")],
            vec![edge("root-a", "child")],
        ),
        1_000,
    )
    .expect("valid supervisor graph");

    assert_eq!(
        supervisor.runnable_nodes(),
        vec![node_id("root-b"), node_id("root-a")]
    );
}

#[test]
fn supervisor_waits_for_all_dependencies() {
    let mut supervisor = TaskSupervisor::new(
        graph(
            vec![node("a"), node("b"), node("join")],
            vec![edge("a", "join"), edge("b", "join")],
        ),
        1_000,
    )
    .expect("valid supervisor graph");

    supervisor.start_node(&node_id("a"), 1_001).unwrap();
    supervisor
        .succeed_node(&node_id("a"), json!("a-result"), 1_002)
        .unwrap();

    assert_eq!(supervisor.runnable_nodes(), vec![node_id("b")]);

    supervisor.start_node(&node_id("b"), 1_003).unwrap();
    supervisor
        .succeed_node(&node_id("b"), json!("b-result"), 1_004)
        .unwrap();

    assert_eq!(supervisor.runnable_nodes(), vec![node_id("join")]);
}

#[test]
fn supervisor_success_propagates_readiness() {
    let mut supervisor = TaskSupervisor::new(
        graph(vec![node("a"), node("b")], vec![edge("a", "b")]),
        1_000,
    )
    .expect("valid supervisor graph");

    supervisor.start_node(&node_id("a"), 1_001).unwrap();
    supervisor
        .succeed_node(&node_id("a"), json!({ "ok": true }), 1_002)
        .unwrap();

    assert_eq!(supervisor.runnable_nodes(), vec![node_id("b")]);
    assert_eq!(
        supervisor.node_state(&node_id("a")).unwrap().output,
        Some(json!({ "ok": true }))
    );
}

#[test]
fn supervisor_failure_blocks_only_descendants() {
    let mut supervisor = TaskSupervisor::new(
        graph(
            vec![node("a"), node("a-child"), node("b"), node("b-child")],
            vec![edge("a", "a-child"), edge("b", "b-child")],
        ),
        1_000,
    )
    .expect("valid supervisor graph");

    supervisor.start_node(&node_id("a"), 1_001).unwrap();
    supervisor
        .fail_node(&node_id("a"), "upstream failed", 1_002)
        .unwrap();

    assert_eq!(
        supervisor.node_state(&node_id("a-child")).unwrap().status,
        TaskNodeStatus::Blocked
    );
    assert_eq!(
        supervisor.node_state(&node_id("b")).unwrap().status,
        TaskNodeStatus::Runnable
    );
    assert_eq!(supervisor.runnable_nodes(), vec![node_id("b")]);
}

#[test]
fn supervisor_cancellation_blocks_only_descendants() {
    let mut supervisor = TaskSupervisor::new(
        graph(
            vec![node("a"), node("a-child"), node("b")],
            vec![edge("a", "a-child")],
        ),
        1_000,
    )
    .expect("valid supervisor graph");

    supervisor.cancel_node(&node_id("a"), 1_001).unwrap();

    assert_eq!(
        supervisor.node_state(&node_id("a-child")).unwrap().status,
        TaskNodeStatus::Blocked
    );
    assert_eq!(
        supervisor.node_state(&node_id("b")).unwrap().status,
        TaskNodeStatus::Runnable
    );
}

fn complete_branch(supervisor: &mut TaskSupervisor, id: &str, output: &str, now: i64) {
    supervisor.start_node(&node_id(id), now).unwrap();
    supervisor
        .succeed_node(&node_id(id), json!(output), now + 1)
        .unwrap();
}

#[test]
fn editing_upstream_invalidates_only_dependents() {
    let mut supervisor = TaskSupervisor::new(
        graph(
            vec![
                node("source-a"),
                node("child-a"),
                node("grandchild-a"),
                node("source-b"),
                node("child-b"),
            ],
            vec![
                edge("source-a", "child-a"),
                edge("child-a", "grandchild-a"),
                edge("source-b", "child-b"),
            ],
        ),
        1_000,
    )
    .expect("valid supervisor graph");

    complete_branch(&mut supervisor, "source-a", "old-a", 1_001);
    complete_branch(&mut supervisor, "child-a", "old-child-a", 1_003);
    complete_branch(&mut supervisor, "grandchild-a", "old-grandchild-a", 1_005);
    complete_branch(&mut supervisor, "source-b", "stable-b", 1_007);
    complete_branch(&mut supervisor, "child-b", "stable-child-b", 1_009);

    let old_revision = supervisor.graph_revision();
    let new_revision = supervisor
        .update_node_input(&node_id("source-a"), json!({ "source": "new-a" }), 1_011)
        .unwrap();

    assert!(new_revision > old_revision);
    for id in ["source-a", "child-a", "grandchild-a"] {
        let state = supervisor.node_state(&node_id(id)).unwrap();
        assert_eq!(state.status, TaskNodeStatus::Invalidated);
        assert!(state.output.is_none());
    }
    for (id, output) in [("source-b", "stable-b"), ("child-b", "stable-child-b")] {
        let state = supervisor.node_state(&node_id(id)).unwrap();
        assert_eq!(state.status, TaskNodeStatus::Succeeded);
        assert_eq!(state.output, Some(json!(output)));
    }
}

#[test]
fn checkpoint_restore_preserves_state_and_advances_revision() {
    let mut supervisor = TaskSupervisor::new(
        graph(vec![node("a"), node("b")], vec![edge("a", "b")]),
        1_000,
    )
    .expect("valid supervisor graph");
    complete_branch(&mut supervisor, "a", "before", 1_001);
    let checkpoint = supervisor.checkpoint(1_003);

    supervisor
        .update_node_input(&node_id("a"), json!({ "changed": true }), 1_004)
        .unwrap();
    let restored_revision = supervisor.restore(&checkpoint).unwrap();

    assert_eq!(restored_revision, supervisor.graph_revision());
    assert_eq!(
        restored_revision.value(),
        checkpoint.graph_revision.value() + 2
    );
    assert_eq!(
        supervisor.node_state(&node_id("a")).unwrap().status,
        TaskNodeStatus::Succeeded
    );
    assert_eq!(
        supervisor.node_state(&node_id("a")).unwrap().output,
        Some(json!("before"))
    );
}

#[test]
fn checkpoint_from_another_graph_is_rejected_atomically() {
    let mut supervisor =
        TaskSupervisor::new(graph(vec![node("a")], vec![]), 1_000).expect("valid supervisor graph");
    let other = TaskSupervisor::new(
        TaskGraph::new(
            graph_id("other-graph"),
            GraphRevision::initial(),
            vec![node("a")],
            vec![],
        )
        .unwrap(),
        1_000,
    )
    .expect("valid supervisor graph");
    let checkpoint = other.checkpoint(1_001);
    let before_revision = supervisor.graph_revision();

    assert!(matches!(
        supervisor.restore(&checkpoint),
        Err(TaskSupervisorError::CheckpointGraphMismatch { .. })
    ));
    assert_eq!(supervisor.graph_revision(), before_revision);
    assert_eq!(
        supervisor.node_state(&node_id("a")).unwrap().status,
        TaskNodeStatus::Runnable
    );
}

#[test]
fn supervisor_rejects_deserialized_zero_revision_graph_before_initializing_state() {
    let invalid_graph: TaskGraph = serde_json::from_value(json!({
        "schema_version": TASK_GRAPH_SCHEMA_VERSION,
        "id": "graph-1",
        "revision": 0,
        "nodes": [{
            "id": "a",
            "kind": "work",
            "title": "node a",
            "input": {},
            "retry_policy": { "max_attempts": 1 }
        }],
        "edges": []
    }))
    .expect("serde can construct the bypassed graph fixture");

    let result = TaskSupervisor::new(invalid_graph, 1_000);

    assert!(matches!(
        result,
        Err(TaskSupervisorError::Graph(
            TaskGraphValidationError::InvalidRevision(0)
        ))
    ));
}

#[test]
fn supervisor_graph_edits_validate_and_increment_revision() {
    let mut supervisor =
        TaskSupervisor::new(graph(vec![node("source"), node("stable")], vec![]), 1_000)
            .expect("valid supervisor graph");
    complete_branch(&mut supervisor, "stable", "stable-output", 1_001);
    let initial_revision = supervisor.graph_revision();

    let revision = supervisor.add_node(node("child"), 1_003).unwrap();
    assert_eq!(revision.value(), initial_revision.value() + 1);
    let revision = supervisor.add_edge(edge("source", "child"), 1_004).unwrap();
    assert_eq!(revision.value(), initial_revision.value() + 2);
    assert_eq!(
        supervisor.node_state(&node_id("stable")).unwrap().status,
        TaskNodeStatus::Succeeded
    );

    let updated = TaskNode::new(
        node_id("source"),
        TaskNodeKind::Work,
        "updated source",
        serde_json::json!({"updated": true}),
    )
    .unwrap();
    let revision = supervisor.update_node(updated, 1_005).unwrap();
    assert_eq!(revision.value(), initial_revision.value() + 3);
    assert_eq!(
        supervisor.node_state(&node_id("child")).unwrap().status,
        TaskNodeStatus::Invalidated
    );

    let revision = supervisor
        .remove_edge(&edge("source", "child"), 1_006)
        .unwrap();
    assert_eq!(revision.value(), initial_revision.value() + 4);
    let revision = supervisor.remove_node(&node_id("child"), 1_007).unwrap();
    assert_eq!(revision.value(), initial_revision.value() + 5);
    assert!(supervisor.node_state(&node_id("child")).is_none());
}

#[test]
fn restore_preserves_checkpoint_state_at_a_newer_monotonic_revision() {
    let mut supervisor = TaskSupervisor::new(graph(vec![node("source")], vec![]), 1_000).unwrap();
    let checkpoint = supervisor.checkpoint(1_001);
    supervisor
        .update_node_input(
            &node_id("source"),
            serde_json::json!({"changed": true}),
            1_002,
        )
        .unwrap();
    let current_revision = supervisor.graph_revision();

    let restored_revision = supervisor
        .restore(&checkpoint)
        .expect("restore should advance revision");

    assert_eq!(restored_revision.value(), current_revision.value() + 1);
    assert_eq!(supervisor.graph_revision(), restored_revision);
    assert_eq!(
        supervisor.graph().nodes[0].input,
        serde_json::json!({"source": "source"})
    );
    assert_eq!(
        supervisor.node_state(&node_id("source")).unwrap().status,
        TaskNodeStatus::Runnable
    );
}

#[test]
fn active_command_execution_blocks_graph_edits() {
    let mut supervisor = TaskSupervisor::new(graph(vec![focus_node("focus")], vec![]), 1_000)
        .expect("valid supervisor graph");
    supervisor.start_node(&node_id("focus"), 1_001).unwrap();
    supervisor
        .attach_command_execution(
            &node_id("focus"),
            TaskCommandExecution {
                request_id: "focus-request".into(),
                command: "desktop.app.focus".into(),
                app_id: "app:code.exe".into(),
                attempt: 1,
                graph_revision: GraphRevision::initial(),
                status: TaskCommandExecutionStatus::Dispatching,
                approval_id: None,
            },
        )
        .unwrap();

    let result = supervisor.update_node_input(
        &node_id("focus"),
        json!({"executor_ref": "command://desktop.app.focus"}),
        1_002,
    );
    assert!(matches!(
        result,
        Err(TaskSupervisorError::ActiveCommandExecution { .. })
    ));
    assert_eq!(supervisor.graph_revision(), GraphRevision::initial());
    assert_eq!(
        supervisor
            .node_state(&node_id("focus"))
            .unwrap()
            .command_execution
            .as_ref()
            .unwrap()
            .request_id,
        "focus-request"
    );
}

#[test]
fn command_execution_attachment_must_match_the_node_binding() {
    let mut supervisor = TaskSupervisor::new(graph(vec![node("ordinary")], vec![]), 1_000)
        .expect("valid supervisor graph");
    supervisor.start_node(&node_id("ordinary"), 1_001).unwrap();
    let result = supervisor.attach_command_execution(
        &node_id("ordinary"),
        TaskCommandExecution {
            request_id: "focus-request".into(),
            command: "desktop.app.focus".into(),
            app_id: "app:code.exe".into(),
            attempt: 1,
            graph_revision: GraphRevision::initial(),
            status: TaskCommandExecutionStatus::Dispatching,
            approval_id: None,
        },
    );
    assert!(matches!(
        result,
        Err(TaskSupervisorError::InvalidCommandExecution(_))
    ));
}

#[test]
fn persisted_command_execution_must_match_binding_and_revision() {
    let mut source = TaskSupervisor::new(graph(vec![focus_node("focus")], vec![]), 1_000)
        .expect("valid supervisor graph");
    source.start_node(&node_id("focus"), 1_001).unwrap();
    source
        .attach_command_execution(
            &node_id("focus"),
            TaskCommandExecution {
                request_id: "focus-request".into(),
                command: "desktop.app.focus".into(),
                app_id: "app:code.exe".into(),
                attempt: 1,
                graph_revision: GraphRevision::initial(),
                status: TaskCommandExecutionStatus::Dispatching,
                approval_id: None,
            },
        )
        .unwrap();
    let mut checkpoint = source.checkpoint(1_002);
    checkpoint.node_states[0]
        .command_execution
        .as_mut()
        .unwrap()
        .app_id = "app:other.exe".into();

    let mut target = TaskSupervisor::new(graph(vec![focus_node("focus")], vec![]), 1_000)
        .expect("valid supervisor graph");
    let result = target.restore_exact(&checkpoint);
    assert!(matches!(
        result,
        Err(TaskSupervisorError::InvalidCheckpoint(_))
    ));
}

#[test]
fn focus_command_cannot_succeed_without_verified_execution() {
    let mut supervisor = TaskSupervisor::new(graph(vec![focus_node("focus")], vec![]), 1_000)
        .expect("valid supervisor graph");
    supervisor.start_node(&node_id("focus"), 1_001).unwrap();

    let result = supervisor.succeed_node(&node_id("focus"), json!("unverified"), 1_002);

    assert!(matches!(
        result,
        Err(TaskSupervisorError::CommandVerificationRequired(_))
    ));
    assert_eq!(
        supervisor.node_state(&node_id("focus")).unwrap().status,
        TaskNodeStatus::Running
    );
}

#[test]
fn terminal_command_correlation_follows_graph_revision_changes() {
    let mut supervisor = TaskSupervisor::new(graph(vec![focus_node("focus")], vec![]), 1_000)
        .expect("valid supervisor graph");
    supervisor.start_node(&node_id("focus"), 1_001).unwrap();
    supervisor
        .attach_command_execution(
            &node_id("focus"),
            TaskCommandExecution {
                request_id: "focus-request".into(),
                command: "desktop.app.focus".into(),
                app_id: "app:code.exe".into(),
                attempt: 1,
                graph_revision: GraphRevision::initial(),
                status: TaskCommandExecutionStatus::Dispatching,
                approval_id: None,
            },
        )
        .unwrap();
    supervisor
        .update_command_execution(
            &node_id("focus"),
            "focus-request",
            "app:code.exe",
            TaskCommandExecutionStatus::WaitingApproval,
            Some("approval-1".into()),
            1_002,
        )
        .unwrap();
    supervisor
        .mark_command_verified(&node_id("focus"), "focus-request", "app:code.exe", 1_003)
        .unwrap();
    supervisor
        .succeed_node(&node_id("focus"), json!("verified"), 1_004)
        .unwrap();
    let checkpoint = supervisor.checkpoint(1_005);

    supervisor.add_node(node("other"), 1_006).unwrap();
    assert_eq!(
        supervisor
            .node_state(&node_id("focus"))
            .unwrap()
            .command_execution
            .as_ref()
            .unwrap()
            .graph_revision,
        supervisor.graph_revision()
    );

    let restored_revision = supervisor.restore(&checkpoint).unwrap();
    assert_eq!(
        supervisor
            .node_state(&node_id("focus"))
            .unwrap()
            .command_execution
            .as_ref()
            .unwrap()
            .graph_revision,
        restored_revision
    );
}
