//! Controlled repair HTTP handlers.
use super::*;

pub async fn propose_repair_handler(
    State(server): State<Arc<AppServer>>,
    Path(goal_id): Path<String>,
    Json(request): Json<ProposeRepairRequest>,
) -> Response {
    let now_ms = now();
    let (report, source_snapshot) = match server.db.get_acceptance_report(&request.report_id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return proof_error_response(ProofError::new(
                "report_not_found",
                format!("未找到验收报告 '{}'", request.report_id),
            ));
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };
    if report.goal_id != goal_id {
        return proof_error_response(ProofError::new("goal_mismatch", "验收报告不属于请求的目标"));
    }

    let contract = match server
        .db
        .load_goal_version(&report.goal_id, report.goal_version)
    {
        Ok(Some(c)) => c,
        _ => return proof_error_response(ProofError::new("goal_not_found", "未找到目标契约")),
    };

    let failed_exec_id = match request.failure_execution_ids.first() {
        Some(id) => id,
        None => {
            return proof_error_response(ProofError::new(
                "no_failed_executions",
                "缺少失败执行记录",
            ));
        }
    };

    let exec_id = match crate::modules::task::execution::NodeExecutionId::new(failed_exec_id) {
        Ok(id) => id,
        Err(err) => {
            return proof_error_response(ProofError::new("invalid_execution_id", err.to_string()))
        }
    };

    let failed_exec = match server.db.load_node_execution(&exec_id) {
        Ok(Some(e)) => e,
        _ => return proof_error_response(ProofError::new("execution_not_found", "未找到执行记录")),
    };
    if failed_exec.graph_id.as_str() != report.graph_id
        || !source_snapshot
            .executions
            .iter()
            .any(|execution| execution.execution_id == failed_exec.id.as_str())
        || failed_exec.status != crate::modules::task::NodeExecutionStatus::Failed
    {
        return proof_error_response(ProofError::new(
            "invalid_failure_evidence",
            "失败记录必须属于此报告选定的当前执行",
        ));
    }
    let graph_id = match crate::modules::task::TaskGraphId::new(&report.graph_id) {
        Ok(id) => id,
        Err(error) => {
            return proof_error_response(ProofError::new("invalid_graph_id", error.to_string()))
        }
    };
    if !server
        .task_world
        .get_graph(&graph_id)
        .is_some_and(|graph| graph.revision.value() == report.graph_revision)
    {
        return proof_error_response(ProofError::new(
            "proofloop_stale_revision",
            "任务图已改变，需重新验收后生成修复",
        ));
    }
    let rounds_used = match server
        .db
        .repair_rounds_used(&contract.goal_id, contract.version)
    {
        Ok(rounds) => rounds,
        Err(error) => return proof_error_response(ProofError::new("db_error", error.to_string())),
    };
    if let Err(error) =
        crate::modules::task::proofloop::budget::check_goal_budget(&server.db, &contract, now_ms)
    {
        return proof_error_response(error);
    }

    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let cap_snapshot = build_capability_snapshot(&descriptors, None, now_ms);

    let failed_criterion = report
        .results
        .iter()
        .find(|r| r.status == crate::modules::task::proofloop::contracts::CriterionStatus::Fail);

    let mut proposal = match classify_failure(
        &failed_exec,
        failed_criterion,
        &cap_snapshot,
        &contract,
        rounds_used,
        report.graph_revision,
        report.report_id.clone(),
    ) {
        Ok(p) => p,
        Err(err) => return proof_error_response(err),
    };

    if let Err(err) = server
        .db
        .save_repair_proposal_with_budget(&mut proposal, &contract, now_ms)
    {
        let code = if err.to_string().contains("proofloop_budget_exhausted") {
            "proofloop_budget_exhausted"
        } else {
            "db_error"
        };
        return proof_error_response(ProofError::new(code, err.to_string()));
    }

    (StatusCode::OK, Json(proposal)).into_response()
}

