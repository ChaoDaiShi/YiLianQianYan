//! Budget checks for the existing ProofLoop planning and execution paths.
use super::{GoalContract, PlanningUsage, ProofError};
use crate::{app::state::AppServer, db::Database, modules::task::TaskGraphId};
use std::time::Duration;

pub fn check_goal_budget(
    db: &Database,
    contract: &GoalContract,
    now: i64,
) -> Result<Duration, ProofError> {
    let draft = db
        .confirmed_proof_draft_for_goal(&contract.goal_id, contract.version)
        .map_err(|error| ProofError::new("db_error", error.to_string()))?
        .ok_or_else(|| ProofError::new("goal_graph_not_found", "目标尚未确认"))?;
    let start = if draft.planning_usage.started_at_ms > 0 {
        draft.planning_usage.started_at_ms
    } else {
        db.goal_confirmed_at(&contract.goal_id, contract.version)
            .map_err(|error| ProofError::new("db_error", error.to_string()))?
    };
    check_usage(contract, &draft.planning_usage, start, now)
}

fn check_usage(
    contract: &GoalContract,
    usage: &PlanningUsage,
    start: i64,
    now: i64,
) -> Result<Duration, ProofError> {
    let elapsed = now.saturating_sub(start).max(0) as u64;
    if usage.model_calls > contract.budget.max_model_calls
        || contract.budget.max_total_tokens == 0
        || usage
            .total_tokens
            .is_some_and(|tokens| tokens > contract.budget.max_total_tokens)
        || elapsed >= contract.budget.max_wall_time_ms
    {
        return Err(ProofError::new(
            "proofloop_budget_exhausted",
            "目标的模型调用、token 或耗时预算已耗尽",
        ));
    }
    Ok(Duration::from_millis(
        contract.budget.max_wall_time_ms - elapsed,
    ))
}

pub fn graph_execution_budget(
    server: &AppServer,
    graph_id: &TaskGraphId,
) -> Result<Option<Duration>, ProofError> {
    let Some(draft) = server
        .db
        .confirmed_proof_draft_for_graph(graph_id.as_str())
        .map_err(|error| ProofError::new("db_error", error.to_string()))?
    else {
        return Ok(None);
    };
    let contract = server
        .db
        .load_goal_version(&draft.contract.goal_id, draft.contract.version)
        .map_err(|error| ProofError::new("db_error", error.to_string()))?
        .ok_or_else(|| ProofError::new("goal_not_found", "目标契约不存在"))?;
    check_goal_budget(&server.db, &contract, chrono::Utc::now().timestamp_millis()).map(Some)
}
