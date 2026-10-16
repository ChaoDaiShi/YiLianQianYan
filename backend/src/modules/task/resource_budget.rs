//! Shared reservations for parent and child model calls. Unknown usage remains
//! unknown, and reserved child capacity is unavailable to its parent.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct BudgetReservation {
    pub handoff_id: String,
    pub calls: u32,
    pub tokens: u64,
}

#[derive(Debug, Default)]
struct ReservationUsage {
    remaining_calls: u32,
    remaining_tokens: u64,
    used_calls: u32,
    used_tokens: u64,
    unknown: bool,
}

#[derive(Debug, Default)]
struct Usage {
    calls: u32,
    tokens: u64,
    unknown: bool,
    reservations: HashMap<String, ReservationUsage>,
    active_calls: HashSet<String>,
}

#[derive(Debug)]
pub struct SharedExecutionBudget {
    max_calls: u32,
    max_tokens: u64,
    usage: Mutex<Usage>,
}

impl SharedExecutionBudget {
    pub fn new(max_calls: u32, max_tokens: u64) -> Arc<Self> {
        Arc::new(Self {
            max_calls,
            max_tokens,
            usage: Mutex::new(Usage::default()),
        })
    }

    pub fn reserve(self: &Arc<Self>, request: BudgetReservation) -> Result<BudgetLease, String> {
        let mut usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        if usage.unknown {
            return Err("proofloop_usage_unavailable".into());
        }
        if request.handoff_id.trim().is_empty()
            || usage.reservations.contains_key(&request.handoff_id)
        {
            return Err("重复的任务交接标识".into());
        }

        let reserved_calls: u128 = usage
            .reservations
            .values()
            .map(|reservation| u128::from(reservation.remaining_calls))
            .sum();
        let reserved_tokens: u128 = usage
            .reservations
            .values()
            .map(|reservation| u128::from(reservation.remaining_tokens))
            .sum();
        if u128::from(usage.calls) + reserved_calls + u128::from(request.calls)
            > u128::from(self.max_calls)
            || u128::from(usage.tokens) + reserved_tokens + u128::from(request.tokens)
                > u128::from(self.max_tokens)
        {
            return Err("proofloop_budget_exhausted".into());
        }

        let handoff_id = request.handoff_id;
        usage.reservations.insert(
            handoff_id.clone(),
            ReservationUsage {
                remaining_calls: request.calls,
                remaining_tokens: request.tokens,
                ..ReservationUsage::default()
            },
        );
        Ok(BudgetLease {
            budget: Arc::clone(self),
            handoff_id,
        })
    }

    /// Charge a model call before it starts. Parent calls cannot consume child
    /// reservations, and child calls can consume only their own reservation.
    pub fn begin_call(&self, handoff: Option<&str>) -> Result<(), String> {
        let mut usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        if usage.unknown {
            return Err("proofloop_usage_unavailable".into());
        }
        if let Some(id) = handoff {
            let reservation = usage.reservations.get_mut(id).ok_or("任务预算未预留")?;
            if reservation.remaining_calls == 0 {
                return Err("proofloop_budget_exhausted".into());
            }
            reservation.remaining_calls -= 1;
            reservation.used_calls = reservation
                .used_calls
                .checked_add(1)
                .ok_or("预算计数超出范围")?;
            usage.active_calls.insert(id.to_owned());
        } else {
            let reserved_calls: u128 = usage
                .reservations
                .values()
                .map(|reservation| u128::from(reservation.remaining_calls))
                .sum();
            if u128::from(usage.calls) + reserved_calls >= u128::from(self.max_calls) {
                return Err("proofloop_budget_exhausted".into());
            }
        }
        usage.calls = usage.calls.checked_add(1).ok_or("预算计数超出范围")?;
        Ok(())
    }

    /// Settle the provider-reported token count for a call. A missing or
    /// over-reservation value marks the shared budget unavailable.
    pub fn settle_tokens(&self, handoff: Option<&str>, tokens: Option<u64>) -> Result<(), String> {
        let mut usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        if let Some(id) = handoff {
            usage.active_calls.remove(id);
        }
        let Some(tokens) = tokens else {
            usage.unknown = true;
            if let Some(id) = handoff {
                if let Some(reservation) = usage.reservations.get_mut(id) {
                    reservation.unknown = true;
                }
            }
            return Err("proofloop_usage_unavailable".into());
        };

        if let Some(id) = handoff {
            let remaining = usage
                .reservations
                .get(id)
                .ok_or("任务预算未预留")?
                .remaining_tokens;
            if tokens > remaining {
                usage.unknown = true;
                if let Some(reservation) = usage.reservations.get_mut(id) {
                    reservation.unknown = true;
                }
                return Err("proofloop_budget_exhausted".into());
            }
            let reservation = usage.reservations.get_mut(id).ok_or("任务预算未预留")?;
            reservation.remaining_tokens -= tokens;
            reservation.used_tokens = reservation
                .used_tokens
                .checked_add(tokens)
                .ok_or("预算计数超出范围")?;
        }

        let reserved_tokens: u128 = usage
            .reservations
            .values()
            .map(|reservation| u128::from(reservation.remaining_tokens))
            .sum();
        let total = u128::from(usage.tokens) + reserved_tokens + u128::from(tokens);
        if total > u128::from(self.max_tokens) {
            usage.unknown = true;
            if let Some(id) = handoff {
                if let Some(reservation) = usage.reservations.get_mut(id) {
                    reservation.unknown = true;
                }
            }
            return Err("proofloop_budget_exhausted".into());
        }
        usage.tokens = usage.tokens.checked_add(tokens).ok_or("预算计数超出范围")?;
        Ok(())
    }

