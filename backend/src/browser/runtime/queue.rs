use super::*;
use tokio::sync::{OwnedMutexGuard, OwnedSemaphorePermit};

pub(super) struct OperationGuard {
    operations: Arc<Mutex<HashMap<String, Operation>>>,
    request: BrowserRequest,
    pub(super) cancel: CancellationToken,
    pub(super) dispatched: bool,
    complete: bool,
    in_flight: Arc<Mutex<HashMap<String, InFlight>>>,
    mutation_sequence: Option<Arc<tokio::sync::Mutex<()>>>,
    // Owned by the receipt guard so caller Drop settles the ledger before either
    // resource is released and a queued mutation can enter the native host.
    sequence_permit: Option<OwnedMutexGuard<()>>,
    slot_permit: Option<OwnedSemaphorePermit>,
}

impl BrowserRuntime {
    pub(super) fn check_unknown_fence(&self, request: &BrowserRequest) -> Result<(), BrowserError> {
        if request.action.requires_replay_barrier()
            && self.operations.lock().values().any(|op| {
                op.unknown
                    && !op.acknowledged
                    && same_tab(op.request.target.as_ref(), request.target.as_ref())
            })
        {
            return Err(BrowserError::new(
                "unknown_requires_review",
                "An earlier action on this tab has an uncertain result; explicit human review is required",
            ));
        }
        Ok(())
    }
}

impl OperationGuard {
    /// Caller holds lifecycle_gate through operation acceptance and registration.
    pub(super) fn register(
        runtime: &BrowserRuntime,
        request: &BrowserRequest,
        cancel: CancellationToken,
        agent: bool,
    ) -> Self {
        let mut pending = runtime.in_flight.lock();
        if matches!(request.action, BrowserAction::Stop | BrowserAction::Close) {
            for operation in pending.values() {
                if same_tab(operation.target.as_ref(), request.target.as_ref()) {
                    operation.cancel.cancel();
                }
            }
        }
        // Open has no existing tab to sequence. Reads and controls remain
        // independent so review and emergency actions can still observe/stop it.
        let mutation_sequence =
            (request.target.is_some() && request.action.requires_replay_barrier()).then(|| {
                pending
                    .values()
                    .find_map(|operation| {
                        same_tab(operation.target.as_ref(), request.target.as_ref())
                            .then(|| operation.mutation_sequence.clone())
                            .flatten()
                    })
                    .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(())))
            });
        pending.insert(
            request.operation_id.clone(),
            InFlight {
                target: request.target.clone(),
                cancel: cancel.clone(),
                agent,
                mutation_sequence: mutation_sequence.clone(),
            },
        );
        Self {
            operations: runtime.operations.clone(),
            request: request.clone(),
            cancel,
            dispatched: false,
            complete: false,
            in_flight: runtime.in_flight.clone(),
            mutation_sequence,
            sequence_permit: None,
            slot_permit: None,
        }
    }

    pub(super) async fn wait_for_dispatch(
        &mut self,
        runtime: &BrowserRuntime,
        authorization_changes: &mut tokio::sync::watch::Receiver<u64>,
        agent: bool,
    ) -> Result<(), BrowserError> {
        let cancelled = || {
            BrowserError::new(
                "cancelled_before_execution",
                "Cancelled while waiting for browser dispatch",
            )
        };
        let revoked = || {
            BrowserError::new(
                "permission_revoked_before_execution",
                "Browser authorization changed while queued",
            )
        };
        if let Some(sequence) = self.mutation_sequence.clone() {
            self.sequence_permit = Some(tokio::select! { biased;
                _ = authorization_changes.changed(), if agent => return Err(revoked()),
                _ = self.cancel.cancelled() => return Err(cancelled()),
                permit = sequence.lock_owned() => permit,
            });
            runtime.check_unknown_fence(&self.request)?;
        }
        if !is_control(&self.request.action) {
            self.slot_permit = Some(tokio::select! { biased;
                _ = authorization_changes.changed(), if agent => return Err(revoked()),
                _ = self.cancel.cancelled() => return Err(cancelled()),
                permit = runtime.slots.clone().acquire_owned() => permit.map_err(|_| BrowserError::new("runtime_closed", "Browser runtime is closed"))?,
            });
        }
        Ok(())
    }

    pub(super) fn finish(&mut self, result: Result<BrowserReply, BrowserError>) {
        if let Some(op) = self.operations.lock().get_mut(&self.request.operation_id) {
            op.unknown = result
                .as_ref()
                .is_ok_and(|reply| reply.state == "result_unknown");
            op.result = Some(result);
        }
        self.in_flight.lock().remove(&self.request.operation_id);
        self.complete = true;
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.cancel.cancel();
        if !self.complete {
            let result = if self.dispatched {
                interrupted(
                    &self.request,
                    "Caller ended before the native result was known",
                )
            } else {
                Err(BrowserError::new(
                    "cancelled_before_execution",
                    "Caller ended before dispatch",
                ))
            };
            self.finish(result);
        }
        // Rust drops sequence/slot permits only after this receipt settlement.
    }
}
