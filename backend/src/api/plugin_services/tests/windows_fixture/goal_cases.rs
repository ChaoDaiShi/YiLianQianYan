use super::*;
use crate::{
    agent::{
        delegation::{ExecutionScope, EXECUTION_SCOPE},
        subagent_runtime::{ChildModel, CURRENT_CHILD_MODEL},
    },
    modules::task::{
        application::execution_service::{dispatch_execution, executor_resolver},
        proofloop::{
            collect_capability_snapshot, compile_candidate, CandidateNode, CandidatePlan,
            DraftService, DraftStatus, GoalContract, PlanningUsage, ProofDraft,
        },
        resource_budget::{BudgetReservation, SharedExecutionBudget},
        TaskGraphId, TaskNodeId,
    },
    safety::{
        grant::{GrantEffect, GrantResource, GrantSource, SecurityGrant},
        subject::{DelegatedSubject, ToolInvocation},
        PermissionId,
    },
};

pub(super) async fn child_uses_its_own_model(
    server: &Arc<AppServer>,
    tool: &str,
    managed_id: &str,
    parent_turn: ChatTurnModel,
) {
    let child = SecuritySubject::subagent("model-fixture-child");
    let parent = ToolInvocation {
        subject: SecuritySubject::local_user(),
        agent_id: None,
        conversation_id: parent_turn.conversation_id.clone(),
        tool_call_id: "fixture-delegate".into(),
        tool_name: "subagent_model_fixture".into(),
    };
    let handoff_id = "fixture-handoff".to_owned();
    let delegated = DelegatedSubject::new(
        child.clone(),
        "model-fixture-child",
        handoff_id.clone(),
        parent,
        PathBuf::from("."),
    );
    server
        .db
        .create_grant(&SecurityGrant {
            id: "fixture-child-grant".into(),
            subject_id: child.subject_id.clone(),
            effect: GrantEffect::Allow,
            permission: PermissionId::McpInvoke,
            resource: GrantResource::Mcp {
                server_id: managed_id.into(),
                tool_name: Some("identify_instance".into()),
            },
            source: GrantSource::User,
            created_at: 1,
            expires_at: None,
        })
        .unwrap();
    let registry = server.build_agent_tool_registry().await;
    let gateway =
        crate::safety::production_gateway::build(server, &server.workspace_root, registry, None)
            .with_delegation(delegated);
    let execution = SecurityExecutionRequest {
        conversation_id: "synthetic-child-conversation".into(),
        tool_call_id: "child-tool-call".into(),
        tool_name: tool.into(),
        arguments: json!({}),
        subject: child,
    };
    let mut child_config = server.config.read().model.clone();
    child_config.name = "child-fixture-model".into();
    let budget = SharedExecutionBudget::new(2, 1_000);
    let lease = budget
        .reserve(BudgetReservation {
            handoff_id: handoff_id.clone(),
            calls: 1,
            tokens: 700,
        })
        .unwrap();
    let scope = ExecutionScope {
        owner: Some(ExecutionOwner::Conversation {
            conversation_id: parent_turn.conversation_id.clone(),
        }),
        cancel: CancellationToken::new(),
        budget: Some(budget.clone()),
        handoff_id: Some(handoff_id.clone()),
        goal_id: None,
        goal_version: None,
        evidence_ids: vec![],
        deadline: None,
    };
    let outcome = CURRENT_CHAT_TURN_MODEL
        .scope(
            Some(parent_turn),
            CURRENT_CHILD_MODEL.scope(
                ChildModel {
                    handoff_id: handoff_id.clone(),
                    name: "model-fixture-child".into(),
                    config: child_config,
                    generation: Default::default(),
                },
                EXECUTION_SCOPE.scope(
                    scope,
                    gateway.execute(&execution, crate::tools::RiskLevel::High),
                ),
            ),
        )
        .await
        .unwrap();
    let SecurityExecutionOutcome::Executed { tool_result, .. } = outcome else {
        panic!("child model tool did not execute: {outcome:?}");
    };
    let data = tool_result.structured.unwrap();
    assert_eq!(data["proxy_status"], 200, "{data}");
    let source = &data["model_result"]["source"];
    assert_eq!(source["kind"], "subagent_config");
    assert_eq!(source["subagent_name"], "model-fixture-child");
    assert_eq!(source["reported_model"], "child-fixture-model-v1");
    assert_eq!(budget.handoff_usage(&handoff_id).unwrap(), (1, Some(10)));
    drop(lease);
}

