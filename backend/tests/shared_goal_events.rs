#[path = "support/shared_goal_activity.rs"]
mod support;
use serde_json::{json, Value};
use support::*;
use yilian_backend::{
    agent::user_input::UserInputRequest,
    db::ConversationRunStatus,
    modules::task::{
        artifact::{ArtifactService, ArtifactSource},
        NodeContext, NodeExecution, NodeExecutionId, NodeExecutionStatus, TaskNodeId,
        ValidationResult,
    },
    shared::event::YiEvent,
};

async fn changed(receiver: &mut tokio::sync::broadcast::Receiver<YiEvent>, change: &str) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let event = receiver.recv().await.unwrap();
            if event.event_type == "task.goal.changed" && event.payload["change"] == change {
                return event.payload;
            }
        }
    })
    .await
    .expect("committed goal change was not broadcast")
}

#[tokio::test]
async fn binding_and_chat_lifecycle_publish_committed_goal_identity_only() {
    let (server, contract, graph) = setup();
    let mut events = server.event_hub.subscribe();
    let conversation = bound_conversation(&server, &graph).await;
    let event = changed(&mut events, "conversation_bound").await;
    assert_eq!(event["graph_id"], graph.as_str());
    assert_eq!(event["workspace_id"], contract.workspace_id);
    assert_eq!(event["conversation_id"], conversation);
    assert!(server
        .db
        .load_conversation_goal_binding(&conversation)
        .unwrap()
        .is_some());
    assert_eq!(
        call(
            &server,
            "PUT",
            &format!("/api/conversations/{conversation}/goal"),
            json!({"graph_id":graph})
        )
        .await
        .0,
        200
    );
    assert!(
        events.try_recv().is_err(),
        "idempotent binding emitted a new transition"
    );
    let rejected = server
        .db
        .create_conversation("Rejected binding")
        .unwrap()
        .id;
    server.db.conn().execute_batch("CREATE TRIGGER reject_goal_binding BEFORE INSERT ON conversation_goal_bindings BEGIN SELECT RAISE(ABORT, 'test binding transaction failure'); END;").unwrap();
    assert_eq!(
        call(
            &server,
            "PUT",
            &format!("/api/conversations/{rejected}/goal"),
            json!({"graph_id":graph})
        )
        .await
        .0,
        500
    );
    assert!(server
        .db
        .load_conversation_goal_binding(&rejected)
        .unwrap()
        .is_none());
    assert!(
        events.try_recv().is_err(),
        "rolled-back binding emitted a change"
    );
    start_turn(&server, &conversation);
    assert_eq!(
        changed(&mut events, "conversation_status").await["status"],
        "running"
    );
    server
        .db
        .set_conversation_run_status(
            &conversation,
            ConversationRunStatus::Failed,
            Some("private diagnostic details"),
        )
        .unwrap();
    let event = changed(&mut events, "conversation_status").await;
    assert_eq!(event["status"], "failed");
    assert!(!event.to_string().contains("private diagnostic details"));
    assert_eq!(
        server
            .db
            .get_conversation(&conversation)
            .unwrap()
            .run_status,
        ConversationRunStatus::Failed
    );
    server.db.delete_conversation(&conversation).unwrap();
    assert_eq!(
        changed(&mut events, "conversation_unbound").await["graph_id"],
        graph.as_str()
    );
    assert!(server
        .db
        .find_conversation(&conversation)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn input_wait_answer_and_cancel_publish_current_status_without_answers() {
    let (server, _, graph) = setup();
    let conversation = bound_conversation(&server, &graph).await;
    start_turn(&server, &conversation);
    let mut events = server.event_hub.subscribe();
    let input = server
        .db
        .create_conversation_input(
            &conversation,
            "ask",
            &UserInputRequest {
                title: "Choose a result".into(),
                kind: "text".into(),
                options: vec![],
                allow_text: true,
            },
        )
        .unwrap();
    assert_eq!(
        changed(&mut events, "conversation_status").await["status"],
        "waiting_input"
    );
    server
        .db
        .answer_conversation_input(
            &input.question_id,
            &yilian_backend::db::conversation_inputs::AnswerInputRequest {
                revision: 1,
                option_id: None,
                text: Some("private answer".into()),
            },
        )
        .unwrap();
    let event = changed(&mut events, "conversation_status").await;
    assert_eq!(event["status"], "running");
    assert!(!event.to_string().contains("private answer"));
    server
        .db
        .create_conversation_input(
            &conversation,
            "ask-again",
            &UserInputRequest {
                title: "Continue?".into(),
                kind: "text".into(),
                options: vec![],
                allow_text: true,
            },
        )
        .unwrap();
    changed(&mut events, "conversation_status").await;
    server.db.cancel_conversation_inputs(&conversation).unwrap();
    assert_eq!(
        changed(&mut events, "conversation_status").await["status"],
        "cancelled"
    );
}

#[tokio::test]
async fn artifact_and_evaluation_review_publish_after_real_bytes_and_reports_commit() {
    let (server, contract, graph) = setup();
    let mut execution = NodeExecution::new(
        NodeExecutionId::generate(),
        graph.clone(),
        TaskNodeId::new("compare").unwrap(),
        1,
        None,
        NodeContext::new(
            "Goal",
            "Report",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .unwrap(),
        now(),
    )
    .unwrap();
    execution.status = NodeExecutionStatus::Succeeded;
    execution.output = Some(json!({"structured":{"summary":"Actual local fixture report"}}));
    execution.validation = Some(ValidationResult::accepted());
    server.db.save_node_execution(&execution).unwrap();
    let mut events = server.event_hub.subscribe();
    let service = ArtifactService::new(server.db.clone());
    let (artifact, _) = service
        .materialize(
            &ArtifactSource::Node {
                graph_id: graph.to_string(),
                node_id: "compare".into(),
                execution_id: execution.id.to_string(),
                workspace_id: contract.workspace_id.clone(),
            },
            &contract.deliverables[0].title,
            &server.workspace_root,
            now(),
        )
        .unwrap();
    let event = changed(&mut events, "artifact_created").await;
    assert_eq!(event["artifact_id"], artifact.id.as_str());
    assert!(!service
        .read_bytes(&artifact.id, &server.workspace_root)
        .unwrap()
        .1
        .is_empty());
    let (status, report) = call(
        &server,
        "POST",
        &format!(
            "/api/task-world/proofloop/goals/{}/evaluations",
            contract.goal_id
        ),
        json!({"goal_version":contract.version,"graph_revision":1,"execution_ids":[execution.id]}),
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(
        changed(&mut events, "acceptance_updated").await["report_id"],
        report["report_id"]
    );
    let (status, reviewed) = call(
        &server,
        "POST",
        &format!(
            "/api/task-world/proofloop/evaluations/{}/human-review",
            report["report_id"].as_str().unwrap()
        ),
        json!({"criterion_id":"human","decision":"pass","reason":"Actual local review"}),
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
    assert_eq!(
        changed(&mut events, "acceptance_updated").await["report_id"],
        reviewed["report_id"]
    );
    assert_eq!(reviewed["status"], "pass");
}