    /// Compatibility helper for callers that report a completed parent call.
    pub fn observe_parent_usage(&self, tokens: Option<u64>) -> Result<(), String> {
        let mut usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        match usage.calls.checked_add(1) {
            Some(calls) => usage.calls = calls,
            None => usage.unknown = true,
        }
        match tokens {
            Some(value) => match usage.tokens.checked_add(value) {
                Some(tokens) => usage.tokens = tokens,
                None => usage.unknown = true,
            },
            None => usage.unknown = true,
        }
        Ok(())
    }

    /// Mark shared model billing unavailable after an external provider did
    /// work without reporting cumulative billed tokens. This does not invent a
    /// call count or token value.
    pub fn mark_usage_unavailable(&self, handoff: Option<&str>) -> Result<(), String> {
        let mut usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        usage.unknown = true;
        if let Some(id) = handoff {
            let reservation = usage.reservations.get_mut(id).ok_or("任务预算未预留")?;
            reservation.unknown = true;
        }
        Ok(())
    }

    pub fn available(&self) -> Result<(u32, u64), String> {
        let usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        if usage.unknown {
            return Err("proofloop_usage_unavailable".into());
        }
        let reserved_calls: u128 = usage
            .reservations
            .values()
            .map(|reservation| u128::from(reservation.remaining_calls))
            .sum();
        let reserved_tokens: u128 = usage
            .reservations
            .values()
            .map(|reservation| u128::from(reservation.remaining_tokens))
            .sum();
        Ok((
            u128::from(self.max_calls)
                .saturating_sub(u128::from(usage.calls) + reserved_calls)
                .min(u128::from(u32::MAX)) as u32,
            u128::from(self.max_tokens)
                .saturating_sub(u128::from(usage.tokens) + reserved_tokens)
                .min(u128::from(u64::MAX)) as u64,
        ))
    }

    /// Cap a strict call's output after its conservative prompt-input estimate.
    /// Child calls can spend only their own reservation; parent calls leave all
    /// child reservations unavailable. Provider usage is still settled after
    /// completion and remains authoritative.
    pub fn output_token_limit(
        &self,
        handoff: Option<&str>,
        input_upper_bound: u64,
        configured_output_limit: u32,
    ) -> Result<u32, String> {
        let usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        if usage.unknown {
            return Err("proofloop_usage_unavailable".into());
        }
        let remaining = if let Some(id) = handoff {
            let reservation = usage.reservations.get(id).ok_or("任务预算未预留")?;
            if reservation.unknown || usage.active_calls.contains(id) {
                return Err("proofloop_usage_unavailable".into());
            }
            reservation.remaining_tokens
        } else {
            let reserved: u128 = usage
                .reservations
                .values()
                .map(|reservation| u128::from(reservation.remaining_tokens))
                .sum();
            u128::from(self.max_tokens)
                .saturating_sub(u128::from(usage.tokens) + reserved)
                .min(u128::from(u64::MAX)) as u64
        };
        let output_limit = remaining
            .checked_sub(input_upper_bound)
            .ok_or("proofloop_budget_exhausted")?
            .min(u64::from(configured_output_limit));
        if output_limit == 0 {
            return Err("proofloop_budget_exhausted".into());
        }
        Ok(output_limit.min(u64::from(u32::MAX)) as u32)
    }

    pub fn usage(&self) -> Result<(u32, Option<u64>), String> {
        let usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        Ok((
            usage.calls,
            if usage.unknown {
                None
            } else {
                Some(usage.tokens)
            },
        ))
    }

    /// Return usage for this handoff only, independent of parent or sibling
    /// calls that share the same execution budget.
    pub fn handoff_usage(&self, handoff_id: &str) -> Result<(u32, Option<u64>), String> {
        let usage = self.usage.lock().map_err(|_| "资源预算不可用")?;
        let reservation = usage.reservations.get(handoff_id).ok_or("任务预算未预留")?;
        Ok((
            reservation.used_calls,
            if reservation.unknown || usage.active_calls.contains(handoff_id) {
                None
            } else {
                Some(reservation.used_tokens)
            },
        ))
    }
}

