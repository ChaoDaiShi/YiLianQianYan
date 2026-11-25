use super::{problem, Error};
use crate::{
    agent::delegation::ExecutionScope,
    integrations::llm::{
        client::conservative_input_token_upper_bound,
        types::{ChatMessage, Usage},
    },
};
use axum::http::StatusCode;

pub(super) struct BudgetCharge {
    scope: ExecutionScope,
    settled: bool,
}

impl BudgetCharge {
    pub(super) fn begin(
        scope: &ExecutionScope,
        input: &[ChatMessage],
        cap: u32,
    ) -> Result<(u32, Option<Self>), Error> {
        let Some(budget) = scope.budget.as_ref() else {
            return Ok((cap, None));
        };
        let input =
            conservative_input_token_upper_bound(input, &[]).ok_or_else(budget_unavailable)?;
        let handoff = scope.handoff_id.as_deref();
        let cap = budget
            .output_token_limit(handoff, input, cap)
            .map_err(|_| budget_unavailable())?;
        budget
            .begin_call(handoff)
            .map_err(|_| budget_unavailable())?;
        Ok((
            cap,
            Some(Self {
                scope: scope.clone(),
                settled: false,
            }),
        ))
    }

    pub(super) fn finish(mut self, usage: Option<&Usage>) -> Result<(), Error> {
        self.settled = true;
        self.scope
            .budget
            .as_ref()
            .expect("active model charge has a budget")
            .settle_tokens(
                self.scope.handoff_id.as_deref(),
                usage.map(|usage| u64::from(usage.total_tokens)),
            )
            .map_err(|_| budget_unavailable())
    }
}

impl Drop for BudgetCharge {
    fn drop(&mut self) {
        if !self.settled {
            if let Some(budget) = &self.scope.budget {
                let _ = budget.settle_tokens(self.scope.handoff_id.as_deref(), None);
            }
        }
    }
}

pub(super) fn budget_unavailable() -> Error {
    problem(
        StatusCode::CONFLICT,
        "model_budget_unavailable",
        "任务模型调用预算不足或用量待核验",
    )
}
