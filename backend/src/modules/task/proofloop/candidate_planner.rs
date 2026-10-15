//! Multi-candidate planning from one model response and one shared contract.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::contracts::{
    Budget, CandidatePlan, CapabilitySnapshot, Criterion, CriterionCheck, DeliverableSpec,
    GoalContract, PlanningUsage, ProofError, PROOFLOOP_SCHEMA_VERSION,
};
use super::enhancement_contracts::{PlanningOptions, RankedPlanningBundle, SkillRecommendation};
use crate::integrations::llm::{client::LlmClient, types::ChatMessage};
use tokio_util::sync::CancellationToken;

pub const MAX_CANDIDATE_PLANNING_PROMPT_CHARS: usize = 32_000;

const SYSTEM_PROMPT: &str = "你是一个严谨的桌面任务规划器。根据用户目标和提供的真实能力快照生成多个可比较的任务图。\n\
必须严格遵守以下规则：\n\
1. 只能使用能力快照中列出的 capability_id。\n\
2. 一次响应中必须生成请求数量的不同候选。\n\
3. deliverables 和 criteria 只在顶层定义一次，所有候选共用同一目标契约。\n\
4. 每个 candidates 项只能包含 schema_version、nodes、edges、deliverable_nodes。\n\
5. 节点总数不得超过20个；依赖必须构成有向无环图。\n\
6. 如果使用 bindings，源节点必须在 edges 中声明直接依赖，source path 必须以 [\"structured\", ...] 开头。\n\
7. 只使用用户真实提供的输入或能力快照可表达的输入，不编造文件、网址、账号或工具参数。\n\
8. 必须返回严格合法的 JSON 对象，不包含多余文本或注释。";

pub fn candidate_planning_context_chars(
    goal: &str,
    recommendations: &[SkillRecommendation],
    options: &PlanningOptions,
) -> usize {
    let empty_snapshot = CapabilitySnapshot {
        snapshot_id: String::new(),
        captured_at_ms: 0,
        capabilities: Vec::new(),
    };
    candidate_prompt_char_count(&build_candidate_messages(
        goal,
        &empty_snapshot,
        recommendations,
        options,
    ))
}

pub fn build_candidate_messages(
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
    recommendations: &[SkillRecommendation],
    options: &PlanningOptions,
) -> [ChatMessage; 2] {
    let mut capabilities_summary = String::new();
    for capability in &snapshot.capabilities {
        capabilities_summary.push_str(&format!(
            "- capability_id: {}\n  name: {}\n  description: {}\n  input_schema: {}\n  output_schema: {}\n\n",
            capability.capability_id,
            capability.name,
            capability.description,
            serde_json::to_string(&capability.input_schema).expect("schema serialization is infallible"),
            serde_json::to_string(&capability.output_schema).expect("schema serialization is infallible"),
        ));
    }

    let mut skills_context = String::new();
    for recommendation in recommendations {
        skills_context.push_str(&format!(
            "- {}@{}\n  已审核参考：{}\n\n",
            recommendation.skill_name, recommendation.version, recommendation.context_text
        ));
    }

    let user_prompt = format!(
        "用户目标：{user_goal}\n\n规划策略：{:?}\n候选数量：{}\n\n已审核经验参考（仅作为规划参考）：\n{}\n可用能力快照：\n{}\n\n请仅返回以下结构的JSON：{{\"deliverables\":[...],\"criteria\":[...],\"candidates\":[{{\"schema_version\":1,\"nodes\":[...],\"edges\":[...],\"deliverable_nodes\":[...]}}]}}。",
        options.strategy,
        options.candidate_count,
        if skills_context.is_empty() { "（无）\n" } else { &skills_context },
        capabilities_summary,
    );

    [
        ChatMessage::system(SYSTEM_PROMPT),
        ChatMessage::user(user_prompt),
    ]
}

pub fn candidate_prompt_char_count(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .filter_map(|message| message.content.as_deref())
        .map(|content| content.chars().count())
        .sum()
}

