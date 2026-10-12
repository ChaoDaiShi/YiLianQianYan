//! Natural-language Goal and CandidatePlan generator for ProofLoop.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop/01-capability-and-compiler.md §4
//!
//! Generates candidate plans using LLM. Model output is unprivileged and untrusted;
//! results are strictly parsed and passed to the deterministic compiler.

use tokio_util::sync::CancellationToken;

use super::contracts::{
    Budget, CandidatePlan, CapabilitySnapshot, Criterion, CriterionCheck, DeliverableSpec,
    GoalContract, ProofError, PROOFLOOP_SCHEMA_VERSION,
};
use crate::integrations::llm::{client::LlmClient, types::ChatMessage};

pub const MAX_PLANNING_PROMPT_CHARS: usize = 32_000;

pub async fn propose_candidate(
    workspace_id: &str,
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
    llm_client: Option<&LlmClient>,
    cancel: &CancellationToken,
) -> Result<(GoalContract, CandidatePlan), ProofError> {
    let (goal, candidate, _) =
        propose_candidate_with_usage(workspace_id, user_goal, snapshot, llm_client, cancel).await?;
    Ok((goal, candidate))
}

pub async fn propose_candidate_with_usage(
    workspace_id: &str,
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
    llm_client: Option<&LlmClient>,
    cancel: &CancellationToken,
) -> Result<(GoalContract, CandidatePlan, super::contracts::PlanningUsage), ProofError> {
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    if cancel.is_cancelled() {
        return Err(ProofError::new("aborted", "规划请求已被取消"));
    }

    let client = llm_client.ok_or_else(|| {
        ProofError::new(
            "proofloop_model_not_configured",
            "大模型未配置或不可用，无法生成候选任务计划",
        )
    })?;

    if snapshot.capabilities.is_empty() {
        return Err(ProofError::new(
            "proofloop_capability_unavailable",
            "当前没有可用的工具能力，无法编排任务图",
        ));
    }

    let system_prompt = "你是一个严谨的桌面任务规划器。你的职责是将用户的自然语言目标分解为由真实工具能力构成的任务图 (DAG)。\n\
请根据提供的工具清单生成任务候选方案。\n\
必须严格遵守以下规则：\n\
1. 只能使用给定的 capability_id，严禁虚构或推测不存在的工具。\n\
2. 节点总数不得超过 20 个。\n\
3. 必须输出严格合法的 JSON 对象，不包含多余文本或注释。\n\
4. JSON 结构必须包含：\n\
   - deliverables: [ { \"id\": string, \"title\": string, \"mime_type\": string } ]\n\
   - criteria: [ { \"id\": string, \"description\": string, \"required\": bool, \"check\": { \"kind\": \"artifact_exists\", \"deliverable_id\": string } } ]\n\
   - candidate: {\n\
       \"schema_version\": 1,\n\
       \"nodes\": [ { \"id\": string, \"title\": string, \"instruction\": string, \"capability_id\": string, \"literal_input\": {}, \"bindings\": [] } ],\n\
       \"edges\": [ { \"from\": string, \"to\": string } ],\n\
       \"deliverable_nodes\": [ { \"deliverable_id\": string, \"node_id\": string } ]\n\
     }\n\
5. 如果使用 bindings，源节点必须在 edges 中声明直接依赖，且 source path 必须以 [\"structured\", ...] 开头。\n\
6. 严禁生成系统级提权、外部任意命令或未经授权的操作。";

    // Build capabilities description
    let mut caps_summary = String::new();
    for cap in &snapshot.capabilities {
        caps_summary.push_str(&format!(
            "- capability_id: {}\n  name: {}\n  description: {}\n  input_schema: {}\n  output_schema: {}\n\n",
            cap.capability_id,
            cap.name,
            cap.description,
            serde_json::to_string(&cap.input_schema).unwrap_or_else(|_| "null".to_string()),
            serde_json::to_string(&cap.output_schema).unwrap_or_else(|_| "null".to_string())
        ));
    }

    let user_prompt = format!(
        "用户目标：{}\n\n可用能力快照：\n{}\n\n请直接返回规划 JSON：",
        user_goal, caps_summary
    );

    let messages = vec![
        ChatMessage {
            role: "system".into(),
            content: Some(system_prompt.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMessage {
            role: "user".into(),
            content: Some(user_prompt),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ProofError::new("aborted", "规划请求已被取消")),
        result = tokio::time::timeout(std::time::Duration::from_millis(Budget::default().max_wall_time_ms), client.invoke(&messages, &[])) => {
            result.map_err(|_| ProofError::new("proofloop_budget_exhausted", "规划耗时超过目标预算"))?
                .map_err(|e| ProofError::new("llm_invoke_failed", format!("模型调用失败: {e}")))?
        }
    };

    if cancel.is_cancelled() {
        return Err(ProofError::new("aborted", "规划请求已被取消"));
    }

    let choice = response
        .choices
        .first()
        .ok_or_else(|| ProofError::new("empty_llm_response", "模型未返回任何规划结果"))?;

    let content = choice.message.content.as_deref().unwrap_or("").trim();

    let clean_json = extract_json_block(content);

    let parsed: serde_json::Value = serde_json::from_str(clean_json).map_err(|e| {
        ProofError::new(
            "invalid_json",
            format!("模型返回的内容无法解析为 JSON: {e}"),
        )
    })?;

    let deliverables: Vec<DeliverableSpec> = parsed
        .get("deliverables")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let criteria: Vec<Criterion> = parsed
        .get("criteria")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(|| {
            // Default criteria: each deliverable exists
            deliverables
                .iter()
                .map(|d| Criterion {
                    id: format!("crit-{}", d.id),
                    description: format!("交付物 '{}' 存在", d.title),
                    required: true,
                    check: CriterionCheck::ArtifactExists {
                        deliverable_id: d.id.clone(),
                    },
                })
                .collect()
        });

    let mut raw_candidate = parsed
        .get("candidate")
        .cloned()
        .ok_or_else(|| ProofError::new("invalid_candidate_plan", "缺少 candidate 字段"))?;
    // The enclosing response owns the protocol version. Preserve compatibility
    // with the original prompt, which omitted this field.
    if let Some(object) = raw_candidate.as_object_mut() {
        object
            .entry("schema_version")
            .or_insert(serde_json::json!(PROOFLOOP_SCHEMA_VERSION));
    }
    let candidate_plan: CandidatePlan =
        serde_json::from_value(raw_candidate).ok().ok_or_else(|| {
            ProofError::new(
                "invalid_candidate_plan",
                "返回的 JSON 中缺少有效的 candidate 字段",
            )
        })?;

    let allowed_capability_ids: Vec<String> = snapshot
        .capabilities
        .iter()
        .map(|c| c.capability_id.clone())
        .collect();

    let goal_id = format!("goal-{}", uuid::Uuid::new_v4());

    let contract = GoalContract {
        schema_version: PROOFLOOP_SCHEMA_VERSION,
        goal_id,
        version: 1,
        workspace_id: workspace_id.to_string(),
        user_goal: user_goal.to_string(),
        deliverables,
        criteria,
        constraints: Vec::new(),
        allowed_capability_ids,
        budget: Budget::default(),
    };

    let total_tokens = response
        .usage
        .as_ref()
        .map(|usage| u64::from(usage.total_tokens));
    if total_tokens.is_some_and(|tokens| tokens > contract.budget.max_total_tokens) {
        return Err(ProofError::new(
            "proofloop_budget_exhausted",
            "模型消耗超过目标总 token 预算",
        ));
    }
    Ok((
        contract,
        candidate_plan,
        super::contracts::PlanningUsage {
            model_calls: 1,
            total_tokens,
            started_at_ms,
        },
    ))
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