pub(super) async fn goal_node_uses_the_persisted_proofloop_budget(
    server: &Arc<AppServer>,
    tool: &str,
    cwd: &std::path::Path,
) {
    let stamp = chrono::Utc::now().timestamp_millis();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/proofloop/declared-two-node.json"
    ))
    .unwrap();
    let mut contract: GoalContract = serde_json::from_value(fixture["contract"].clone()).unwrap();
    contract.budget.max_model_calls = 2;
    let workspace_id = crate::workspace::WorkspaceId::new(&contract.workspace_id).unwrap();
    server
        .db
        .create_workspace(
            &crate::workspace::Workspace::new(
                workspace_id,
                "Plugin budget fixture".into(),
                "".into(),
                None,
                stamp,
            )
            .unwrap(),
        )
        .unwrap();
    let capability_id = format!("mcp.{}", tool.strip_prefix("mcp_").unwrap());
    contract.allowed_capability_ids = vec![capability_id.clone()];
    let registry = server.build_capability_registry().await;
    let snapshot =
        collect_capability_snapshot(&registry, Some(&contract.allowed_capability_ids), stamp);
    assert_eq!(snapshot.capabilities.len(), 1);
    let mut candidate: CandidatePlan =
        serde_json::from_value(fixture["candidate"].clone()).unwrap();
    candidate.edges.clear();
    for delivery in &mut candidate.deliverable_nodes {
        delivery.node_id = "charge".into();
    }
    candidate.nodes = [("charge", false), ("exhausted", false), ("pause", true)]
        .into_iter()
        .map(|(id, wait)| CandidateNode {
            id: id.into(),
            title: id.into(),
            instruction: "Call the managed service fixture".into(),
            capability_id: capability_id.clone(),
            literal_input: json!({"wait":wait}).as_object().unwrap().clone(),
            bindings: vec![],
        })
        .collect();
    let graph = TaskGraphId::generate();
    let draft = ProofDraft {
        source: None,
        planning_review: None,
        draft_id: "managed-plugin-goal".into(),
        revision: 1,
        graph_id: graph.to_string(),
        compilation: compile_candidate(&contract, &candidate, &snapshot, &graph, &[]),
        contract,
        snapshot: snapshot.clone(),
        candidate: Some(candidate),
        binding_confirmations: vec![],
        status: DraftStatus::ReadyForConfirmation,
        confirmed_graph_revision: None,
        planning_usage: PlanningUsage {
            model_calls: 1,
            total_tokens: Some(100),
            started_at_ms: stamp,
            elapsed_ms: Some(1),
        },
    };
    server.db.insert_proof_draft(&draft, stamp).unwrap();
    let confirmed = DraftService::new(server.db.clone_connection())
        .confirm_draft(&server.task_world, &draft.draft_id, 1, &snapshot, stamp)
        .unwrap();

    for (name, expected_status) in [("charge", 200), ("exhausted", 409)] {
        let node = TaskNodeId::new(name).unwrap();
        let resolver = executor_resolver(server, &graph, &node).await.unwrap();
        let execution = server
            .task_world
            .start_execution_with_resolver(
                &graph,
                &node,
                confirmed.graph_revision,
                resolver,
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap();
        dispatch_execution(server.clone(), graph.clone(), execution.id.clone())
            .await
            .unwrap();
        let result: Value =
            serde_json::from_slice(&std::fs::read(cwd.join("last-model-result")).unwrap()).unwrap();
        assert_eq!(result["proxy_status"], expected_status, "{name}: {result}");
        if expected_status == 200 {
            assert_eq!(
                result["model_result"]["execution_owner"]["execution_id"],
                execution.id.as_str()
            );
            assert_eq!(
                result["model_result"]["source"]["reported_model"],
                "fixture-model-v1"
            );
        } else {
            assert_eq!(result["model_result"]["code"], "proofloop_budget_exhausted");
        }
    }

    let node = TaskNodeId::new("pause").unwrap();
    let resolver = executor_resolver(server, &graph, &node).await.unwrap();
    for marker in ["mcp-continue", "mcp-inflight", "last-model-result"] {
        std::fs::remove_file(cwd.join(marker)).unwrap();
    }
    let attempt = server
        .task_world
        .start_execution_with_resolver(
            &graph,
            &node,
            confirmed.graph_revision,
            resolver.clone(),
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    let pending = tokio::spawn(dispatch_execution(
        server.clone(),
        graph.clone(),
        attempt.id,
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !cwd.join("mcp-inflight").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let control = server
        .task_world
        .pause_task(&graph, chrono::Utc::now().timestamp_millis())
        .unwrap();
    assert_eq!(
        control.state,
        crate::modules::task::TaskExecutionControlState::Paused
    );
    let stopped = pending.await.unwrap().unwrap();
    assert!(!stopped.status.is_active());
    assert_ne!(
        stopped.status,
        crate::modules::task::NodeExecutionStatus::Succeeded
    );
    std::fs::write(cwd.join("mcp-continue"), "late plugin callback").unwrap();
    let late: Value = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = std::fs::read(cwd.join("last-model-result")) {
                if let Ok(receipt) = serde_json::from_slice(&bytes) {
                    return receipt;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(late["proxy_status"], 401, "{late}");
    let failed = server
        .task_world
        .start_execution_with_resolver(
            &graph,
            &node,
            confirmed.graph_revision,
            resolver,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap_err();
    assert!(matches!(
        failed,
        crate::modules::task::TaskWorldRuntimeError::ExecutionPaused(_)
    ));
    let usage = server
        .db
        .combined_goal_model_usage(&draft.contract, &draft.planning_usage)
        .unwrap();
    assert_eq!(usage.model_calls, 2);
    assert_eq!(usage.total_tokens, Some(110));
}
