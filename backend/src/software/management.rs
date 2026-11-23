use super::*;
fn validate(input: &mut RegistrationInput) -> Result<(), SoftwareError> {
    input.name = input.name.trim().to_string();
    if input.name.is_empty()
        || input.name.chars().count() > 120
        || input.name.chars().any(char::is_control)
    {
        return Err(SoftwareError::new(
            "invalid_software_name",
            "Software name must contain 1-120 printable characters",
        ));
    }
    if !matches!(
        input.adapter.as_str(),
        "wechat" | "generic" | "generic_fixture"
    ) {
        return Err(SoftwareError::new(
            "unsupported_software_adapter",
            "Choose the WeChat or generic Windows application adapter",
        ));
    }
    let path = std::path::Path::new(&input.executable);
    if input.executable.len() > 2048
        || input.executable.contains('\0')
        || !path.is_absolute()
        || !path.is_file()
    {
        return Err(SoftwareError::new(
            "invalid_software_executable",
            "Choose an existing absolute executable file",
        ));
    }
    if cfg!(windows) && !input.executable.to_ascii_lowercase().ends_with(".exe") {
        return Err(SoftwareError::new(
            "invalid_software_executable",
            "Windows software must be launched directly from an EXE",
        ));
    }
    if input.adapter == "wechat"
        && !path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            n.eq_ignore_ascii_case("WeChat.exe") || n.eq_ignore_ascii_case("Weixin.exe")
        })
    {
        return Err(SoftwareError::new(
            "invalid_wechat_executable",
            "WeChat adapter requires WeChat.exe or Weixin.exe",
        ));
    }
    input.executable = path
        .canonicalize()
        .map_err(|e| SoftwareError::new("invalid_software_executable", e.to_string()))?
        .to_string_lossy()
        .into_owned();
    if (input.allow_actions || input.allow_content || input.allow_window_ocr)
        && !input.allow_observe
    {
        return Err(SoftwareError::new(
            "invalid_software_permissions",
            "UI actions require explicit observation permission as well",
        ));
    }
    Ok(())
}
impl SoftwareRuntime {
    /// Called BEFORE committing a project archive. Cancellation evidence must
    /// reach native last-dispatch checks even if persisting the stop later fails.
    pub fn revoke_workspace(&self, workspace_id: &str) -> Result<(), SoftwareError> {
        self.ready()?;
        // Latch before collecting sessions: concurrent creation/attachment must
        // not enter the gap between cancellation and the archive transaction.
        self.revoked_workspaces.write().insert(workspace_id.into());
        let registrations: std::collections::HashSet<String> = self
            .state
            .lock()
            .registrations
            .values()
            .filter(|r| r.config.workspace_id.as_deref() == Some(workspace_id))
            .map(|r| r.id.clone())
            .collect();
        let operations: Vec<String> = self
            .state
            .lock()
            .operations
            .values()
            .filter(|r| {
                registrations.contains(&r.record.registration_id) && r.record.state == "running"
            })
            .map(|r| r.record.operation_id.clone())
            .collect();
        // Signal EVERY pending call before any fallible database operation.
        {
            let pending = self.pending.lock();
            for operation in &operations {
                if let Some(token) = pending.get(operation) {
                    token.store(true, Ordering::Release);
                }
            }
        }
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.observations.lock().clear();
        for operation in operations {
            self.cancel_operation(&operation)
        }
        let saved=self.mutate(|state|{
            for session in state.sessions.values_mut().filter(|s|registrations.contains(&s.registration_id)&&!matches!(s.state.as_str(),"released"|"interrupted"|"lost")){
                session.generation=session.generation.saturating_add(1);
                if session.state!="result_unknown"{session.state="stopped".into();}
                session.last_error=Some("Project access was revoked; native automation was cancelled before project archival".into());
            }Ok(())
        });
        if saved.is_err() {
            let mut state = self.state.lock();
            for session in state.sessions.values_mut().filter(|s| {
                registrations.contains(&s.registration_id)
                    && !matches!(s.state.as_str(), "released" | "interrupted" | "lost")
            }) {
                session.generation = session.generation.saturating_add(1);
                if session.state != "result_unknown" {
                    session.state = "stopped".into();
                }
                session.last_error=Some("Project revocation could not be persisted; automation remains stopped in memory".into());
            }
        }
        saved
    }
    pub fn register(&self, mut config: RegistrationInput) -> Result<Registration, SoftwareError> {
        validate(&mut config)?;
        self.workspace_ready(config.workspace_id.as_deref())?;
        self.mutate(|state| {
            if state.registrations.len() >= 64 {
                return Err(SoftwareError::new(
                    "software_registration_limit",
                    "At most 64 software registrations are supported",
                ));
            }
            let registration = Registration {
                id: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                config,
            };
            state
                .registrations
                .insert(registration.id.clone(), registration.clone());
            Ok(registration)
        })
    }
    pub fn update(
        &self,
        id: &str,
        revision: u64,
        mut config: RegistrationInput,
    ) -> Result<Registration, SoftwareError> {
        validate(&mut config)?;
        self.workspace_ready(config.workspace_id.as_deref())?;
        let updated=self.mutate(|state|{
            let previous=state.registrations.get(id).cloned().ok_or_else(||SoftwareError::new("software_not_found","Software registration does not exist"))?;
            if previous.revision!=revision{return Err(SoftwareError::new("stale_software_revision","Software registration changed; refresh before saving"))}
            let active=state.sessions.values().any(|s|s.registration_id==id&&!matches!(s.state.as_str(),"released"|"interrupted"|"lost"))||state.operations.values().any(|r|r.record.registration_id==id&&r.record.state=="running");
            let mut restrictive=previous.config.clone();restrictive.allow_observe=config.allow_observe;restrictive.allow_actions=config.allow_actions;restrictive.allow_content=config.allow_content;restrictive.allow_window_ocr=config.allow_window_ocr;
            let shrinking=restrictive==config&&(!config.allow_window_ocr||previous.config.allow_window_ocr)&&(!config.allow_content||previous.config.allow_content)&&(!config.allow_observe||previous.config.allow_observe)&&(!config.allow_actions||previous.config.allow_actions);
            if active&&!shrinking{return Err(SoftwareError::new("software_session_active","Release active sessions before changing the executable, project or expanding permissions"))}
            let registration=Registration{id:id.into(),revision:revision.checked_add(1).ok_or_else(||SoftwareError::new("software_revision_overflow","Registration revision overflow"))?,config};
            state.registrations.insert(id.into(),registration.clone());
            for session in state.sessions.values_mut().filter(|s|s.registration_id==id&&s.state!="released"){
                session.generation=session.generation.saturating_add(1);
                if active&&!matches!(session.state.as_str(),"result_unknown"|"interrupted"|"lost"){session.state="stopped".into();session.last_error=Some("Automation stopped because software permissions changed".into());}
            }Ok(registration)
        })?;
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.observations.lock().clear();
        let operations: Vec<String> = self
            .state
            .lock()
            .operations
            .values()
            .filter(|r| r.record.registration_id == id && r.record.state == "running")
            .map(|r| r.record.operation_id.clone())
            .collect();
        for operation in operations {
            self.cancel_operation(&operation)
        }
        Ok(updated)
    }
    pub fn remove(&self, id: &str, revision: u64) -> Result<(), SoftwareError> {
        self.mutate(|state| {
            let registration = state.registrations.get(id).ok_or_else(|| {
                SoftwareError::new("software_not_found", "Software registration does not exist")
            })?;
            if registration.revision != revision {
                return Err(SoftwareError::new(
                    "stale_software_revision",
                    "Refresh before removing software",
                ));
            }
            if state.sessions.values().any(|s| {
                s.registration_id == id
                    && !matches!(s.state.as_str(), "released" | "interrupted" | "lost")
            }) {
                return Err(SoftwareError::new(
                    "software_session_active",
                    "Restore and release all software sessions before removing this registration",
                ));
            }
            state.registrations.remove(id);
            Ok(())
        })?;
        self.epoch.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    pub fn discover(&self, id: &str) -> Result<Vec<WindowCandidate>, SoftwareError> {
        let registration = self.registration(id)?;
        let candidates = self.host()?.discover(&registration)?;
        Ok(candidates
            .into_iter()
            .filter(|c| same_executable(&c.identity.executable, &registration.config.executable))
            .take(64)
            .collect())
    }
}
pub(super) fn same_executable(a: &str, b: &str) -> bool {
    let normalize = |v: &str| {
        std::path::Path::new(v)
            .canonicalize()
            .ok()
            .map(|p| p.to_string_lossy().to_ascii_lowercase())
    };
    matches!((normalize(a),normalize(b)),(Some(a),Some(b)) if a==b)
}
