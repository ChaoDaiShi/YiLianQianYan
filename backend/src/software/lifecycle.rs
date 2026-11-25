use super::*;
use management::same_executable;
impl SoftwareRuntime {
    fn attach_candidate(
        &self,
        registration: &Registration,
        candidate: WindowCandidate,
        launched: bool,
        token: &AtomicBool,
    ) -> Result<Session, SoftwareError> {
        if !same_executable(
            &registration.config.executable,
            &candidate.identity.executable,
        ) {
            return Err(SoftwareError::new(
                "software_target_mismatch",
                "Candidate executable differs from the registered software",
            ));
        }
        Self::check_cancel(token)?;
        let window = self.host()?.verify(&candidate.identity)?;
        self.mutate(|state| {
            if state.sessions.len() >= 256 {
                return Err(SoftwareError::new(
                    "software_session_limit",
                    "Software session history limit reached",
                ));
            }
            if state.sessions.values().any(|s| {
                s.window.identity == window.identity
                    && !matches!(s.state.as_str(), "released" | "interrupted" | "lost")
            }) {
                return Err(SoftwareError::new(
                    "software_window_attached",
                    "This window already belongs to another software session",
                ));
            }
            if state
                .registrations
                .get(&registration.id)
                .map(|r| r.revision)
                != Some(registration.revision)
            {
                return Err(SoftwareError::new(
                    "stale_software_revision",
                    "Software registration changed before attachment",
                ));
            }
            let session = Session {
                id: uuid::Uuid::new_v4().to_string(),
                registration_id: registration.id.clone(),
                generation: 1,
                window,
                state: if token.load(Ordering::Acquire) {
                    "result_unknown"
                } else {
                    "active"
                }
                .into(),
                launched_by_host: launched,
                last_error: None,
            };
            state.sessions.insert(session.id.clone(), session.clone());
            Ok(session)
        })
    }
    pub fn attach(
        &self,
        id: &str,
        identity: WindowIdentity,
        operation_id: &str,
    ) -> Result<Session, SoftwareError> {
        let registration = self.registration(id)?;
        self.run_operation(
            operation_id,
            id,
            None,
            "attach",
            serde_json::json!(identity),
            |token| {
                Self::check_cancel(token)?;
                if self.registration(id)?.revision != registration.revision {
                    return Err(SoftwareError::new(
                        "stale_software_revision",
                        "Registration changed before attachment",
                    ));
                }
                let candidates = self.host()?.discover(&registration)?;
                let candidate = candidates
                    .into_iter()
                    .find(|c| c.identity == identity)
                    .ok_or_else(|| {
                        SoftwareError::new(
                            "stale_software_candidate",
                            "Selected window is no longer an exact candidate; discover again",
                        )
                    })?;
                self.attach_candidate(&registration, candidate, false, token)
            },
        )
    }
    pub fn launch(&self, id: &str, operation_id: &str) -> Result<Session, SoftwareError> {
        let registration = self.registration(id)?;
        self.launch_confirmed(
            id,
            registration.revision,
            &registration.config.executable,
            operation_id,
        )
    }
    pub fn launch_confirmed(
        &self,
        id: &str,
        revision: u64,
        expected_executable: &str,
        operation_id: &str,
    ) -> Result<Session, SoftwareError> {
        let registration = self.registration(id)?;
        self.run_operation(operation_id,id,None,"launch",serde_json::json!({"revision":revision,"expected_executable":expected_executable}),|token|{
            Self::check_cancel(token)?;
            let current=self.registration(id)?;
            if current.revision!=revision||current.config.executable!=expected_executable||current!=registration{return Err(SoftwareError::new("stale_software_launch","The registered executable changed after confirmation; inspect the new launch details"))}
            if self.state.lock().operations.values().any(|r|r.record.registration_id==id&&r.record.action=="launch"&&r.record.state=="result_unknown"){return Err(SoftwareError::new("software_launch_blocked","An earlier launch has an unknown outcome; this request did not start another process. Discover and inspect the original windows before another launch"))}
            let host=self.host()?;
            if !host.discover(&registration)?.is_empty(){return Err(SoftwareError::new("software_existing_windows","Existing windows were found; select a specific window instead of starting another process"))}
            Self::check_cancel(token)?;
            let candidate=host.launch(&registration)?;
            if token.load(Ordering::Acquire){return Err(SoftwareError::unknown("Launch may have succeeded after cancellation; discover windows rather than launching again"))}
            self.attach_candidate(&registration,candidate,true,token)
        })
    }
    pub fn present(
        &self,
        target: SessionTarget,
        mode: &str,
        viewport: Option<Viewport>,
        operation_id: &str,
    ) -> Result<Session, SoftwareError> {
        if !matches!(mode, "embedded" | "independent" | "focus") {
            return Err(SoftwareError::new(
                "invalid_software_presentation",
                "Choose embedded or independent presentation",
            ));
        }
        if let Some(rect) = viewport {
            if rect.width == 0
                || rect.height == 0
                || rect.width > 8192
                || rect.height > 8192
                || rect.x.unsigned_abs() > 32768
                || rect.y.unsigned_abs() > 32768
            {
                return Err(SoftwareError::new(
                    "invalid_software_viewport",
                    "Software viewport is out of bounds",
                ));
            }
        } else if mode == "embedded" {
            return Err(SoftwareError::new(
                "invalid_software_viewport",
                "Embedded mode requires a current host viewport",
            ));
        }
        let registration_id = self
            .state
            .lock()
            .sessions
            .get(&target.session_id)
            .map(|s| s.registration_id.clone())
            .ok_or_else(|| {
                SoftwareError::new(
                    "software_session_not_found",
                    "Software session does not exist",
                )
            })?;
        self.run_operation(
            operation_id,
            &registration_id,
            Some(&target.session_id),
            "present",
            serde_json::json!({"target":target,"mode":mode,"viewport":viewport}),
            |token| {
                let session = if mode == "independent" {
                    self.cleanup_session(&target)?
                } else {
                    self.session(&target)?.0
                };
                if session.state == "result_unknown" && mode == "embedded" {
                    return Err(SoftwareError::new(
                        "result_unknown",
                        "Inspect the existing window before embedding again",
                    ));
                }
                Self::check_cancel(token)?;
                let window = self
                    .host()?
                    .present(&session.window.identity, mode, viewport)?;
                self.replace_window(&target, window, None, mode == "independent", token)
            },
        )
    }
    pub(super) fn replace_window(
        &self,
        target: &SessionTarget,
        window: WindowState,
        next_state: Option<&str>,
        safe_cleanup: bool,
        token: &AtomicBool,
    ) -> Result<Session, SoftwareError> {
        self.observations.lock().remove(&target.session_id);
        let recovery_window = window.clone();
        let saved = self.mutate(|state| {
            let session = state
                .sessions
                .get_mut(&target.session_id)
                .ok_or_else(|| SoftwareError::unknown("Session disappeared during dispatch"))?;
            if session.generation != target.generation || session.state == "released" {
                return Err(SoftwareError::unknown(
                    "Session changed during native dispatch; late completion cannot restore it",
                ));
            }
            session.window = window;
            session.generation = session.generation.saturating_add(1);
            if let Some(status) = next_state {
                session.state = status.into()
            }
            if token.load(Ordering::Acquire) {
                session.state = "result_unknown".into();
                session.last_error = Some(
                    "Native completion arrived after cancellation; inspect original window".into(),
                );
            }
            Ok(session.clone())
        });
        if safe_cleanup
            && saved.as_ref().err().is_some_and(|error| {
                matches!(
                    error.code.as_str(),
                    "software_storage_unavailable" | "stale_software_inventory"
                )
            })
        {
            self.epoch.fetch_add(1, Ordering::AcqRel);
            let mut state = self.state.lock();
            let session = state.sessions.get_mut(&target.session_id).ok_or_else(|| {
                SoftwareError::unknown("Restored window no longer has a session record")
            })?;
            if session.generation != target.generation || session.state == "released" {
                return Err(SoftwareError::unknown(
                    "Restoration completed after session ownership changed",
                ));
            }
            session.window = recovery_window;
            session.generation = session.generation.saturating_add(1);
            session.state = if next_state == Some("released") {
                "released"
            } else {
                "result_unknown"
            }
            .into();
            session.last_error=Some("Native window restoration succeeded, but its record could not be persisted. No process was terminated; inspect before continuing".into());
            return Ok(session.clone());
        }
        saved
    }
    pub fn release(
        &self,
        target: SessionTarget,
        operation_id: &str,
    ) -> Result<Session, SoftwareError> {
        let registration_id = self
            .state
            .lock()
            .sessions
            .get(&target.session_id)
            .map(|s| s.registration_id.clone())
            .ok_or_else(|| {
                SoftwareError::new(
                    "software_session_not_found",
                    "Software session does not exist",
                )
            })?;
        self.run_operation(
            operation_id,
            &registration_id,
            Some(&target.session_id),
            "release",
            serde_json::json!(target),
            |token| {
                let session = self.cleanup_session(&target)?;
                Self::check_cancel(token)?;
                let host = self.host()?;
                if let Err(error) = host.release(&session.window.identity) {
                    if !matches!(error.code.as_str(), "window_closed" | "target_changed") {
                        return Err(error);
                    }
                    // Never modify a replacement HWND after the original closed.
                }
                let mut window = session.window;
                window.mode = "independent".into();
                self.replace_window(&target, window, Some("released"), true, token)
            },
        )
    }
    fn pending_for_session(&self, id: &str) -> bool {
        let pending = self.pending.lock();
        self.state.lock().operations.values().any(|r| {
            r.record.session_id.as_deref() == Some(id)
                && pending.contains_key(&r.record.operation_id)
        })
    }
    pub fn stop(&self, target: SessionTarget) -> Result<Session, SoftwareError> {
        self.cleanup_session(&target)?;
        let operations: Vec<String> = self
            .state
            .lock()
            .operations
            .values()
            .filter(|r| {
                r.record.session_id.as_deref() == Some(&target.session_id)
                    && r.record.state == "running"
            })
            .map(|r| r.record.operation_id.clone())
            .collect();
        for operation in operations {
            self.cancel_operation(&operation)
        }
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.observations.lock().remove(&target.session_id);
        self.mutate(|state| {
            let session = state.sessions.get_mut(&target.session_id).ok_or_else(|| {
                SoftwareError::new("software_session_not_found", "Session disappeared")
            })?;
            if !matches!(
                session.state.as_str(),
                "released" | "result_unknown" | "lost"
            ) {
                session.state = "stopped".into();
            }
            session.generation = session.generation.saturating_add(1);
            Ok(session.clone())
        })
    }
    pub fn resume(&self, target: SessionTarget) -> Result<Session, SoftwareError> {
        let (session, _) = self.session(&target)?;
        if session.state != "stopped" {
            return Err(SoftwareError::new(
                "software_resume_refused",
                "Only an explicitly stopped, inspected session can resume",
            ));
        }
        if self.pending_for_session(&target.session_id) {
            return Err(SoftwareError::new(
                "software_operation_pending",
                "Wait for the original native operation to finish before resuming",
            ));
        }
        let window = self.host()?.verify(&session.window.identity)?;
        self.replace_window(
            &target,
            window,
            Some("active"),
            false,
            &AtomicBool::new(false),
        )
    }
    pub fn acknowledge_unknown(&self, target: SessionTarget) -> Result<Session, SoftwareError> {
        let session = self.cleanup_session(&target)?;
        if session.state != "result_unknown" {
            return Err(SoftwareError::new(
                "software_acknowledge_refused",
                "No unknown operation is awaiting inspection",
            ));
        }
        if self.pending_for_session(&target.session_id) {
            return Err(SoftwareError::new(
                "software_operation_pending",
                "Original native operation is still running; acknowledgement is not yet safe",
            ));
        }
        self.observations.lock().remove(&target.session_id);
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.mutate(|state|{
            let session=state.sessions.get_mut(&target.session_id).ok_or_else(||SoftwareError::new("software_session_not_found","Session disappeared"))?;
            if session.generation!=target.generation{return Err(SoftwareError::new("stale_software_target","Refresh the session before acknowledgement"))}
            session.state="stopped".into();session.generation=session.generation.saturating_add(1);session.last_error=Some("User inspected the unknown result; original action was not replayed. Resume explicitly to continue".into());Ok(session.clone())
        })
    }
    pub fn acknowledge_launch(&self, id: &str, operation_id: &str) -> Result<(), SoftwareError> {
        self.ready()?;
        if self.pending.lock().contains_key(operation_id) {
            return Err(SoftwareError::new(
                "software_operation_pending",
                "The original launch is still running",
            ));
        }
        self.mutate(|state|{
            let receipt=state.operations.get_mut(operation_id).ok_or_else(||SoftwareError::new("software_operation_not_found","Original launch receipt does not exist"))?;
            if receipt.record.registration_id!=id||receipt.record.action!="launch"||!matches!(receipt.record.state.as_str(),"result_unknown"|"acknowledged_unknown"){return Err(SoftwareError::new("software_acknowledge_refused","Only the selected unknown launch can be acknowledged"))}
            receipt.record.state="acknowledged_unknown".into();
            receipt.record.reason=Some("User inspected the launch result. No launch was replayed; discover and attach existing windows explicitly".into());Ok(())
        })?;
        self.epoch.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    pub fn refresh_sessions(&self) -> Result<Vec<Session>, SoftwareError> {
        if !self.available() {
            return self.sessions();
        }
        let host = self.host()?;
        for session in self
            .sessions()?
            .into_iter()
            .filter(|s| !matches!(s.state.as_str(), "released" | "interrupted" | "lost"))
        {
            let actual = host.verify(&session.window.identity);
            self.mutate(|state| {
                if let Some(current) = state.sessions.get_mut(&session.id) {
                    if current.generation == session.generation {
                        match actual {
                            Ok(window) => current.window = window,
                            Err(error) => {
                                current.state = "lost".into();
                                current.generation = current.generation.saturating_add(1);
                                current.last_error = Some(error.to_string());
                            }
                        }
                    }
                }
                Ok(())
            })?;
        }
        self.sessions()
    }
    pub fn shutdown(&self) -> Result<(), SoftwareError> {
        if self.closing.swap(true, Ordering::AcqRel) {
            return self.host.read().shutdown();
        }
        self.stop_monitors();
        self.epoch.fetch_add(1, Ordering::AcqRel);
        for token in self.pending.lock().values() {
            token.store(true, Ordering::Release)
        }
        self.observations.lock().clear();
        let result = self.host.read().shutdown();
        let mut state = self.state.lock();
        let mut next = state.clone();
        for session in next.sessions.values_mut().filter(|s| s.state != "released") {
            session.state = "interrupted".into();
            session.generation = session.generation.saturating_add(1);
            session.window.mode = "independent_unverified".into();
            session.last_error = Some(
                "Host exited; re-discover and attach explicitly. Processes were not terminated"
                    .into(),
            );
        }
        for receipt in next
            .operations
            .values_mut()
            .filter(|r| r.record.state == "running")
        {
            receipt.record.state = "result_unknown".into();
            receipt.record.reason =
                Some("Host exited during dispatch; original operation must not be replayed".into());
        }
        store::save(&self.db, state.revision, &mut next)?;
        *state = next;
        result
    }
}
