//! Controlled repair proposer and patch validator for ProofLoop.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/03-controlled-repair.md

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::contracts::{
    CapabilitySnapshot, CriterionResult, GoalContract, ProofError, RepairOperation, RepairProposal,
};
use crate::modules::task::{
    execution::{NodeExecution, NodeExecutionStatus},
    input_binding::validate_graph_bindings,
    task_graph::{GraphRevision, TaskGraph, TaskNodeId},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidatedRepairPatch {
    pub repair_id: String,
    pub rerun_root_node_id: String,
    pub affected_node_ids: Vec<String>,
    pub updated_graph: TaskGraph,
    pub diff_summary: Vec<String>,
}

/// Classifies a failure and suggests a controlled repair proposal.
pub fn classify_failure(
    failed_execution: &NodeExecution,
    _failed_criterion: Option<&CriterionResult>,
    _snapshot: &CapabilitySnapshot,
    contract: &GoalContract,
    current_round: u32,
    graph_revision: u64,
    source_report_id: String,
) -> Result<RepairProposal, ProofError> {
    let next_round = current_round + 1;
    if next_round > contract.budget.max_repair_rounds {
        return Err(ProofError::new(
            "proofloop_budget_exhausted",
            format!(
                "修复轮次已达预算上限 {} (当前申请第 {} 轮)",
                contract.budget.max_repair_rounds, next_round
            ),
        ));
    }

    let node_id = failed_execution.node_id.as_str().to_string();

    // Check non-repairable conditions
    if failed_execution.status == NodeExecutionStatus::Cancelled {
        return Err(ProofError::new(
            "repair_aborted",
            "执行已被用户取消，受控修复停止",
        ));
    }

    if let Some(err_msg) = &failed_execution.error {
        if err_msg.contains("approval denied") || err_msg.contains("拒绝审批") {
            return Err(ProofError::new(
                "repair_aborted",
                "审批已被用户拒绝，禁止自动绕过或规避审批",
            ));
        }
        if err_msg.contains("timeout") || err_msg.contains("超时") {
            return Err(ProofError::new(
                "external_mutation_uncertain",
                "外部调用超时，写入副作用结果未知，需人工介入核实，禁止盲目重跑",
            ));
        }
    }

    let mut operations = Vec::new();
    let rationale;

    // Detect missing input vs binding vs transient error
    if let Some(code) = failed_execution.failure_code.as_deref() {
        match code {
            "missing_required_argument" | "invalid_argument" => {
                return Err(ProofError::new(
                    "repair_needs_input",
                    "没有可验证的正确参数值，请在节点属性中补充真实值后重新编译；不会生成占位参数",
                ));
            }
            "binding_failed"
            | "input_binding_schema_mismatch"
            | "input_binding_path_missing"
            | "input_binding_type_mismatch" => {
                return Err(ProofError::new(
                    "repair_needs_input",
                    "请根据真实输出选择兼容的绑定路径，再请求局部重跑",
                ));
            }
            "capability_unavailable" => {
                return Err(ProofError::new(
                    "repair_needs_input",
                    "尚无输入输出兼容的替代能力证据，请先选择有效能力；不会任意替换工具",
                ));
            }
            "provider_unavailable" | "input_binding_source_missing" | "execution_interrupted" => {
                rationale = format!("节点 '{}' 执行异常 ({})，提议受控重跑该节点", node_id, code);
                operations.push(RepairOperation::RerunNode {
                    node_id: node_id.clone(),
                });
            }
            "security_denied" | "approval_rejected" => {
                return Err(ProofError::new(
                    "repair_aborted",
                    "授权或审批已拒绝，不能通过修复绕过",
                ))
            }
            _ => {
                return Err(ProofError::new(
                    "repair_needs_input",
                    "尚未证明该失败可安全重跑，请核实结果或修正节点后重试",
                ))
            }
        }
    } else {
        return Err(ProofError::new(
            "repair_needs_input",
            "缺少明确的失败代码，不能生成重跑提案",
        ));
    }

    Ok(RepairProposal {
        schema_version: 1,
        repair_id: uuid::Uuid::new_v4().to_string(),
        goal_id: contract.goal_id.clone(),
        goal_version: contract.version,
        expected_graph_revision: graph_revision,
        source_report_id,
        failure_execution_ids: vec![failed_execution.id.as_str().to_string()],
        round: next_round,
        operations,
        rerun_root_node_id: node_id.clone(),
        affected_node_ids: vec![node_id],
        rationale,
    })
}

/// Validates a RepairProposal against current graph and capability snapshot.
pub fn validate_repair(
    proposal: &RepairProposal,
    current_graph: &TaskGraph,
    current_revision: u64,
    snapshot: &CapabilitySnapshot,
    contract: &GoalContract,
) -> Result<ValidatedRepairPatch, ProofError> {
    if proposal.schema_version != 1 {
        return Err(ProofError::new(
            "unsupported_schema_version",
            format!("不支持的修复提案 schema 版本 {}", proposal.schema_version),
        ));
    }

    if proposal.expected_graph_revision != current_revision
        || current_graph.revision.value() != current_revision
    {
        return Err(ProofError::new(
            "proofloop_stale_revision",
            format!(
                "修复提案预期图版本 {} 与当前图版本 {} 不符，提案已过期",
                proposal.expected_graph_revision, current_revision
            ),
        ));
    }

    if proposal.goal_id != contract.goal_id || proposal.goal_version != contract.version {
        return Err(ProofError::new(
            "goal_mismatch",
            "修复提案与当前目标定义不匹配",
        ));
    }

    if proposal.round > contract.budget.max_repair_rounds {
        return Err(ProofError::new(
            "proofloop_budget_exhausted",
            format!(
                "修复轮次 {} 超过最大预算轮次 {}",
                proposal.round, contract.budget.max_repair_rounds
            ),
        ));
    }

    // 1. Verify root node exists
    let root_node_id = TaskNodeId::new(&proposal.rerun_root_node_id)
        .map_err(|e| ProofError::new("invalid_node_id", e.to_string()))?;

    if !current_graph.nodes.iter().any(|n| n.id == root_node_id) {
        return Err(ProofError::new(
            "root_node_not_found",
            format!(
                "重跑根节点 '{}' 在当前图中不存在",
                proposal.rerun_root_node_id
            ),
        ));
    }

    // 2. Compute reachable downstream affected branch (BFS on edges)
    let affected_node_ids = compute_downstream_branch(current_graph, &proposal.rerun_root_node_id);
    let allowed_mutation_set: HashSet<&str> =
        affected_node_ids.iter().map(|s| s.as_str()).collect();

    // 3. Check every operation modifies ONLY nodes in the affected branch
    for op in &proposal.operations {
        let target_node = match op {
            RepairOperation::ReplaceLiteral { node_id, .. } => node_id,
            RepairOperation::ReplaceBinding { node_id, .. } => node_id,
            RepairOperation::ReplaceCapability { node_id, .. } => node_id,
            RepairOperation::RerunNode { node_id } => node_id,
        };

        if !allowed_mutation_set.contains(target_node.as_str()) {
            return Err(ProofError::new(
                "unrelated_branch_mutation_forbidden",
                format!(
                    "修复操作试图修改无关节点 '{}'。受控修复严格禁止修改失败根节点及其下游依赖以外的成功分支",
                    target_node
                ),
            ));
        }
    }

    // 4. Build updated graph in memory
    let mut updated_nodes = current_graph.nodes.clone();
    let mut diff_summary = Vec::new();

    for op in &proposal.operations {
        match op {
            RepairOperation::ReplaceLiteral {
                node_id,
                target_path,
                value,
            } => {
                let node = updated_nodes
                    .iter_mut()
                    .find(|n| n.id.as_str() == node_id)
                    .unwrap();

                if let Some(obj) = node.input.as_object_mut() {
                    let capability_input = obj
                        .entry("capability_input")
                        .or_insert_with(|| Value::Object(serde_json::Map::new()));

                    if let Some(input_map) = capability_input.as_object_mut() {
                        let mut destination = Value::Object(input_map.clone());
                        super::super::input_binding::validate_path(target_path)
                            .map_err(|error| ProofError::new(error.code, error.reason))?;
                        set_literal_path(&mut destination, target_path, value.clone())?;
                        *input_map = destination.as_object().unwrap().clone();
                    }
                }
                diff_summary.push(format!(
                    "修改节点 '{}' 的固定参数 {:?} 为 {:?}",
                    node_id, target_path, value
                ));
            }

            RepairOperation::ReplaceBinding { node_id, binding } => {
                let node = updated_nodes
                    .iter_mut()
                    .find(|n| n.id.as_str() == node_id)
                    .unwrap();

                // Validate source node is a direct dependency
                let source_node_id = binding.source.node_id().as_str();
                let has_edge = current_graph
                    .edges
                    .iter()
                    .any(|e| e.from.as_str() == source_node_id && e.to.as_str() == node_id);

                if !has_edge {
                    return Err(ProofError::new(
                        "input_binding_source_not_dependency",
                        format!(
                            "绑定源节点 '{}' 不是 '{}' 的直接上游依赖",
                            source_node_id, node_id
                        ),
                    ));
                }

                if let Some(obj) = node.input.as_object_mut() {
                    let bindings_val = obj
                        .entry("capability_bindings")
                        .or_insert_with(|| Value::Array(Vec::new()));

                    if let Some(bindings_arr) = bindings_val.as_array_mut() {
                        // Replace or append
                        let target_path_json = serde_json::to_value(&binding.target_path).unwrap();
                        let mut replaced = false;
                        for item in bindings_arr.iter_mut() {
                            if item.get("target_path") == Some(&target_path_json) {
                                *item = serde_json::to_value(binding).unwrap();
                                replaced = true;
                                break;
                            }
                        }
                        if !replaced {
                            bindings_arr.push(serde_json::to_value(binding).unwrap());
                        }
                    }
                }
                diff_summary.push(format!(
                    "修改节点 '{}' 绑定路径 {:?} <- '{}:{:?}'",
                    node_id,
                    binding.target_path,
                    binding.source.node_id().as_str(),
                    binding.source.path()
                ));
            }

            RepairOperation::ReplaceCapability {
                node_id,
                capability_id,
                literal_input,
                bindings,
            } => {
                // Must be allowed and ready
                if !contract.allowed_capability_ids.contains(capability_id) {
                    return Err(ProofError::new(
                        "capability_not_allowed",
                        format!("备选能力 '{}' 不在目标允许的能力集合中", capability_id),
                    ));
                }

                let cap = snapshot
                    .capabilities
                    .iter()
                    .find(|c| &c.capability_id == capability_id)
                    .ok_or_else(|| {
                        ProofError::new(
                            "capability_not_found",
                            format!("能力 '{}' 未在可用能力快照中找到", capability_id),
                        )
                    })?;

                if cap.readiness != "ready" {
                    return Err(ProofError::new(
                        "capability_unavailable",
                        format!("能力 '{}' 当前处于不可用状态", capability_id),
                    ));
                }

                let node = updated_nodes
                    .iter_mut()
                    .find(|n| n.id.as_str() == node_id)
                    .unwrap();

                if let Some(obj) = node.input.as_object_mut() {
                    obj.insert(
                        "executor_ref".to_string(),
                        Value::String(format!("capability://{}", capability_id)),
                    );
                    obj.insert(
                        "capability_input".to_string(),
                        Value::Object(literal_input.clone()),
                    );
                    obj.insert(
                        "capability_bindings".to_string(),
                        serde_json::to_value(bindings).unwrap(),
                    );
                }
                diff_summary.push(format!(
                    "替换节点 '{}' 的执行能力为 '{}'",
                    node_id, capability_id
                ));
            }

            RepairOperation::RerunNode { node_id } => {
                diff_summary.push(format!("标记重跑节点 '{}'", node_id));
            }
        }
    }

    // 5. Construct and validate candidate updated graph
    let updated_graph = TaskGraph::new(
        current_graph.id.clone(),
        GraphRevision::new(current_revision + 1)
            .map_err(|e| ProofError::new("revision_error", e.to_string()))?,
        updated_nodes,
        current_graph.edges.clone(),
    )
    .map_err(|e| ProofError::new("graph_validation_failed", e.to_string()))?;

    // 6. Validate input bindings consistency
    validate_graph_bindings(&updated_graph).map_err(|e| ProofError::new(e.code, e.reason))?;

    Ok(ValidatedRepairPatch {
        repair_id: proposal.repair_id.clone(),
        rerun_root_node_id: proposal.rerun_root_node_id.clone(),
        affected_node_ids,
        updated_graph,
        diff_summary,
    })
}

fn set_literal_path(target: &mut Value, path: &[String], value: Value) -> Result<(), ProofError> {
    let object = target.as_object_mut().ok_or_else(|| {
        ProofError::new("input_binding_type_mismatch", "固定参数父路径必须是 object")
    })?;
    if path.len() == 1 {
        object.insert(path[0].clone(), value);
        return Ok(());
    }
    let child = object
        .entry(path[0].clone())
        .or_insert_with(|| serde_json::json!({}));
    set_literal_path(child, &path[1..], value)
}

/// Applies a validated repair patch, advancing the graph revision and returning affected node ids.
pub fn apply_repair(
    patch: ValidatedRepairPatch,
    current_revision: u64,
) -> Result<(TaskGraph, u64, Vec<String>), ProofError> {
    if patch.updated_graph.revision.value() <= current_revision {
        return Err(ProofError::new(
            "invalid_revision",
            "修复应用必须推进图的版本号",
        ));
    }
    let new_revision = patch.updated_graph.revision.value();
    Ok((patch.updated_graph, new_revision, patch.affected_node_ids))
}

/// Computes the transitive downstream dependent nodes from a given root node ID.
pub fn compute_downstream_branch(graph: &TaskGraph, root_node_id: &str) -> Vec<String> {
    let mut affected = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(root_node_id.to_string());
    visited.insert(root_node_id.to_string());

    while let Some(current) = queue.pop_front() {
        affected.push(current.clone());

        for edge in &graph.edges {
            if edge.from.as_str() == current && !visited.contains(edge.to.as_str()) {
                visited.insert(edge.to.as_str().to_string());
                queue.push_back(edge.to.as_str().to_string());
            }
        }
    }

    affected
}