pub struct BudgetLease {
    budget: Arc<SharedExecutionBudget>,
    pub handoff_id: String,
}

impl Drop for BudgetLease {
    fn drop(&mut self) {
        if let Ok(mut usage) = self.budget.usage.lock() {
            let active = usage.active_calls.remove(&self.handoff_id);
            if active {
                usage.unknown = true;
            }
            usage.reservations.remove(&self.handoff_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_reservations_cannot_spend_same_tokens() {
        let budget = SharedExecutionBudget::new(6, 1_000);
        let first = budget
            .reserve(BudgetReservation {
                handoff_id: "a".into(),
                calls: 2,
                tokens: 800,
            })
            .unwrap();
        assert!(budget
            .reserve(BudgetReservation {
                handoff_id: "b".into(),
                calls: 2,
                tokens: 800,
            })
            .is_err());
        drop(first);
        assert!(budget
            .reserve(BudgetReservation {
                handoff_id: "b".into(),
                calls: 2,
                tokens: 800,
            })
            .is_ok());
    }

    #[test]
    fn missing_usage_blocks_further_budgeted_calls() {
        let budget = SharedExecutionBudget::new(6, 1_000);
        budget.begin_call(None).unwrap();
        assert!(budget.settle_tokens(None, None).is_err());
        assert!(budget.begin_call(None).is_err());
        assert_eq!(budget.usage().unwrap(), (1, None));
    }

    #[test]
    fn observation_only_parent_usage_does_not_gate_later_chat_calls() {
        let budget = SharedExecutionBudget::new(6, 1_000);
        budget.observe_parent_usage(None).unwrap();
        budget.observe_parent_usage(Some(100)).unwrap();

        assert_eq!(budget.usage().unwrap(), (2, None));
        assert!(budget.available().is_err());
        assert!(budget
            .reserve(BudgetReservation {
                handoff_id: "child".into(),
                calls: 1,
                tokens: 100,
            })
            .is_err());
    }

    #[test]
    fn parent_calls_respect_calls_reserved_for_a_child() {
        let budget = SharedExecutionBudget::new(3, 1_000);
        let _child = budget
            .reserve(BudgetReservation {
                handoff_id: "child".into(),
                calls: 2,
                tokens: 800,
            })
            .unwrap();

        budget.begin_call(None).unwrap();
        budget.settle_tokens(None, Some(100)).unwrap();
        assert!(budget.begin_call(None).is_err());
        assert_eq!(budget.usage().unwrap(), (1, Some(100)));
    }

    #[test]
    fn parent_usage_cannot_be_charged_against_child_token_reservation() {
        let budget = SharedExecutionBudget::new(6, 1_000);
        let _child = budget
            .reserve(BudgetReservation {
                handoff_id: "child".into(),
                calls: 2,
                tokens: 800,
            })
            .unwrap();

        budget.begin_call(None).unwrap();
        assert!(budget.settle_tokens(None, Some(201)).is_err());
        assert_eq!(budget.usage().unwrap(), (1, None));
        assert!(budget.begin_call(None).is_err());
    }

    #[test]
    fn child_usage_is_scoped_to_its_handoff() {
        let budget = SharedExecutionBudget::new(6, 1_000);
        let _first = budget
            .reserve(BudgetReservation {
                handoff_id: "first".into(),
                calls: 2,
                tokens: 400,
            })
            .unwrap();
        let _second = budget
            .reserve(BudgetReservation {
                handoff_id: "second".into(),
                calls: 2,
                tokens: 400,
            })
            .unwrap();

        budget.begin_call(Some("first")).unwrap();
        budget.settle_tokens(Some("first"), Some(100)).unwrap();
        budget.begin_call(Some("second")).unwrap();
        budget.settle_tokens(Some("second"), Some(200)).unwrap();

        assert_eq!(budget.handoff_usage("first").unwrap(), (1, Some(100)));
        assert_eq!(budget.handoff_usage("second").unwrap(), (1, Some(200)));
    }

    #[test]
    fn strict_call_caps_output_after_prompt_inside_its_reservation() {
        let budget = SharedExecutionBudget::new(6, 1_000);
        let _lease = budget
            .reserve(BudgetReservation {
                handoff_id: "child".into(),
                calls: 2,
                tokens: 400,
            })
            .unwrap();

        assert_eq!(
            budget
                .output_token_limit(Some("child"), 250, 1_000)
                .unwrap(),
            150
        );
        assert!(budget
            .output_token_limit(Some("child"), 400, 1_000)
            .is_err());
        assert_eq!(budget.output_token_limit(None, 500, 1_000).unwrap(), 100);
    }
}
