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

const SYSTEM_PROMPT: &str = "你是一个严谨的桌面任务规划器。你的职责是将用户的自然语言目标分解为由真实工具能力构成的任务图 (DAG)。\n\
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

pub(crate) fn build_planning_messages(
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
) -> [ChatMessage; 2] {
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

    [
        ChatMessage {
            reasoning_content: None,
            role: "system".into(),
            content: Some(SYSTEM_PROMPT.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMessage {
            reasoning_content: None,
            role: "user".into(),
            content: Some(user_prompt),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ]
}

pub(crate) fn planning_prompt_char_count(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .filter_map(|message| message.content.as_deref())
        .map(|content| content.chars().count())
        .sum()
}

pub(super) fn validate_planning_prompt_size(messages: &[ChatMessage]) -> Result<(), ProofError> {
    if planning_prompt_char_count(messages) > MAX_PLANNING_PROMPT_CHARS {
        return Err(ProofError::new(
            "proofloop_planning_input_too_large",
            format!(
                "完整规划输入超过 {MAX_PLANNING_PROMPT_CHARS} 字符上限，请缩减目标描述或选择更少的工具能力范围"
            ),
        ));
    }
    Ok(())
}

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
    propose_candidate_with_usage_and_budget(
        workspace_id,
        user_goal,
        snapshot,
        llm_client,
        cancel,
        Budget::default(),
    )
    .await
}

pub async fn propose_candidate_with_usage_and_budget(
    workspace_id: &str,
    user_goal: &str,
    snapshot: &CapabilitySnapshot,
    llm_client: Option<&LlmClient>,
    cancel: &CancellationToken,
    budget: Budget,
) -> Result<(GoalContract, CandidatePlan, super::contracts::PlanningUsage), ProofError> {
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let planning_started = std::time::Instant::now();
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

    let messages = build_planning_messages(user_goal, snapshot);
    validate_planning_prompt_size(&messages)?;
    let max_output_tokens = client
        .output_tokens_for_total_budget(&messages, &[], budget.max_total_tokens)
        .ok_or_else(|| {
            ProofError::new(
                "proofloop_budget_exhausted",
                "规划输入已占满目标 token 预算，未调用模型",
            )
        })?;

    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ProofError::new("aborted", "规划请求已被取消")),
        result = tokio::time::timeout(client.invoke_timeout().min(std::time::Duration::from_millis(budget.max_wall_time_ms)), client.invoke_with_max_tokens(&messages, &[], max_output_tokens)) => {
            match result {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => return Err(unknown_usage_error(
                    "llm_invoke_failed",
                    format!("模型调用失败: {error}"),
                    started_at_ms,
                    planning_started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                )),
                Err(_) => return Err(unknown_usage_error(
                    "proofloop_budget_exhausted",
                    "规划耗时超过目标预算",
                    started_at_ms,
                    planning_started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                )),
            }
        }
    };

    let total_tokens = response
        .usage
        .as_ref()
        .map(|usage| u64::from(usage.total_tokens));
    let planning_usage = super::contracts::PlanningUsage {
        model_calls: 1,
        total_tokens,
        started_at_ms,
        elapsed_ms: Some(planning_started.elapsed().as_millis().min(u64::MAX as u128) as u64),
    };
    if total_tokens.is_none() {
        return Err(ProofError::new(
            "proofloop_usage_unavailable",
            "模型未返回真实 token 用量，不能保存新的预算目标",
        )
        .with_planning_usage(planning_usage));
    }
    if total_tokens.is_some_and(|tokens| tokens > budget.max_total_tokens) {
        return Err(ProofError::new(
            "proofloop_budget_exhausted",
            "模型消耗超过目标总 token 预算",
        )
        .with_planning_usage(planning_usage));
    }

    if cancel.is_cancelled() {
        return Err(
            ProofError::new("aborted", "规划请求已被取消").with_planning_usage(planning_usage)
        );
    }

    let choice = response.choices.first().ok_or_else(|| {
        ProofError::new("empty_llm_response", "模型未返回任何规划结果")
            .with_planning_usage(planning_usage.clone())
    })?;

    let content = choice.message.content.as_deref().unwrap_or("").trim();

    let clean_json = extract_json_block(content);

    let parsed: serde_json::Value = serde_json::from_str(clean_json).map_err(|e| {
        ProofError::new(
            "invalid_json",
            format!("模型返回的内容无法解析为 JSON: {e}"),
        )
        .with_planning_usage(planning_usage.clone())
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

    let mut raw_candidate = parsed.get("candidate").cloned().ok_or_else(|| {
        ProofError::new("invalid_candidate_plan", "缺少 candidate 字段")
            .with_planning_usage(planning_usage.clone())
    })?;
    // The enclosing response owns the protocol version. Preserve compatibility
    // with the original prompt, which omitted this field.
    if let Some(object) = raw_candidate.as_object_mut() {
        object
            .entry("schema_version")
            .or_insert(serde_json::json!(PROOFLOOP_SCHEMA_VERSION));
    }
    let candidate_plan: CandidatePlan = serde_json::from_value(raw_candidate)
        .ok()
        .ok_or_else(|| {
            ProofError::new(
                "invalid_candidate_plan",
                "返回的 JSON 中缺少有效的 candidate 字段",
            )
        })
        .map_err(|error| error.with_planning_usage(planning_usage.clone()))?;

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
        budget,
    };
    Ok((contract, candidate_plan, planning_usage))
}

fn unknown_usage_error(
    code: &str,
    message: impl Into<String>,
    started_at_ms: i64,
    elapsed_ms: u64,
) -> ProofError {
    ProofError::new(code, message).with_planning_usage(super::contracts::PlanningUsage {
        model_calls: 1,
        total_tokens: None,
        started_at_ms,
        elapsed_ms: Some(elapsed_ms),
    })
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
