use super::*;
use sha2::{Digest, Sha256};
use store::Receipt;
impl SoftwareRuntime {
    pub(super) fn run_operation(
        &self,
        operation_id: &str,
        registration_id: &str,
        session_id: Option<&str>,
        action: &str,
        arguments: serde_json::Value,
        work: impl FnOnce(&Arc<AtomicBool>) -> Result<Session, SoftwareError>,
    ) -> Result<Session, SoftwareError> {
        uuid::Uuid::parse_str(operation_id).map_err(|_| {
            SoftwareError::new(
                "invalid_software_operation",
                "A unique UUID operation_id is required",
            )
        })?;
        let _gate = self.gate.lock();
        self.ready()?;
        let safe_cleanup = action == "release"
            || action == "present"
                && arguments.get("mode").and_then(serde_json::Value::as_str) == Some("independent");
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(registration_id, session_id, action, arguments))
                    .map_err(|e| SoftwareError::new("invalid_software_operation", e.to_string()))?
            )
        );
        let receipt_capacity_reached;
        {
            let state = self.state.lock();
            if let Some(receipt) = state.operations.get(operation_id) {
                if receipt.fingerprint != fingerprint {
                    return Err(SoftwareError::new(
                        "software_operation_conflict",
                        "Operation ID was already used with different arguments",
                    ));
                }
                if receipt.record.state == "verified" {
                    return receipt
                        .record
                        .session_id
                        .as_ref()
                        .and_then(|id| state.sessions.get(id))
                        .cloned()
                        .ok_or_else(|| {
                            SoftwareError::new(
                                "software_session_not_found",
                                "Original session is no longer available",
                            )
                        });
                }
                return Err(SoftwareError::new(
                    &receipt.record.state,
                    receipt
                        .record
                        .reason
                        .as_deref()
                        .unwrap_or("Original operation is pending; it will not be repeated"),
                ));
            }
            receipt_capacity_reached = state.operations.len() >= 4096;
            if receipt_capacity_reached && !safe_cleanup {
                return Err(SoftwareError::new("software_history_limit","Software operation receipt limit reached; old operation IDs are never silently forgotten"));
            }
        }
        let token = {
            let mut pending = self.pending.lock();
            if !pending.contains_key(operation_id) && pending.len() >= 4096 && !safe_cleanup {
                return Err(SoftwareError::new(
                    "software_pending_limit",
                    "Too many cancelled or pending software operation IDs",
                ));
            }
            if pending.len() >= 4096 && !pending.contains_key(operation_id) {
                Arc::new(AtomicBool::new(false))
            } else {
                pending
                    .entry(operation_id.into())
                    .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                    .clone()
            }
        };
        let cancelled = token.load(Ordering::Acquire);
        if receipt_capacity_reached && safe_cleanup {
            if cancelled {
                return Err(SoftwareError::new(
                    "cancelled",
                    "Restoration was cancelled before dispatch",
                ));
            }
            // Keep the full receipt/tombstone set bounded without trapping an
            // externally owned window. This escape path can ONLY restore or
            // release an exact existing target, never launch/read/act/embed.
            self.epoch.fetch_add(1, Ordering::AcqRel);
            self.observations.lock().clear();
            let restored = work(&token);
            self.pending.lock().remove(operation_id);
            return restored;
        }
        let reservation = Receipt {
            fingerprint,
            record: OperationRecord {
                operation_id: operation_id.into(),
                action: action.into(),
                session_id: session_id.map(str::to_owned),
                registration_id: registration_id.into(),
                state: if cancelled { "cancelled" } else { "running" }.into(),
                reason: cancelled.then(|| "Cancelled before dispatch; no automatic replay".into()),
                created_at: chrono::Utc::now().timestamp_millis(),
                window_evidence: None,
            },
        };
        let reserved = self.mutate(|state| {
            state
                .operations
                .insert(operation_id.into(), reservation.clone());
            Ok(())
        });
        if let Err(error) = reserved {
            if safe_cleanup
                && matches!(
                    error.code.as_str(),
                    "software_storage_unavailable" | "stale_software_inventory"
                )
            {
                // A database outage must not trap an externally owned window.
                // This volatile reservation allows ONLY verified restoration.
                self.state
                    .lock()
                    .operations
                    .insert(operation_id.into(), reservation);
            } else {
                self.pending.lock().remove(operation_id);
                return Err(error);
            }
        }
        if cancelled {
            self.pending.lock().remove(operation_id);
            return Err(SoftwareError::new(
                "cancelled",
                "Software operation was cancelled before dispatch",
            ));
        }
        let result = work(&token);
        let cancelled = token.load(Ordering::Acquire);
        let result = if cancelled {
            Err(SoftwareError::unknown("Operation was cancelled during native dispatch; inspect the original window before continuing"))
        } else {
            result
        };
        let persisted = self.mutate(|state| {
            let receipt = state
                .operations
                .get_mut(operation_id)
                .ok_or_else(|| SoftwareError::unknown("Original operation receipt disappeared"))?;
            if receipt.record.state == "result_unknown" {
                return Ok(());
            }
            match &result {
                Ok(session) => {
                    receipt.record.state = "verified".into();
                    receipt.record.session_id = Some(session.id.clone());
                    receipt.record.window_evidence =
                        Some(WindowEvidence::capture(session.generation, &session.window));
                }
                Err(error) => {
                    receipt.record.state = if error.code == "result_unknown" {
                        "result_unknown"
                    } else {
                        "failed"
                    }
                    .into();
                    receipt.record.reason = Some(error.to_string());
                }
            }
            if receipt.record.state == "result_unknown" {
                if let Some(id) = &receipt.record.session_id {
                    if let Some(session) = state.sessions.get_mut(id) {
                        if session.state != "released" {
                            session.state = "result_unknown".into();
                            session.last_error = receipt.record.reason.clone();
                            session.generation = session.generation.saturating_add(1);
                        }
                    }
                }
            }
            Ok(())
        });
        self.pending.lock().remove(operation_id);
        if let Err(error) = persisted {
            // The native action may already have happened. Failure to commit
            // cannot leave an apparently active session that accepts another
            // operation ID. Keep an in-memory barrier even if storage is down.
            self.epoch.fetch_add(1, Ordering::AcqRel);
            self.observations.lock().clear();
            let mut state = self.state.lock();
            let mut affected = session_id.map(str::to_owned);
            if let Ok(session) = &result {
                affected = Some(session.id.clone());
            }
            if let Some(receipt) = state.operations.get_mut(operation_id) {
                receipt.record.state = "result_unknown".into();
                receipt.record.reason =
                    Some(format!("Native completion was not committed: {error}"));
                receipt.record.session_id = affected.clone();
            }
            if let Some(session) = affected.and_then(|id| state.sessions.get_mut(&id)) {
                if session.state != "released" {
                    session.state = "result_unknown".into();
                    session.generation = session.generation.saturating_add(1);
                    session.last_error = Some("Native operation completed but its receipt could not be saved; inspect before continuing".into());
                }
            }
            if safe_cleanup {
                return result
                    .map(|session| state.sessions.get(&session.id).cloned().unwrap_or(session));
            }
            return Err(SoftwareError::unknown(format!(
                "Operation completion could not be committed: {error}"
            )));
        }
        result
    }
    /// May run before a queued worker starts. Keep that cancellation tombstone
    /// until the worker has durably reserved the ID; cancellation never replays.
    pub fn cancel_operation(&self, operation_id: &str) {
        if uuid::Uuid::parse_str(operation_id).is_err() {
            return;
        }
        {
            let mut pending = self.pending.lock();
            if pending.len() >= 4096 && !pending.contains_key(operation_id) {
                self.closing.store(true, Ordering::Release);
                return;
            }
            pending
                .entry(operation_id.into())
                .or_insert_with(|| Arc::new(AtomicBool::new(true)))
                .store(true, Ordering::Release);
        }
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.mutate(|state| {
            if let Some(receipt) = state.operations.get_mut(operation_id) {
                if receipt.record.state == "running" {
                    receipt.record.state = "result_unknown".into();
                    receipt.record.reason = Some(
                        "Cancelled during dispatch; result must be inspected, never replayed"
                            .into(),
                    );
                    if let Some(id) = &receipt.record.session_id {
                        if let Some(session) = state.sessions.get_mut(id) {
                            if session.state != "released" {
                                session.state = "result_unknown".into();
                                session.generation = session.generation.saturating_add(1);
                                session.last_error = receipt.record.reason.clone();
                            }
                        }
                    }
                }
            }
            Ok(())
        });
        self.observations.lock().clear();
    }
    pub(super) fn check_cancel(token: &AtomicBool) -> Result<(), SoftwareError> {
        if token.load(Ordering::Acquire) {
            Err(SoftwareError::new(
                "cancelled",
                "Software operation was cancelled before native dispatch",
            ))
        } else {
            Ok(())
        }
    }
}
