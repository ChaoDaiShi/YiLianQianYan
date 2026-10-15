use super::ProofError;

fn prompt_input_upper_bound(
    strategy: crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy,
    goal: &str,
    snapshot: &crate::modules::task::proofloop::CapabilitySnapshot,
    recommendations: &[crate::modules::task::proofloop::enhancement_contracts::SkillRecommendation],
    options: &crate::modules::task::proofloop::enhancement_contracts::PlanningOptions,
) -> Result<u64, ProofError> {
    let messages = match strategy {
        crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy::Existing => {
            crate::modules::task::proofloop::planner::build_planning_messages(goal, snapshot)
        }
        crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy::Ranked
        | crate::modules::task::proofloop::enhancement_contracts::PlanningStrategy::Experience => {
            crate::modules::task::proofloop::candidate_planner::build_candidate_messages(
                goal,
                snapshot,
                recommendations,
                options,
            )
        }
    };
    crate::integrations::llm::client::conservative_input_token_upper_bound(&messages, &[])
        .ok_or_else(|| {
            ProofError::new(
                "proofloop_usage_unavailable",
                "无法保守估算规划输入用量，未调用模型",
            )
        })
}

fn prompt_fits_budget(input_tokens: u64, total_budget: u64, output_reserve: u32) -> bool {
    input_tokens
        .checked_add(u64::from(output_reserve))
        .is_some_and(|needed| needed <= total_budget)
}

pub(super) fn fit_ranked_snapshot_to_budget(
    descriptors: &[crate::modules::capability::CapabilityDescriptor],
    routes: &[crate::modules::task::proofloop::enhancement_contracts::RouteScore],
    explicit_ids: Option<&[String]>,
    now_ms: i64,
    goal: &str,
    recommendations: &[crate::modules::task::proofloop::enhancement_contracts::SkillRecommendation],
    options: &crate::modules::task::proofloop::enhancement_contracts::PlanningOptions,
    total_budget: u64,
    output_reserve: u32,
) -> Result<crate::modules::task::proofloop::CapabilitySnapshot, ProofError> {
    use crate::modules::task::proofloop::{
        candidate_planner::candidate_planning_context_chars, planner::MAX_PLANNING_PROMPT_CHARS,
        routing::build_ranked_snapshot,
    };
    let max_snapshot_chars = MAX_PLANNING_PROMPT_CHARS.saturating_sub(
        candidate_planning_context_chars(goal, recommendations, options),
    );
    if explicit_ids.is_some() {
        let snapshot = build_ranked_snapshot(
            descriptors,
            routes,
            explicit_ids,
            now_ms,
            max_snapshot_chars,
        )?;
        let input =
            prompt_input_upper_bound(options.strategy, goal, &snapshot, recommendations, options)?;
        if !prompt_fits_budget(input, total_budget, output_reserve) {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "显式选择的能力无法同时满足规划输入和输出 token 预算",
            ));
        }
        return Ok(snapshot);
    }

    let mut snapshot =
        build_ranked_snapshot(descriptors, routes, None, now_ms, max_snapshot_chars)?;
    while !snapshot.capabilities.is_empty() {
        let input =
            prompt_input_upper_bound(options.strategy, goal, &snapshot, recommendations, options)?;
        if prompt_fits_budget(input, total_budget, output_reserve) {
            return Ok(snapshot);
        }
        snapshot.capabilities.pop();
    }
    Err(ProofError::new(
        "proofloop_budget_exhausted",
        "规划目标和至少一项能力无法在保留输出额度后装入 token 预算",
    ))
}

pub(super) fn fit_existing_snapshot_to_budget(
    descriptors: &[crate::modules::capability::CapabilityDescriptor],
    explicit_ids: Option<&[String]>,
    now_ms: i64,
    goal: &str,
    options: &crate::modules::task::proofloop::enhancement_contracts::PlanningOptions,
    total_budget: u64,
    output_reserve: u32,
) -> Result<crate::modules::task::proofloop::CapabilitySnapshot, ProofError> {
    use crate::modules::task::proofloop::{
        capabilities::{build_capability_snapshot_with_char_limit, MAX_SNAPSHOT_CONTEXT_CHARS},
        planner::MAX_PLANNING_PROMPT_CHARS,
    };
    if explicit_ids.is_some() {
        let snapshot = build_capability_snapshot_with_char_limit(
            descriptors,
            explicit_ids,
            now_ms,
            MAX_SNAPSHOT_CONTEXT_CHARS,
        );
        if explicit_ids.is_some_and(|ids| {
            ids.iter().any(|id| {
                !snapshot
                    .capabilities
                    .iter()
                    .any(|capability| capability.capability_id == *id)
            })
        }) {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "显式选择的能力不能完整装入规划输入",
            ));
        }
        let input = prompt_input_upper_bound(options.strategy, goal, &snapshot, &[], options)?;
        if !prompt_fits_budget(input, total_budget, output_reserve) {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "显式选择的能力无法同时满足规划输入和输出 token 预算",
            ));
        }
        return Ok(snapshot);
    }

    let mut snapshot = build_capability_snapshot_with_char_limit(
        descriptors,
        None,
        now_ms,
        MAX_SNAPSHOT_CONTEXT_CHARS,
    );
    while !snapshot.capabilities.is_empty() {
        let chars = crate::modules::task::proofloop::planner::planning_prompt_char_count(
            &crate::modules::task::proofloop::planner::build_planning_messages(goal, &snapshot),
        );
        let input = prompt_input_upper_bound(options.strategy, goal, &snapshot, &[], options)?;
        if chars <= MAX_PLANNING_PROMPT_CHARS
            && prompt_fits_budget(input, total_budget, output_reserve)
        {
            return Ok(snapshot);
        }
        snapshot.capabilities.pop();
    }
    Err(ProofError::new(
        "proofloop_budget_exhausted",
        "规划目标和至少一项能力无法在保留输出额度后装入 token 预算",
    ))
}