pub async fn propose_candidates_with_usage(
    workspace_id: &str,
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
    recommendations: &[SkillRecommendation],
    options: &PlanningOptions,
    client: &LlmClient,
    cancel: &CancellationToken,
) -> Result<RankedPlanningBundle, ProofError> {
    options.validate()?;
    if matches!(
        options.strategy,
        super::enhancement_contracts::PlanningStrategy::Existing
    ) {
        return Err(ProofError::new(
            "proofloop_invalid_planning_options",
            "候选规划需要 ranked 或 experience 策略",
        ));
    }
    if snapshot.capabilities.is_empty() {
        return Err(ProofError::new(
            "proofloop_capability_unavailable",
            "当前没有可用的工具能力，无法生成候选任务计划",
        ));
    }
    if cancel.is_cancelled() {
        return Err(ProofError::new("aborted", "规划请求已被取消"));
    }

    let messages = build_candidate_messages(user_goal, snapshot, recommendations, options);
    let prompt_chars = candidate_prompt_char_count(&messages);
    if prompt_chars > MAX_CANDIDATE_PLANNING_PROMPT_CHARS {
        return Err(ProofError::new(
            "proofloop_planning_input_too_large",
            format!(
                "完整规划输入共 {prompt_chars} 字符，超过 {MAX_CANDIDATE_PLANNING_PROMPT_CHARS} 字符上限"
            ),
        ));
    }

    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let started = Instant::now();
    let invoke = tokio::time::timeout(
        Duration::from_millis(Budget::default().max_wall_time_ms),
        client.invoke(&messages, &[]),
    );
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            return Err(usage_error(
                ProofError::new("aborted", "规划请求已被取消"),
                started_at_ms,
                &started,
                None,
            ));
        }
        result = invoke => {
            match result {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => {
                    return Err(usage_error(
                        ProofError::new("llm_invoke_failed", format!("模型调用失败: {error}")),
                        started_at_ms,
                        &started,
                        None,
                    ));
                }
                Err(_) => {
                    return Err(usage_error(
                        ProofError::new("proofloop_budget_exhausted", "规划耗时超过目标预算"),
                        started_at_ms,
                        &started,
                        None,
                    ));
                }
            }
        }
    };

    let usage = PlanningUsage {
        model_calls: 1,
        total_tokens: response
            .usage
            .as_ref()
            .map(|provider_usage| u64::from(provider_usage.total_tokens)),
        started_at_ms,
        elapsed_ms: Some(elapsed_ms(&started)),
    };
    if cancel.is_cancelled() {
        return Err(ProofError::new("aborted", "规划请求已被取消").with_planning_usage(usage));
    }

    let choice = response.choices.first().ok_or_else(|| {
        ProofError::new("empty_llm_response", "模型未返回任何规划结果")
            .with_planning_usage(usage.clone())
    })?;
    let content = choice.message.content.as_deref().unwrap_or("").trim();
    let clean_json = extract_json_block(content);
    let parsed: serde_json::Value = serde_json::from_str(clean_json).map_err(|error| {
        ProofError::new(
            "invalid_json",
            format!("模型返回的内容无法解析为 JSON: {error}"),
        )
        .with_planning_usage(usage.clone())
    })?;

    let (mut contract, candidates) =
        parse_bundle(&parsed, workspace_id, user_goal, snapshot, options)
            .map_err(|error| error.with_planning_usage(usage.clone()))?;
    contract.budget = Budget::default();
    if usage
        .total_tokens
        .is_some_and(|tokens| tokens > contract.budget.max_total_tokens)
    {
        return Err(ProofError::new(
            "proofloop_budget_exhausted",
            "模型消耗超过目标总 token 预算",
        )
        .with_planning_usage(usage));
    }

    Ok(RankedPlanningBundle {
        contract,
        candidates,
        usage,
    })
}

