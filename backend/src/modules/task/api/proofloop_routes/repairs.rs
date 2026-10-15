//! Controlled repair HTTP handlers.
use super::*;

mod apply;

#[derive(serde::Serialize)]
struct RepairProposalHttpResponse {
    #[serde(flatten)]
    proposal: crate::modules::task::proofloop::contracts::RepairProposal,
    #[serde(skip_serializing_if = "Option::is_none")]
    ai_repair_usage: Option<crate::modules::task::proofloop::contracts::PlanningUsage>,
}

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
    let mut ai_repair_usage = None;
    let ai_repair_enabled = server.config.read().ai_repair_enabled;
    if ai_repair_enabled {
        use crate::modules::task::proofloop::ai_repair::FailureContext;
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
        let activity_deadline = tokio::time::Instant::now() + activity.remaining();
        let combined = match server
            .db
            .combined_goal_model_usage(&contract, &draft.planning_usage)
        {
            Ok(v) => v,
            Err(e) => return proof_error_response(e),
        };
        let Some(used_tokens) = combined.total_tokens else {
            return proof_error_response(ProofError::new(
                "proofloop_usage_unavailable",
                "目标的历史模型用量未知，不能调用修复模型",
            ));
        };
        let remaining_tokens = contract.budget.max_total_tokens.saturating_sub(used_tokens);
        if remaining_tokens == 0 {
            return proof_error_response(ProofError::new(
                "proofloop_budget_exhausted",
                "目标的剩余 token 预算已耗尽",
            ));
        }
        let mut config = server.effective_model_config();
        config.invoke_timeout_ms = config
            .invoke_timeout_ms
            .min(contract.budget.max_wall_time_ms);
        let usage_recorder = server
            .db
            .get_active_llm_model()
            .ok()
            .flatten()
            .filter(|model| model.model == config.name && model.provider == config.provider)
            .map(|model| {
                std::sync::Arc::new(crate::integrations::llm::usage::DatabaseUsageRecorder::new(
                    server.db.clone_connection(),
                    model.id,
                    model.provider,
                    model.model,
                    "proofloop-ai-repair",
                ))
                    as std::sync::Arc<dyn crate::integrations::llm::client::UsageRecorder>
            });
        let client = LlmClient::new_with_usage_recorder(
            &config,
            Arc::clone(&server.secret_resolver),
            usage_recorder,
        );
        let messages =
            match crate::modules::task::proofloop::ai_repair::build_ai_repair_messages(&context) {
                Ok(value) => value,
                Err(error) => return proof_error_response(error),
            };
        let max_output_tokens =
            match client.output_tokens_for_total_budget(&messages, &[], remaining_tokens) {
                Some(value) => value,
                None => {
                    return proof_error_response(ProofError::new(
                        "proofloop_budget_exhausted",
                        "修复输入已占满目标剩余 token 预算，未调用模型",
                    ))
                }
            };
        if activity_deadline <= tokio::time::Instant::now() {
            return proof_error_response(ProofError::new(
                "proofloop_budget_exhausted",
                "修复活动时间预算已耗尽，未调用模型",
            ));
        }
        let invocation = match server.db.reserve_goal_model_call(
            &contract,
            &draft.planning_usage,
            "ai-repair",
            now_ms,
        ) {
            Ok(v) => v,
            Err(e) => return proof_error_response(e),
        };
        let result =
            crate::modules::task::proofloop::ai_repair::propose_ai_repair_with_usage_and_limits(
                &context,
                &client,
                &CancellationToken::new(),
                max_output_tokens,
                remaining_tokens,
                activity_deadline,
            )
            .await;
        let measured_usage = match &result {
            Ok((_, usage)) => Some(usage.clone()),
            Err(error) => error.planning_usage.clone(),
        };
        if let Err(error) = server
            .db
            .finish_goal_model_call(&invocation, measured_usage.as_ref())
        {
            let error = match measured_usage.clone() {
                Some(usage) => error.with_planning_usage(usage),
                None => error,
            };
            return proof_error_response(error);
        }
        if let Err(error) = activity.finish() {
            let error = match measured_usage.clone() {
                Some(usage) => error.with_planning_usage(usage),
                None => error,
            };
            return proof_error_response(error);
        }
        proposal = match result {
            Ok((proposal, usage)) => {
                ai_repair_usage = Some(usage);
                proposal
            }
            Err(error) => return proof_error_response(error),
        };
        if let Err(error) = validate_repair(
            &proposal,
            &graph,
            report.graph_revision,
            &cap_snapshot,
            &contract,
        ) {
            let error = match ai_repair_usage.clone() {
                Some(usage) => error.with_planning_usage(usage),
                None => error,
            };
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
        let err = match ai_repair_usage.clone() {
            Some(usage) => err.with_planning_usage(usage),
            None => err,
        };
        return proof_error_response(err);
    }

    (
        StatusCode::OK,
        Json(RepairProposalHttpResponse {
            proposal,
            ai_repair_usage,
        }),
    )
        .into_response()
}

pub async fn apply_repair_handler(
    State(server): State<Arc<AppServer>>,
    Path(repair_id): Path<String>,
    Json(request): Json<ApplyRepairRequest>,
) -> Response {
    apply::apply_repair_handler_inner(State(server), Path(repair_id), Json(request)).await
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

#[cfg(test)]
#[path = "repairs/tests.rs"]
mod tests;
