//! Deterministic compiler for ProofLoop candidate plans.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/01-capability-and-compiler.md §5
//!
//! Evaluates candidate nodes and bindings strictly against capability schemas
//! and goal contracts. Does not call tools or grant permissions.

mod inputs;
use inputs::{
    collect_missing_inputs, literal_contains_path, remove_required_checks,
    resolve_input_property_type, resolve_output_property_type, types_compatible,
};

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

use super::contracts::{
    BindingConfirmation, CandidateNode, CandidatePlan, CapabilityKindSpec, CapabilitySnapshot,
    CapabilitySpec, CompileResult, CompileStatus, GoalContract, PendingInput, PendingInputReason,
    ProofError,
};
use crate::modules::task::{
    application::capability_binding::has_header_binding,
    input_binding::{validate_graph_bindings, validate_set},
    task_graph::{
        GraphRevision, TaskEdge, TaskGraph, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind,
    },
};

pub const MAX_PLAN_NODES: usize = 20;

/// Pure function compiler from GoalContract + CandidatePlan + CapabilitySnapshot into CompileResult.
pub fn compile_candidate(
    goal: &GoalContract,
    candidate: &CandidatePlan,
    snapshot: &CapabilitySnapshot,
    graph_id: &TaskGraphId,
    confirmations: &[BindingConfirmation],
) -> CompileResult {
    let mut diagnostics: Vec<ProofError> = Vec::new();
    let mut pending_inputs: Vec<PendingInput> = Vec::new();

    if goal.schema_version != 1 || candidate.schema_version != 1 {
        diagnostics.push(ProofError::new(
            "unsupported_schema_version",
            "目标或候选计划版本不受支持",
        ));
    }

    // 1. Check node count bounds
    if candidate.nodes.is_empty() {
        diagnostics.push(ProofError::new("empty_plan", "候选计划中没有任何节点"));
        return CompileResult {
            status: CompileStatus::Invalid,
            graph: None,
            diagnostics,
            pending_inputs,
        };
    }

    if candidate.nodes.len() > MAX_PLAN_NODES {
        diagnostics.push(ProofError::new(
            "too_many_nodes",
            format!(
                "候选计划节点数超过上限 {} (实际 {})",
                MAX_PLAN_NODES,
                candidate.nodes.len()
            ),
        ));
    }

    // 2. Validate node identities
    let mut node_map: HashMap<&str, &CandidateNode> = HashMap::new();
    for node in &candidate.nodes {
        if TaskNodeId::new(&node.id).is_err() {
            diagnostics.push(
                ProofError::new("invalid_node_id", format!("节点标识 '{}' 不合法", node.id))
                    .with_node(&node.id),
            );
        }
        if node_map.insert(&node.id, node).is_some() {
            diagnostics.push(
                ProofError::new(
                    "duplicate_node_id",
                    format!("发现重复的节点标识 '{}'", node.id),
                )
                .with_node(&node.id),
            );
        }
    }

    // 3. Check deliverable coverage
    for deliverable in &goal.deliverables {
        let mapped = candidate
            .deliverable_nodes
            .iter()
            .find(|m| m.deliverable_id == deliverable.id);
        match mapped {
            None => {
                diagnostics.push(ProofError::new(
                    "proofloop_goal_uncovered",
                    format!("目标交付物 '{}' 未关联任何输出节点", deliverable.title),
                ));
            }
            Some(mapping) => {
                if !node_map.contains_key(mapping.node_id.as_str()) {
                    diagnostics.push(
                        ProofError::new(
                            "proofloop_goal_uncovered",
                            format!(
                                "目标交付物 '{}' 关联的节点 '{}' 不存在",
                                deliverable.title, mapping.node_id
                            ),
                        )
                        .with_node(&mapping.node_id),
                    );
                }
            }
        }
    }

    // 4. Validate edges & DAG
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for node in &candidate.nodes {
        adjacency.insert(&node.id, Vec::new());
        in_degree.insert(&node.id, 0);
    }

    let mut seen_edges: HashSet<(&str, &str)> = HashSet::new();
    for edge in &candidate.edges {
        let from = edge.from.as_str();
        let to = edge.to.as_str();

        if !node_map.contains_key(from) {
            diagnostics.push(ProofError::new(
                "invalid_edge_source",
                format!("依赖边起点节点 '{}' 不存在", from),
            ));
            continue;
        }
        if !node_map.contains_key(to) {
            diagnostics.push(ProofError::new(
                "invalid_edge_target",
                format!("依赖边终点节点 '{}' 不存在", to),
            ));
            continue;
        }
        if from == to {
            diagnostics.push(
                ProofError::new("self_loop_edge", format!("节点 '{}' 不能依赖自身", from))
                    .with_node(from),
            );
            continue;
        }
        if !seen_edges.insert((from, to)) {
            diagnostics.push(ProofError::new(
                "duplicate_edge",
                format!("重复的依赖边 '{}' -> '{}'", from, to),
            ));
            continue;
        }

        adjacency.get_mut(from).unwrap().push(to);
        *in_degree.get_mut(to).unwrap() += 1;
    }

    // DAG Cycle check (Kahn's algorithm)
    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut visited_count = 0;
    while let Some(node_id) = queue.pop_front() {
        visited_count += 1;
        if let Some(neighbors) = adjacency.get(node_id) {
            for &neighbor in neighbors {
                let deg = in_degree.get_mut(neighbor).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(neighbor);
                }
            }
        }
    }

    if visited_count < candidate.nodes.len() {
        diagnostics.push(ProofError::new(
            "cycle_detected",
            "候选计划存在循环依赖，必须为有向无环图 (DAG)",
        ));
    }

    // Map snapshot capabilities by capability_id
    let cap_map: HashMap<&str, &CapabilitySpec> = snapshot
        .capabilities
        .iter()
        .map(|c| (c.capability_id.as_str(), c))
        .collect();

    // 5. Per-node validation
    for node in &candidate.nodes {
        // Check allowed capability
        if !goal
            .allowed_capability_ids
            .iter()
            .any(|a| a == &node.capability_id)
        {
            diagnostics.push(
                ProofError::new(
                    "proofloop_capability_not_allowed",
                    format!(
                        "节点 '{}' 使用了未被目标允许的能力 '{}'",
                        node.id, node.capability_id
                    ),
                )
                .with_node(&node.id),
            );
        }

        let cap_spec = match cap_map.get(node.capability_id.as_str()) {
            Some(spec) => *spec,
            None => {
                diagnostics.push(
                    ProofError::new(
                        "proofloop_capability_unavailable",
                        format!(
                            "节点 '{}' 的能力 '{}' 不在可用能力快照中",
                            node.id, node.capability_id
                        ),
                    )
                    .with_node(&node.id),
                );
                continue;
            }
        };

        if cap_spec.readiness != "ready" {
            diagnostics.push(
                ProofError::new(
                    "proofloop_capability_unavailable",
                    format!(
                        "节点 '{}' 的能力 '{}' 处于不可用状态: {}",
                        node.id,
                        node.capability_id,
                        cap_spec.unavailable_reason.as_deref().unwrap_or("未就绪")
                    ),
                )
                .with_node(&node.id),
            );
        }

        if cap_spec.kind == CapabilityKindSpec::Workflow && !node.bindings.is_empty() {
            diagnostics.push(
                ProofError::new(
                    "proofloop_unsupported_schema",
                    "工作流继续使用既有输入契约，当前不支持 MCP 类型化参数绑定",
                )
                .with_node(&node.id),
            );
        }
        if let Some(schema) = &cap_spec.input_schema {
            let mut literal_schema = schema.clone();
            remove_required_checks(&mut literal_schema);
            if let Err(error) = super::super::binding_schema::validate_arguments(
                &Value::Object(node.literal_input.clone()),
                &literal_schema,
            ) {
                diagnostics.push(ProofError::new(error.code, error.reason).with_node(&node.id));
            }
            collect_missing_inputs(
                schema,
                &Value::Object(node.literal_input.clone()),
                &node.bindings,
                &[],
                &node.id,
                &mut pending_inputs,
            );
        } else if cap_spec.kind == CapabilityKindSpec::McpTool {
            pending_inputs.push(PendingInput {
                node_id: node.id.clone(),
                target_path: vec![],
                reason: PendingInputReason::UnsupportedSchema,
            });
        }

        // Check header forbidden
        if cap_spec
            .input_schema
            .as_ref()
            .is_some_and(has_header_binding)
        {
            diagnostics.push(
                ProofError::new(
                    "header_unsupported",
                    format!(
                        "节点 '{}' 的能力需要安全请求头，当前不支持安全凭据绑定",
                        node.id
                    ),
                )
                .with_node(&node.id),
            );
        }

        // Validate bindings set shape
        if let Err(binding_err) = validate_set(&node.bindings) {
            diagnostics.push(
                ProofError::new(
                    binding_err.code,
                    format!("节点 '{}' 绑定格式非法: {}", node.id, binding_err.reason),
                )
                .with_node(&node.id),
            );
        }

        // Check collision between literal_input and bindings
        for binding in &node.bindings {
            if literal_contains_path(&node.literal_input, &binding.target_path) {
                diagnostics.push(
                    ProofError::new(
                        "binding_target_collision",
                        format!(
                            "节点 '{}' 参数 '{}' 既有固定输入又有动态绑定，目标发生冲突",
                            node.id,
                            binding.target_path.join(".")
                        ),
                    )
                    .with_node(&node.id)
                    .with_target_path(binding.target_path.clone()),
                );
            }
        }

        // Validate each binding's source, edge dependency and schemas
        for binding in &node.bindings {
            let source_id = binding.source.node_id().as_str();
            let has_direct_edge = seen_edges.contains(&(source_id, node.id.as_str()));
            if !has_direct_edge {
                diagnostics.push(
                    ProofError::new(
                        "input_binding_source_not_dependency",
                        format!(
                            "节点 '{}' 绑定了源节点 '{}' 的输出，但两者之间不存在直接依赖边",
                            node.id, source_id
                        ),
                    )
                    .with_node(&node.id),
                );
            }

            // Find source node and its capability
            if let Some(source_node) = node_map.get(source_id) {
                if let Some(source_cap) = cap_map.get(source_node.capability_id.as_str()) {
                    let source_path = binding.source.path();
                    let prefix = &source_cap.output_path_prefix;

                    if !source_path.starts_with(prefix) {
                        diagnostics.push(
                            ProofError::new(
                                "invalid_source_path",
                                format!(
                                    "节点 '{}' 来源路径 {:?} 未以能力输出前缀 {:?} 开头",
                                    node.id, source_path, prefix
                                ),
                            )
                            .with_node(&node.id),
                        );
                        continue;
                    }

                    let relative_path = &source_path[prefix.len()..];
                    if relative_path.is_empty() {
                        diagnostics.push(
                            ProofError::new(
                                "invalid_source_path",
                                format!("节点 '{}' 来源路径缺少具体的属性字段", node.id),
                            )
                            .with_node(&node.id),
                        );
                        continue;
                    }

                    // Check source schema or user confirmation
                    let resolved_source_type = resolve_output_property_type(
                        source_cap.output_schema.as_ref(),
                        relative_path,
                    );

                    let observed_confirmation = confirmations.iter().find(|c| {
                        c.node_id == node.id
                            && c.target_path == binding.target_path
                            && c.source_node_id == source_id
                            && c.source_path == source_path
                            && c.source_capability_id == source_cap.capability_id
                            && c.confirmed_at_ms > 0
                            && !c.source_execution_id.is_empty()
                    });

                    let effective_source_type = match (resolved_source_type, observed_confirmation)
                    {
                        (Some(t), _) => Some(t),
                        (None, Some(c)) => Some(c.observed_type.clone()),
                        (None, None) => {
                            // Needs confirmation or missing schema
                            pending_inputs.push(PendingInput {
                                node_id: node.id.clone(),
                                target_path: binding.target_path.clone(),
                                reason: PendingInputReason::UnknownOutput,
                            });
                            None
                        }
                    };

                    // Type compatibility check with target schema
                    if let Some(src_type) = effective_source_type {
                        if let Some(target_type) = resolve_input_property_type(
                            cap_spec.input_schema.as_ref(),
                            &binding.target_path,
                        ) {
                            if !types_compatible(&src_type, &target_type) {
                                diagnostics.push(
                                    ProofError::new(
                                        "input_binding_schema_mismatch",
                                        format!(
                                            "节点 '{}' 字段 {:?} 类型不兼容: 源输出为 '{}'，目标参数需要 '{}'",
                                            node.id, binding.target_path, src_type, target_type
                                        ),
                                    )
                                    .with_node(&node.id)
                                    .with_target_path(binding.target_path.clone()),
                                );
                            }
                        } else {
                            pending_inputs.push(PendingInput {
                                node_id: node.id.clone(),
                                target_path: binding.target_path.clone(),
                                reason: PendingInputReason::UnsupportedSchema,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check if fatal diagnostics exist
    let has_fatal = !diagnostics.is_empty();

    if has_fatal {
        return CompileResult {
            status: CompileStatus::Invalid,
            graph: None,
            diagnostics,
            pending_inputs,
        };
    }

    if !pending_inputs.is_empty() {
        return CompileResult {
            status: CompileStatus::NeedsInput,
            graph: None,
            diagnostics,
            pending_inputs,
        };
    }

    // 6. Build the TaskGraph
    let mut task_nodes = Vec::with_capacity(candidate.nodes.len());
    for node in &candidate.nodes {
        let cap_spec = cap_map.get(node.capability_id.as_str()).unwrap();
        let input_value = serde_json::json!({
            "executor_ref": match cap_spec.kind {
                CapabilityKindSpec::McpTool => format!("capability://{}", cap_spec.capability_id),
                CapabilityKindSpec::Workflow => cap_spec.capability_id.strip_prefix("workflow.").map(|id| format!("workflow://{id}")).unwrap_or_else(|| cap_spec.executor_ref.clone()),
            },
            "instruction": node.instruction,
            "acceptance_criteria": Vec::<String>::new(),
            "capability_input": node.literal_input,
            "capability_bindings": node.bindings
        });

        let node_id = match TaskNodeId::new(&node.id) {
            Ok(id) => id,
            Err(e) => {
                diagnostics.push(ProofError::new("invalid_node_id", e.to_string()));
                return CompileResult {
                    status: CompileStatus::Invalid,
                    graph: None,
                    diagnostics,
                    pending_inputs,
                };
            }
        };

        let task_node =
            match TaskNode::new(node_id, TaskNodeKind::Work, node.title.clone(), input_value) {
                Ok(n) => n,
                Err(e) => {
                    diagnostics.push(ProofError::new("node_construction_failed", e.to_string()));
                    return CompileResult {
                        status: CompileStatus::Invalid,
                        graph: None,
                        diagnostics,
                        pending_inputs,
                    };
                }
            };

        task_nodes.push(task_node);
    }

    let mut task_edges = Vec::with_capacity(candidate.edges.len());
    for edge in &candidate.edges {
        let from_id = TaskNodeId::new(&edge.from).unwrap();
        let to_id = TaskNodeId::new(&edge.to).unwrap();
        task_edges.push(TaskEdge::new(from_id, to_id));
    }

    let graph = match TaskGraph::new(
        graph_id.clone(),
        GraphRevision::initial(),
        task_nodes,
        task_edges,
    ) {
        Ok(g) => g,
        Err(e) => {
            diagnostics.push(ProofError::new("graph_validation_failed", e.to_string()));
            return CompileResult {
                status: CompileStatus::Invalid,
                graph: None,
                diagnostics,
                pending_inputs,
            };
        }
    };

    if let Err(e) = validate_graph_bindings(&graph) {
        diagnostics.push(ProofError::new(e.code, e.reason));
        return CompileResult {
            status: CompileStatus::Invalid,
            graph: None,
            diagnostics,
            pending_inputs,
        };
    }

    CompileResult {
        status: CompileStatus::Ready,
        graph: Some(graph),
        diagnostics,
        pending_inputs,
    }
}