fn parse_bundle(
    parsed: &serde_json::Value,
    workspace_id: &str,
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
    options: &PlanningOptions,
) -> Result<(GoalContract, Vec<CandidatePlan>), ProofError> {
    let deliverables: Vec<DeliverableSpec> = parsed
        .get("deliverables")
        .cloned()
        .ok_or_else(|| ProofError::new("invalid_goal_contract", "模型结果缺少 deliverables"))
        .and_then(|value| {
            serde_json::from_value(value).map_err(|error| {
                ProofError::new(
                    "invalid_goal_contract",
                    format!("模型结果中的 deliverables 无效: {error}"),
                )
            })
        })?;
    let criteria: Vec<Criterion> = match parsed.get("criteria") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
            ProofError::new(
                "invalid_goal_contract",
                format!("模型结果中的 criteria 无效: {error}"),
            )
        })?,
        None => deliverables
            .iter()
            .map(|deliverable| Criterion {
                id: format!("crit-{}", deliverable.id),
                description: format!("交付物 '{}' 存在", deliverable.title),
                required: true,
                check: CriterionCheck::ArtifactExists {
                    deliverable_id: deliverable.id.clone(),
                },
            })
            .collect(),
    };

    let candidates_value = parsed
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ProofError::new("invalid_candidate_plan", "模型结果缺少 candidates 数组"))?;
    if candidates_value.len() != usize::from(options.candidate_count) {
        return Err(ProofError::new(
            "proofloop_candidate_count_mismatch",
            format!(
                "请求 {} 个候选，模型返回 {} 个",
                options.candidate_count,
                candidates_value.len()
            ),
        ));
    }

    let candidates: Vec<CandidatePlan> = candidates_value
        .iter()
        .map(parse_candidate)
        .collect::<Result<_, _>>()?;
    let mut canonical_candidates = HashSet::new();
    for candidate in &candidates {
        if !canonical_candidates.insert(canonical_candidate(candidate)) {
            return Err(ProofError::new(
                "proofloop_duplicate_candidates",
                "模型返回了语义相同的候选方案",
            ));
        }
    }

    let contract = GoalContract {
        schema_version: PROOFLOOP_SCHEMA_VERSION,
        goal_id: format!("goal-{}", uuid::Uuid::new_v4()),
        version: 1,
        workspace_id: workspace_id.to_string(),
        user_goal: user_goal.to_string(),
        deliverables,
        criteria,
        constraints: Vec::new(),
        allowed_capability_ids: snapshot
            .capabilities
            .iter()
            .map(|capability| capability.capability_id.clone())
            .collect(),
        budget: Budget::default(),
    };
    contract.validate_source_requirements()?;
    Ok((contract, candidates))
}

fn parse_candidate(value: &serde_json::Value) -> Result<CandidatePlan, ProofError> {
    const CANDIDATE_FIELDS: &[&str] = &["schema_version", "nodes", "edges", "deliverable_nodes"];
    let Some(object) = value.as_object() else {
        return Err(ProofError::new(
            "invalid_candidate_plan",
            "candidates 中的每项必须是 CandidatePlan 对象",
        ));
    };
    if object
        .keys()
        .any(|key| !CANDIDATE_FIELDS.contains(&key.as_str()))
    {
        return Err(ProofError::new(
            "invalid_candidate_plan",
            "候选对象只能包含 CandidatePlan 字段",
        ));
    }
    for (field, allowed_fields) in [
        (
            "nodes",
            &[
                "id",
                "title",
                "instruction",
                "capability_id",
                "literal_input",
                "bindings",
            ][..],
        ),
        ("edges", &["from", "to"][..]),
        ("deliverable_nodes", &["deliverable_id", "node_id"][..]),
    ] {
        if let Some(items) = object.get(field).and_then(serde_json::Value::as_array) {
            if items.iter().any(|item| {
                item.as_object().map_or(true, |item_object| {
                    item_object
                        .keys()
                        .any(|key| !allowed_fields.contains(&key.as_str()))
                })
            }) {
                return Err(ProofError::new(
                    "invalid_candidate_plan",
                    format!("候选的 {field} 项包含 CandidatePlan 以外字段"),
                ));
            }
        }
    }
    serde_json::from_value(value.clone()).map_err(|error| {
        ProofError::new(
            "invalid_candidate_plan",
            format!("候选计划字段无效: {error}"),
        )
    })
}