pub async fn apply_repair_handler(
    State(server): State<Arc<AppServer>>,
    Path(repair_id): Path<String>,
    Json(request): Json<ApplyRepairRequest>,
) -> Response {
    let proposal = match server.db.get_repair_proposal(&repair_id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return proof_error_response(ProofError::new("repair_not_found", "未找到修复提案"))
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };

    let contract = match server
        .db
        .load_goal_version(&proposal.goal_id, proposal.goal_version)
    {
        Ok(Some(c)) => c,
        _ => return proof_error_response(ProofError::new("goal_not_found", "未找到目标契约")),
    };

    let (source_report, _) = match server.db.get_acceptance_report(&proposal.source_report_id) {
        Ok(Some(report)) => report,
        _ => {
            return proof_error_response(ProofError::new(
                "report_not_found",
                "找不到修复依据的验收报告",
            ))
        }
    };
    let draft = match server
        .db
        .confirmed_proof_draft_for_goal(&proposal.goal_id, proposal.goal_version)
    {
        Ok(Some(draft)) if draft.graph_id == source_report.graph_id => draft,
        _ => {
            return proof_error_response(ProofError::new(
                "goal_graph_not_found",
                "找不到对应的目标与任务图关联",
            ))
        }
    };
    if let Ok(Some(revision)) = server.db.repair_applied_revision(&repair_id) {
        return (
            StatusCode::OK,
            Json(ApplyRepairResponse {
                graph_revision: revision,
                affected_node_ids: proposal.affected_node_ids,
            }),
        )
            .into_response();
    }
    if let Err(error) =
        crate::modules::task::proofloop::budget::check_goal_budget(&server.db, &contract, now())
    {
        return proof_error_response(error);
    }
    let graph_id = match crate::modules::task::TaskGraphId::new(&draft.graph_id) {
        Ok(id) => id,
        Err(err) => {
            return proof_error_response(ProofError::new("invalid_graph_id", err.to_string()))
        }
    };

    let graph = match server.task_world.get_graph(&graph_id) {
        Some(g) => g,
        None => return proof_error_response(ProofError::new("graph_not_found", "未找到任务图")),
    };

    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let snapshot = build_capability_snapshot(&descriptors, None, now());

    let patch = match validate_repair(
        &proposal,
        &graph,
        request.expected_graph_revision,
        &snapshot,
        &contract,
    ) {
        Ok(p) => p,
        Err(err) => return proof_error_response(err),
    };

    let candidate = match candidate_from_graph(&patch.updated_graph, draft.candidate.as_ref()) {
        Ok(candidate) => candidate,
        Err(error) => return proof_error_response(error),
    };
    let compilation = crate::modules::task::proofloop::compile_candidate(
        &contract,
        &candidate,
        &snapshot,
        &graph_id,
        &draft.binding_confirmations,
    );
    if compilation.status != crate::modules::task::proofloop::CompileStatus::Ready {
        return proof_error_response(ProofError::new(
            "repair_compile_failed",
            format!("修复候选未通过编译：{:?}", compilation.diagnostics),
        ));
    }
    let root = match crate::modules::task::TaskNodeId::new(&patch.rerun_root_node_id) {
        Ok(id) => id,
        Err(error) => {
            return proof_error_response(ProofError::new("invalid_node_id", error.to_string()))
        }
    };
    let resolver =
        match crate::modules::task::application::execution_service::executor_resolver_for_node(
            &server,
            patch.updated_graph.node(&root).unwrap(),
        )
        .await
        {
            Ok(resolver) => resolver,
            Err(error) => {
                return proof_error_response(ProofError::new(
                    "provider_unavailable",
                    error.to_string(),
                ))
            }
        };
    let affected_node_ids = patch.affected_node_ids.clone();
    if let Err(error) =
        crate::modules::task::application::execution_service::ensure_workflow_budget_supported(
            &server,
            patch.updated_graph.node(&root).unwrap(),
        )
    {
        return proof_error_response(ProofError::new(
            "proofloop_usage_unavailable",
            error.to_string(),
        ));
    }
    let (updated_graph, execution) = match server.task_world.apply_proofloop_repair(
        patch,
        request.expected_graph_revision,
        resolver,
        now(),
    ) {
        Ok(result) => result,
        Err(error) => {
            return proof_error_response(ProofError::new(
                "repair_application_failed",
                error.to_string(),
            ))
        }
    };
    let dispatch_server = server.clone();
    let dispatch_graph = graph_id.clone();
    tokio::spawn(async move {
        if let Err(error) =
            crate::modules::task::application::execution_service::dispatch_execution(
                dispatch_server,
                dispatch_graph,
                execution.id,
            )
            .await
        {
            tracing::error!(error = %error, "proofloop repair dispatch failed");
        }
    });

    (
        StatusCode::OK,
        Json(ApplyRepairResponse {
            graph_revision: updated_graph.revision.value(),
            affected_node_ids,
        }),
    )
        .into_response()
}

fn candidate_from_graph(
    graph: &crate::modules::task::TaskGraph,
    original: Option<&crate::modules::task::proofloop::CandidatePlan>,
) -> Result<crate::modules::task::proofloop::CandidatePlan, ProofError> {
    let mut nodes = Vec::new();
    for node in &graph.nodes {
        let reference = node
            .input
            .get("executor_ref")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let capability_id = if let Some(id) = reference.strip_prefix("capability://") {
            id.into()
        } else if let Some(id) = reference.strip_prefix("workflow://") {
            format!("workflow.{id}")
        } else {
            return Err(ProofError::new(
                "repair_compile_failed",
                "节点没有受支持的执行器",
            ));
        };
        nodes.push(crate::modules::task::proofloop::CandidateNode {
            id: node.id.to_string(),
            title: node.title.clone(),
            instruction: node
                .input
                .get("instruction")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .into(),
            capability_id,
            literal_input: node
                .input
                .get("capability_input")
                .and_then(serde_json::Value::as_object)
                .cloned()
                .unwrap_or_default(),
            bindings: crate::modules::task::input_binding::bindings_from_input(&node.input)
                .map_err(|error| ProofError::new(error.code, error.reason))?,
        });
    }
    Ok(crate::modules::task::proofloop::CandidatePlan {
        schema_version: 1,
        nodes,
        edges: graph
            .edges
            .iter()
            .map(|edge| crate::modules::task::proofloop::CandidateEdge {
                from: edge.from.to_string(),
                to: edge.to.to_string(),
            })
            .collect(),
        deliverable_nodes: original
            .map(|plan| plan.deliverable_nodes.clone())
            .unwrap_or_default(),
    })
}
