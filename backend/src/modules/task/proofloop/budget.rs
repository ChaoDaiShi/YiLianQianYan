//! One activity clock per goal. Idle review and approval gaps never start a lease.
use super::{
    BudgetTimeBasis, GoalContract, GoalTimeUsage, PlanningUsage, ProofDraft, ProofError,
    RepairProposal,
};
use crate::{
    app::state::AppServer,
    db::{Database, ProofLoopTimeRecord},
    modules::task::{NodeExecutionId, NodeExecutionStatus, TaskGraphId},
};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

type GoalKey = (String, u32);
struct Activity {
    started: Instant,
    count: u32,
    unpersisted_finishes: u32,
}

pub struct GoalBudgetMeter {
    db: Database,
    running: Mutex<HashMap<GoalKey, Activity>>,
}

pub struct BudgetLease {
    meter: Arc<GoalBudgetMeter>,
    key: Option<GoalKey>,
    remaining: Duration,
}

impl BudgetLease {
    pub fn remaining(&self) -> Duration {
        self.remaining
    }
    pub fn finish(mut self) -> Result<(), ProofError> {
        self.meter
            .finish(self.key.take().expect("live budget lease"))
    }
}
impl Drop for BudgetLease {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            if self.meter.finish(key).is_err() {
                tracing::error!("ProofLoop activity accounting could not be saved");
            }
        }
    }
}