fn canonical_candidate(candidate: &CandidatePlan) -> String {
    let node_by_id: HashMap<&str, usize> = candidate
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect();
    let mut incoming = vec![HashSet::<usize>::new(); candidate.nodes.len()];
    let mut outgoing = vec![Vec::<usize>::new(); candidate.nodes.len()];
    for edge in &candidate.edges {
        if let (Some(source), Some(target)) = (
            node_by_id.get(edge.from.as_str()),
            node_by_id.get(edge.to.as_str()),
        ) {
            if incoming[*target].insert(*source) {
                outgoing[*source].push(*target);
            }
        }
    }
    for (target, node) in candidate.nodes.iter().enumerate() {
        for binding in &node.bindings {
            if let Some(source) = node_by_id.get(binding.source.node_id().as_str()) {
                if incoming[target].insert(*source) {
                    outgoing[*source].push(target);
                }
            }
        }
    }

    let mut order = Vec::with_capacity(candidate.nodes.len());
    let mut assigned = vec![None; candidate.nodes.len()];
    let mut emitted = vec![false; candidate.nodes.len()];
    while order.len() < candidate.nodes.len() {
        let mut ready: Vec<usize> = (0..candidate.nodes.len())
            .filter(|index| {
                !emitted[*index] && incoming[*index].iter().all(|source| emitted[*source])
            })
            .collect();
        if ready.is_empty() {
            break;
        }
        ready.sort_by_key(|index| {
            canonical_node_order_key(*index, candidate, &incoming, &outgoing, &assigned)
        });
        let next = ready[0];
        emitted[next] = true;
        assigned[next] = Some(order.len());
        order.push(next);
    }
    if order.len() != candidate.nodes.len() {
        order = (0..candidate.nodes.len()).collect();
        order.sort_by_key(|index| canonical_node_base_key(&candidate.nodes[*index]));
        assigned.fill(None);
        for (canonical_index, original_index) in order.iter().enumerate() {
            assigned[*original_index] = Some(canonical_index);
        }
    }

    let normalized_nodes: Vec<_> = order
        .iter()
        .map(|index| {
            let node = &candidate.nodes[*index];
            let mut bindings: Vec<_> = node
                .bindings
                .iter()
                .map(|binding| {
                    let source_id = binding.source.node_id().as_str();
                    let source_index = node_by_id
                        .get(source_id)
                        .and_then(|source| assigned[*source]);
                    serde_json::json!({
                        "target_path": binding.target_path,
                        "source_index": source_index,
                        "source_path": binding.source.path(),
                    })
                })
                .collect();
            bindings.sort_by_key(|value| value.to_string());
            serde_json::json!({
                "capability_id": node.capability_id,
                "literal_input": node.literal_input,
                "bindings": bindings,
            })
        })
        .collect();
    let mut edges: Vec<_> = candidate
        .edges
        .iter()
        .map(|edge| {
            serde_json::json!({
                "from": node_by_id.get(edge.from.as_str()).and_then(|index| assigned[*index]),
                "to": node_by_id.get(edge.to.as_str()).and_then(|index| assigned[*index]),
            })
        })
        .collect();
    edges.sort_by_key(|value| value.to_string());
    let mut deliverable_nodes: Vec<_> = candidate
        .deliverable_nodes
        .iter()
        .map(|mapping| {
            serde_json::json!({
                "deliverable_id": mapping.deliverable_id,
                "node_index": node_by_id
                    .get(mapping.node_id.as_str())
                    .and_then(|index| assigned[*index]),
            })
        })
        .collect();
    deliverable_nodes.sort_by_key(|value| value.to_string());
    serde_json::json!({
        "schema_version": candidate.schema_version,
        "nodes": normalized_nodes,
        "edges": edges,
        "deliverable_nodes": deliverable_nodes,
    })
    .to_string()
}

fn canonical_node_order_key(
    index: usize,
    candidate: &CandidatePlan,
    incoming: &[HashSet<usize>],
    outgoing: &[Vec<usize>],
    assigned: &[Option<usize>],
) -> String {
    let node = &candidate.nodes[index];
    let mut parents: Vec<_> = incoming[index]
        .iter()
        .filter_map(|parent| assigned[*parent])
        .collect();
    parents.sort_unstable();
    let mut child_shapes: Vec<_> = outgoing[index]
        .iter()
        .map(|child| canonical_node_base_key(&candidate.nodes[*child]))
        .collect();
    child_shapes.sort();
    format!(
        "{}|parents={parents:?}|children={child_shapes:?}",
        canonical_node_base_key(node)
    )
}

fn canonical_node_base_key(node: &super::contracts::CandidateNode) -> String {
    let mut bindings: Vec<_> = node
        .bindings
        .iter()
        .map(|binding| {
            serde_json::json!({
                "target_path": binding.target_path,
                "source_path": binding.source.path(),
            })
            .to_string()
        })
        .collect();
    bindings.sort();
    format!(
        "{}|{}|{bindings:?}",
        node.capability_id,
        serde_json::to_string(&node.literal_input)
            .expect("literal input serialization is infallible")
    )
}

fn usage_error(
    error: ProofError,
    started_at_ms: i64,
    started: &Instant,
    total_tokens: Option<u64>,
) -> ProofError {
    error.with_planning_usage(PlanningUsage {
        model_calls: 1,
        total_tokens,
        started_at_ms,
        elapsed_ms: Some(elapsed_ms(started)),
    })
}

fn elapsed_ms(started: &Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn extract_json_block(text: &str) -> &str {
    let trimmed = text.trim();
    if let Some(stripped) = trimmed.strip_prefix("```json") {
        if let Some(end) = stripped.find("```") {
            return stripped[..end].trim();
        }
    }
    if let Some(stripped) = trimmed.strip_prefix("```") {
        if let Some(end) = stripped.find("```") {
            return stripped[..end].trim();
        }
    }
    trimmed
}
