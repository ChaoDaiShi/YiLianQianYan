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
    if crate::modules::task::execution::external_result_is_uncertain(failed_exec.failure_code()) {
        return proof_error_response(
            ProofError::new(
                "external_mutation_uncertain",
                "上次调用的外部结果尚未核实，不能再次执行该节点",
            )
            .with_node(failed_exec.node_id.as_str()),
        );
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
    if let Err(error) = server.proofloop_budget.check(&contract, now_ms) {
        return proof_error_response(error);
    }

    let registry = server.build_capability_registry().await;
    let descriptors = registry.list();
    let cap_snapshot =
        build_capability_snapshot(&descriptors, Some(&contract.allowed_capability_ids), now_ms);

    let failed_criterion = report
        .results
        .iter()
        .find(|r| r.status == crate::modules::task::proofloop::contracts::CriterionStatus::Fail);

    let rule_proposal = classify_failure(
        &failed_exec,
        failed_criterion,
        &cap_snapshot,
        &contract,
        rounds_used,
        report.graph_revision,
        report.report_id.clone(),
    );
    if let Err(error) = &rule_proposal {
        if matches!(
            error.code.as_str(),
            "repair_aborted" | "external_mutation_uncertain" | "proofloop_budget_exhausted"
        ) {
            return proof_error_response(error.clone());
        }
    }
    let mut proposal;
    let ai_repair_enabled = server.config.read().ai_repair_enabled;
    if ai_repair_enabled {
        use crate::modules::task::proofloop::ai_repair::{
            propose_ai_repair_with_usage, FailureContext, ObservedValue,
        };
        let graph = match server.task_world.get_graph(&graph_id) {
            Some(v) => v,
            None => {
                return proof_error_response(ProofError::new("graph_not_found", "任务图不存在"))
            }
        };
        let draft = match server
            .db
            .confirmed_proof_draft_for_goal(&contract.goal_id, contract.version)
        {
            Ok(Some(v)) => v,
            _ => {
                return proof_error_response(ProofError::new(
                    "draft_not_found",
                    "已确认目标草稿不存在",
                ))
            }
        };
        let direct_sources: std::collections::HashSet<_> = graph
            .edges
            .iter()
            .filter(|edge| edge.to == failed_exec.node_id)
            .map(|edge| edge.from.to_string())
            .collect();
        let mut observed = Vec::new();
        for evidence in &source_snapshot.evidence {
            if !direct_sources.contains(&evidence.origin.node_id) {
                continue;
            }
            let id = match crate::modules::task::NodeExecutionId::new(&evidence.origin.execution_id)
            {
                Ok(v) => v,
                Err(_) => continue,
            };
            let execution = match server.db.load_node_execution(&id) {
                Ok(Some(v)) if v.status == crate::modules::task::NodeExecutionStatus::Succeeded => {
                    v
                }
                _ => continue,
            };
            if let Some(output) = execution.output {
                collect_observed_values(&output, &mut Vec::new(), evidence, &mut observed);
            }
        }
        // Constants frozen in the current graph are allowed observed inputs; they are not invented replacement values.
        if let Some(node) = graph
            .nodes
            .iter()
            .find(|node| node.id == failed_exec.node_id)
        {
            if let Some(values) = node.input.get("capability_input") {
                collect_frozen_inputs(
                    values,
                    &mut Vec::new(),
                    &draft.draft_id,
                    report.graph_revision,
                    failed_exec.id.as_str(),
                    node.id.as_str(),
                    &mut observed,
                );
            }
        }
        let context = FailureContext {
            contract: contract.clone(),
            snapshot: cap_snapshot.clone(),
            graph_revision: report.graph_revision,
            report_id: report.report_id.clone(),
            failed_execution_ids: vec![failed_exec.id.to_string()],
            failed_node_id: failed_exec.node_id.to_string(),
            failure_code: failed_exec.failure_code.clone().unwrap_or_default(),
            error_message: failed_exec.error.clone().unwrap_or_default(),
            observed_values: observed,
            current_round: rounds_used,
        };
        if let Err(error) =
            crate::modules::task::proofloop::ai_repair::validate_failure_context(&context)
        {
            return proof_error_response(error);
        }
        let activity = match server.proofloop_budget.begin_model_activity(&draft) {
            Ok(v) => v,
            Err(e) => return proof_error_response(e),
        };
        let combined = match server
            .db
            .combined_goal_model_usage(&contract, &draft.planning_usage)
        {
            Ok(v) => v,
            Err(e) => return proof_error_response(e),
        };
        let invocation = match server.db.reserve_goal_model_call(
            &contract,
            &draft.planning_usage,
            "ai-repair",
            now_ms,
        ) {
            Ok(v) => v,
            Err(e) => return proof_error_response(e),
        };
        let mut config = server.effective_model_config();
        let remaining = contract.budget.max_total_tokens.saturating_sub(
            combined
                .total_tokens
                .unwrap_or(contract.budget.max_total_tokens),
        );
        config.max_tokens = config
            .max_tokens
            .min(remaining.min(u64::from(u32::MAX)) as u32);
        let client = LlmClient::new(&config, Arc::clone(&server.secret_resolver));
        let result =
            propose_ai_repair_with_usage(&context, &client, &CancellationToken::new()).await;
        let usage = match &result {
            Ok((_, usage)) => Some(usage),
            Err(error) => error.planning_usage.as_ref(),
        };
        if let Err(error) = server.db.finish_goal_model_call(&invocation, usage) {
            return proof_error_response(error);
        }
        if let Err(error) = activity.finish() {
            return proof_error_response(error);
        }
        proposal = match result {
            Ok((proposal, _)) => proposal,
            Err(error) => return proof_error_response(error),
        };
        if let Err(error) = validate_repair(
            &proposal,
            &graph,
            report.graph_revision,
            &cap_snapshot,
            &contract,
        ) {
            return proof_error_response(error);
        }
    } else {
        proposal = match rule_proposal {
            Ok(value) => value,
            Err(error) => return proof_error_response(error),
        };
    }

    if let Err(err) = server
        .proofloop_budget
        .save_repair(&mut proposal, &contract, now_ms)
    {
        return proof_error_response(err);
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

fn collect_observed_values(
    value: &serde_json::Value,
    path: &mut Vec<String>,
    evidence: &crate::modules::task::proofloop::EvidenceRef,
    out: &mut Vec<crate::modules::task::proofloop::ai_repair::ObservedValue>,
) {
    out.push(crate::modules::task::proofloop::ai_repair::ObservedValue {
        evidence_id: evidence.evidence_id.clone(),
        execution_id: evidence.origin.execution_id.clone(),
        node_id: Some(evidence.origin.node_id.clone()),
        path: path.clone(),
        value: value.clone(),
    });
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                path.push(key.clone());
                collect_observed_values(child, path, evidence, out);
                path.pop();
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                path.push(index.to_string());
                collect_observed_values(child, path, evidence, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn collect_frozen_inputs(
    value: &serde_json::Value,
    path: &mut Vec<String>,
    draft_id: &str,
    revision: u64,
    execution_id: &str,
    node_id: &str,
    out: &mut Vec<crate::modules::task::proofloop::ai_repair::ObservedValue>,
) {
    out.push(crate::modules::task::proofloop::ai_repair::ObservedValue {
        evidence_id: format!("confirmed-draft:{draft_id}:graph:{revision}"),
        execution_id: execution_id.into(),
        node_id: Some(node_id.into()),
        path: path.clone(),
        value: value.clone(),
    });
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                path.push(key.clone());
                collect_frozen_inputs(child, path, draft_id, revision, execution_id, node_id, out);
                path.pop();
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                path.push(index.to_string());
                collect_frozen_inputs(child, path, draft_id, revision, execution_id, node_id, out);
                path.pop();
            }
        }
        _ => {}
    }
}