fn db_error(error: impl std::fmt::Display) -> ProofError {
    ProofError::new("db_error", error.to_string())
}
fn unknown() -> ProofError {
    ProofError::new(
        "proofloop_usage_unavailable",
        "历史执行活动时间尚未核实，预算未重置",
    )
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn remaining_budget(
    contract: &GoalContract,
    planning: &PlanningUsage,
    usage: &GoalTimeUsage,
    confirmed_at: i64,
    now: i64,
) -> Result<Duration, ProofError> {
    if usage.accounting_state == "interrupted_unknown" {
        return Err(unknown());
    }
    // Legacy planning is explicitly reported unknown; its first future activity
    // is bounded without claiming that the old planning cost was measured.
    let elapsed = match contract.budget.time_basis {
        BudgetTimeBasis::ActiveWork => usage
            .execution_active_ms
            .saturating_add(usage.planning_elapsed_ms.unwrap_or(0)),
        BudgetTimeBasis::WallClock => now
            .saturating_sub(if planning.started_at_ms > 0 {
                planning.started_at_ms
            } else {
                confirmed_at
            })
            .max(0) as u64,
    };
    if planning.model_calls > contract.budget.max_model_calls
        || contract.budget.max_total_tokens == 0
        || planning
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

impl GoalBudgetMeter {
    pub fn new(db: Database) -> Result<Self, ProofError> {
        db.recover_proofloop_time().map_err(db_error)?;
        Ok(Self {
            db,
            running: Mutex::new(HashMap::new()),
        })
    }

    fn draft(&self, contract: &GoalContract) -> Result<ProofDraft, ProofError> {
        self.db
            .confirmed_proof_draft_for_goal(&contract.goal_id, contract.version)
            .map_err(db_error)?
            .ok_or_else(|| ProofError::new("goal_graph_not_found", "目标尚未确认"))
    }

    fn record(&self, draft: &ProofDraft) -> Result<ProofLoopTimeRecord, ProofError> {
        if let Some(record) = self
            .db
            .load_proofloop_time(&draft.contract.goal_id, draft.contract.version)
            .map_err(db_error)?
        {
            return Ok(record);
        }
        if draft.contract.budget.time_basis == BudgetTimeBasis::ActiveWork
            && draft.planning_usage.elapsed_ms.is_none()
            && self
                .db
                .proofloop_has_execution_history(&draft.graph_id)
                .map_err(db_error)?
        {
            return Err(unknown());
        }
        let confirmed = self
            .db
            .goal_confirmed_at(&draft.contract.goal_id, draft.contract.version)
            .map_err(db_error)?;
        let usage = GoalTimeUsage {
            planning_elapsed_ms: draft.planning_usage.elapsed_ms,
            execution_active_ms: 0,
            human_wait_ms: draft.planning_usage.elapsed_ms.map(|elapsed| {
                (confirmed
                    .saturating_sub(draft.planning_usage.started_at_ms)
                    .max(0) as u64)
                    .saturating_sub(elapsed)
            }),
            accounting_state: if draft.planning_usage.elapsed_ms.is_some() {
                "complete"
            } else {
                "legacy_unknown"
            }
            .into(),
        };
        let record = ProofLoopTimeRecord {
            usage,
            active_count: 0,
            active_started_at_ms: None,
        };
        self.db
            .save_proofloop_time(
                &draft.contract.goal_id,
                draft.contract.version,
                &record,
                now(),
            )
            .map_err(db_error)?;
        Ok(record)
    }

    fn current_usage(
        &self,
        draft: &ProofDraft,
        running: &HashMap<GoalKey, Activity>,
    ) -> Result<GoalTimeUsage, ProofError> {
        let record = self.record(draft)?;
        let mut usage = record.usage;
        if let Some(active) = running.get(&(draft.contract.goal_id.clone(), draft.contract.version))
        {
            if active.unpersisted_finishes > 0 || record.active_count != active.count {
                return Err(unknown());
            }
            usage.execution_active_ms = usage
                .execution_active_ms
                .saturating_add(active.started.elapsed().as_millis().min(u64::MAX as u128) as u64);
        } else if record.active_count > 0 {
            return Err(unknown());
        }
        Ok(usage)
    }

    pub fn usage(&self, contract: &GoalContract) -> Result<GoalTimeUsage, ProofError> {
        let running = self.running.lock();
        self.current_usage(&self.draft(contract)?, &running)
    }

    pub fn check(&self, contract: &GoalContract, at: i64) -> Result<Duration, ProofError> {
        let running = self.running.lock();
        let draft = self.draft(contract)?;
        let usage = self.current_usage(&draft, &running)?;
        remaining_budget(
            contract,
            &draft.planning_usage,
            &usage,
            self.db
                .goal_confirmed_at(&contract.goal_id, contract.version)
                .map_err(db_error)?,
            at,
        )
    }

    pub fn save_repair(
        &self,
        proposal: &mut RepairProposal,
        contract: &GoalContract,
        at: i64,
    ) -> Result<(), ProofError> {
        let running = self.running.lock();
        let draft = self.draft(contract)?;
        let mut usage = self.current_usage(&draft, &running)?;
        let active_started = running
            .get(&(contract.goal_id.clone(), contract.version))
            .map(|activity| activity.started);
        if active_started.is_some() {
            usage = self.record(&draft)?.usage;
        }
        self.db
            .save_repair_proposal_with_budget(proposal, contract, &usage, active_started, at)
            .map_err(|error| {
                let code = if error.to_string().contains("proofloop_budget_exhausted") {
                    "proofloop_budget_exhausted"
                } else {
                    "db_error"
                };
                ProofError::new(code, error.to_string())
            })
    }

    fn begin(
        self: &Arc<Self>,
        draft: &ProofDraft,
        execution_id: &NodeExecutionId,
    ) -> Result<BudgetLease, ProofError> {
        let mut running = self.running.lock();
        let key = (draft.contract.goal_id.clone(), draft.contract.version);
        let usage = self.current_usage(draft, &running)?;
        let remaining = remaining_budget(
            &draft.contract,
            &draft.planning_usage,
            &usage,
            self.db.goal_confirmed_at(&key.0, key.1).map_err(db_error)?,
            now(),
        )?;
        let mut record = self.record(draft)?;
        if let Some(execution) = self
            .db
            .load_node_execution(execution_id)
            .map_err(db_error)?
        {
            if execution.status == NodeExecutionStatus::WaitingApproval {
                record.usage.human_wait_ms = record.usage.human_wait_ms.map(|value| {
                    value.saturating_add(now().saturating_sub(execution.updated_at).max(0) as u64)
                });
            }
        }
        let count = running.get(&key).map_or(1, |activity| activity.count + 1);
        record.active_count = count;
        if count == 1 {
            record.active_started_at_ms = Some(now());
        }
        self.db
            .save_proofloop_time(&key.0, key.1, &record, now())
            .map_err(db_error)?;
        running
            .entry(key.clone())
            .and_modify(|activity| activity.count = count)
            .or_insert(Activity {
                started: Instant::now(),
                count,
                unpersisted_finishes: 0,
            });
        Ok(BudgetLease {
            meter: Arc::clone(self),
            key: Some(key),
            remaining,
        })
    }

    fn finish(&self, key: GoalKey) -> Result<(), ProofError> {
        let mut running = self.running.lock();
        let Some(activity) = running.get(&key) else {
            return Err(unknown());
        };
        let persisted_count = activity.count;
        let unpersisted_finishes = activity.unpersisted_finishes;
        let started = activity.started;
        let mut record = match self.db.load_proofloop_time(&key.0, key.1) {
            Ok(Some(record)) => record,
            Ok(None) => {
                if let Some(activity) = running.get_mut(&key) {
                    activity.unpersisted_finishes += 1;
                }
                return Err(unknown());
            }
            Err(error) => {
                if let Some(activity) = running.get_mut(&key) {
                    activity.unpersisted_finishes += 1;
                }
                return Err(db_error(error));
            }
        };
        if record.active_count != persisted_count {
            if let Some(activity) = running.get_mut(&key) {
                activity.unpersisted_finishes += 1;
            }
            return Err(unknown());
        }
        let count = persisted_count - unpersisted_finishes - 1;
        record.active_count = count;
        if count == 0 {
            let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            record.usage.execution_active_ms =
                record.usage.execution_active_ms.saturating_add(elapsed);
            record.active_started_at_ms = None;
        }
        if let Err(error) = self.db.save_proofloop_time(&key.0, key.1, &record, now()) {
            if let Some(activity) = running.get_mut(&key) {
                activity.unpersisted_finishes += 1;
            }
            return Err(db_error(error));
        }
        if count == 0 {
            running.remove(&key);
        } else if let Some(activity) = running.get_mut(&key) {
            activity.count = count;
            activity.unpersisted_finishes = 0;
        }
        Ok(())
    }
}

pub fn graph_execution_budget(
    server: &AppServer,
    graph_id: &TaskGraphId,
) -> Result<Option<Duration>, ProofError> {
    let Some(draft) = server
        .db
        .confirmed_proof_draft_for_graph(graph_id.as_str())
        .map_err(db_error)?
    else {
        return Ok(None);
    };
    server
        .proofloop_budget
        .check(&draft.contract, now())
        .map(Some)
}

pub fn begin_goal_execution(
    server: &AppServer,
    graph_id: &TaskGraphId,
    execution_id: &NodeExecutionId,
) -> Result<Option<BudgetLease>, ProofError> {
    let Some(draft) = server
        .db
        .confirmed_proof_draft_for_graph(graph_id.as_str())
        .map_err(db_error)?
    else {
        return Ok(None);
    };
    server
        .proofloop_budget
        .begin(&draft, execution_id)
        .map(Some)
}

#[cfg(test)]
mod tests;
