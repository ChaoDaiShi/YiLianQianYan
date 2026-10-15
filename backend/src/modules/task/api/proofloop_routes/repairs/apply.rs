use super::*;

pub(super) async fn apply_repair_handler_inner(
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

    let (source_report, source_snapshot) =
        match server.db.get_acceptance_report(&proposal.source_report_id) {
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
    if let Err(error) = server.proofloop_budget.check(&contract, now()) {
        return proof_error_response(error);
    }
    let graph_id = match crate::modules::task::TaskGraphId::new(&draft.graph_id) {
        Ok(id) => id,
        Err(err) => {
            return proof_error_response(ProofError::new("invalid_graph_id", err.to_string()))
        }
    };

    if proposal.failure_execution_ids.is_empty() {
        return proof_error_response(ProofError::new(
            "invalid_failure_evidence",
            "修复提案缺少来源执行记录",
        ));
    }
    for failure_execution_id in &proposal.failure_execution_ids {
        let execution_id =
            match crate::modules::task::execution::NodeExecutionId::new(failure_execution_id) {
                Ok(id) => id,
                Err(error) => {
                    return proof_error_response(ProofError::new(
                        "invalid_execution_id",
                        error.to_string(),
                    ))
                }
            };
        let failed_execution = match server.db.load_node_execution(&execution_id) {
            Ok(Some(execution)) => execution,
            _ => {
                return proof_error_response(ProofError::new(
                    "execution_not_found",
                    "未找到修复提案的来源执行记录",
                ))
            }
        };
        if failed_execution.graph_id != graph_id
            || !source_snapshot
                .executions
                .iter()
                .any(|execution| execution.execution_id == failed_execution.id.as_str())
        {
            return proof_error_response(ProofError::new(
                "invalid_failure_evidence",
                "修复提案的来源执行不属于此报告选定的当前执行",
            ));
        }
        if crate::modules::task::execution::external_result_is_uncertain(
            failed_execution.failure_code(),
        ) {
            return proof_error_response(
                ProofError::new(
                    "external_mutation_uncertain",
                    "上次调用的外部结果尚未核实，当前不能再次执行该节点",
                )
                .with_node(failed_execution.node_id.as_str()),
            );
        }
        if failed_execution.status != crate::modules::task::NodeExecutionStatus::Failed {
            return proof_error_response(ProofError::new(
                "invalid_failure_evidence",
                "修复提案的来源执行已不再处于失败状态",
            ));
        }
        let latest = match server
            .db
            .load_node_executions(&failed_execution.graph_id, &failed_execution.node_id)
        {
            Ok(executions) => executions
                .into_iter()
                .max_by_key(|execution| execution.attempt),
            Err(error) => {
                return proof_error_response(ProofError::new("db_error", error.to_string()))
            }
        };
        if latest.map_or(true, |latest| latest.id != failed_execution.id) {
            return proof_error_response(ProofError::new(
                "proofloop_evidence_stale",
                "修复提案的来源执行已被新的 attempt 替代",
            ));
        }
    }

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
        Err(crate::modules::task::TaskWorldRuntimeError::Harness(
            crate::modules::task::TaskHarnessError::ExternalResultUnknown(execution_id),
        )) => {
            return proof_error_response(ProofError::new(
                "external_mutation_uncertain",
                format!(
                    "上次调用的外部结果尚未核实，当前不能再次执行该节点。执行记录：{execution_id}"
                ),
            ))
        }
        Err(crate::modules::task::TaskWorldRuntimeError::Harness(
            crate::modules::task::TaskHarnessError::RepairSourceStale(execution_id),
        )) => {
            return proof_error_response(ProofError::new(
                "proofloop_evidence_stale",
                format!("修复提案的来源执行已不再是最新失败 attempt。执行记录：{execution_id}"),
            ))
        }
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
