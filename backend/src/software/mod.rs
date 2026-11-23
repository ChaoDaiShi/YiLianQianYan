//! Project-scoped desktop software sessions. Neither registrations nor host
//! correlation confer Gateway authority, process ownership, or message access.
mod actions;
mod evidence;
mod lifecycle;
mod management;
pub mod model;
pub(crate) mod monitor;
mod operations;
mod store;
use crate::db::Database;
pub use model::*;
use parking_lot::{Mutex, RwLock};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};
use store::Inventory;

struct UnavailableHost;
impl SoftwareHost for UnavailableHost {
    fn available(&self) -> bool {
        false
    }
    fn discover(&self, _: &Registration) -> Result<Vec<WindowCandidate>, SoftwareError> {
        Err(unavailable())
    }
    fn launch(&self, _: &Registration) -> Result<WindowCandidate, SoftwareError> {
        Err(unavailable())
    }
    fn verify(&self, _: &WindowIdentity) -> Result<WindowState, SoftwareError> {
        Err(unavailable())
    }
    fn present(
        &self,
        _: &WindowIdentity,
        _: &str,
        _: Option<Viewport>,
    ) -> Result<WindowState, SoftwareError> {
        Err(unavailable())
    }
    fn release(&self, _: &WindowIdentity) -> Result<(), SoftwareError> {
        Err(unavailable())
    }
    fn observe(
        &self,
        _: &WindowIdentity,
        _: &str,
        _: bool,
    ) -> Result<Vec<SoftwareControl>, SoftwareError> {
        Err(unavailable())
    }
    fn control(
        &self,
        _: &WindowIdentity,
        _: &SoftwareControl,
        _: &str,
        _: Option<&str>,
        _: &AtomicBool,
    ) -> Result<(), SoftwareError> {
        Err(unavailable())
    }
    fn shutdown(&self) -> Result<(), SoftwareError> {
        Ok(())
    }
}
fn unavailable() -> SoftwareError {
    SoftwareError::new(
        "software_host_unavailable",
        "Desktop software sessions require the Windows native host",
    )
}
pub struct SoftwareRuntime {
    db: Database,
    state: Mutex<Inventory>,
    host: RwLock<Arc<dyn SoftwareHost>>,
    gate: Mutex<()>,
    pending: Mutex<HashMap<String, Arc<AtomicBool>>>,
    observations: Mutex<HashMap<String, Observation>>,
    revoked_workspaces: RwLock<std::collections::HashSet<String>>,
    epoch: AtomicU64,
    closing: AtomicBool,
    monitors: Mutex<HashMap<String, monitor::MonitorEntry>>,
    init_error: Option<SoftwareError>,
}
impl SoftwareRuntime {
    pub fn new(db: Database) -> Self {
        let loaded = store::load(&db);
        let init_error = loaded.as_ref().err().cloned();
        let mut state = loaded.unwrap_or_default();
        let before = state.revision;
        let mut changed = false;
        for session in state
            .sessions
            .values_mut()
            .filter(|s| s.state != "released")
        {
            session.state = "interrupted".into();
            session.generation = session.generation.saturating_add(1);
            session.window.mode = "independent_unverified".into();
            session.last_error =
                Some("Application restarted; no window was reattached or action replayed".into());
            changed = true;
        }
        for receipt in state
            .operations
            .values_mut()
            .filter(|r| r.record.state == "running")
        {
            receipt.record.state = "result_unknown".into();
            receipt.record.reason = Some(
                "Host exited during operation; inspect the original window before retrying".into(),
            );
            changed = true;
        }
        let recovery_error = if changed {
            store::save(&db, before, &mut state).err()
        } else {
            None
        };
        Self {
            db,
            state: Mutex::new(state),
            host: RwLock::new(Arc::new(UnavailableHost)),
            gate: Mutex::new(()),
            pending: Mutex::new(HashMap::new()),
            observations: Mutex::new(HashMap::new()),
            revoked_workspaces: RwLock::new(std::collections::HashSet::new()),
            epoch: AtomicU64::new(1),
            closing: AtomicBool::new(false),
            monitors: Mutex::new(HashMap::new()),
            init_error: init_error.or(recovery_error),
        }
    }
    pub fn install_host(&self, host: Arc<dyn SoftwareHost>) {
        *self.host.write() = host;
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }
    pub fn available(&self) -> bool {
        self.init_error.is_none()
            && !self.closing.load(Ordering::Acquire)
            && self.host.read().available()
    }
    pub fn authorization_epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }
    fn ready(&self) -> Result<(), SoftwareError> {
        if let Some(error) = &self.init_error {
            return Err(error.clone());
        }
        if self.closing.load(Ordering::Acquire) {
            return Err(SoftwareError::new(
                "software_host_closing",
                "Software host is shutting down",
            ));
        }
        Ok(())
    }
    fn mutate<T>(
        &self,
        change: impl FnOnce(&mut Inventory) -> Result<T, SoftwareError>,
    ) -> Result<T, SoftwareError> {
        self.ready()?;
        let mut current = self.state.lock();
        let mut next = current.clone();
        let value = change(&mut next)?;
        store::save(&self.db, current.revision, &mut next)?;
        *current = next;
        Ok(value)
    }
    fn host(&self) -> Result<Arc<dyn SoftwareHost>, SoftwareError> {
        self.ready()?;
        let host = Arc::clone(&self.host.read());
        if !host.available() {
            return Err(unavailable());
        }
        Ok(host)
    }
    pub fn registrations(&self) -> Result<Vec<Registration>, SoftwareError> {
        self.ready()?;
        Ok(self.state.lock().registrations.values().cloned().collect())
    }
    pub fn sessions(&self) -> Result<Vec<Session>, SoftwareError> {
        self.ready()?;
        Ok(self.state.lock().sessions.values().cloned().collect())
    }
    pub fn history(&self) -> Result<Vec<OperationRecord>, SoftwareError> {
        self.ready()?;
        let mut records: Vec<_> = self
            .state
            .lock()
            .operations
            .values()
            .map(|r| r.record.clone())
            .collect();
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        records.truncate(200);
        Ok(records)
    }
    fn registration(&self, id: &str) -> Result<Registration, SoftwareError> {
        self.ready()?;
        let registration = self
            .state
            .lock()
            .registrations
            .get(id)
            .cloned()
            .ok_or_else(|| {
                SoftwareError::new("software_not_found", "Software registration does not exist")
            })?;
        self.workspace_ready(registration.config.workspace_id.as_deref())?;
        Ok(registration)
    }
    fn workspace_ready(&self, id: Option<&str>) -> Result<(), SoftwareError> {
        if id.is_some_and(|id| self.revoked_workspaces.read().contains(id)) {
            return Err(SoftwareError::new("software_workspace_revoked","Project access was revoked in this process; restore/release existing windows and review the project before attaching again"));
        }
        store::workspace(&self.db, id)
    }
    fn session(&self, target: &SessionTarget) -> Result<(Session, Registration), SoftwareError> {
        let session = self.cleanup_session(target)?;
        if session.state == "lost" {
            return Err(SoftwareError::new(
                "software_session_inactive",
                "The original window is gone or changed; only release is allowed",
            ));
        }
        let registration = self.registration(&session.registration_id)?;
        Ok((session, registration))
    }
    // Restoring/releasing an already-owned bookmark cannot depend on a project
    // remaining active. This does not authorize observations or new UI actions.
    fn cleanup_session(&self, target: &SessionTarget) -> Result<Session, SoftwareError> {
        self.ready()?;
        let session = self
            .state
            .lock()
            .sessions
            .get(&target.session_id)
            .cloned()
            .ok_or_else(|| {
                SoftwareError::new(
                    "software_session_not_found",
                    "Software session does not exist",
                )
            })?;
        if session.generation != target.generation {
            return Err(SoftwareError::new(
                "stale_software_target",
                "Session generation changed; refresh and observe again",
            ));
        }
        if matches!(session.state.as_str(), "released" | "interrupted") {
            return Err(SoftwareError::new(
                "software_session_inactive",
                "Choose the current window and attach a new session",
            ));
        }
        Ok(session)
    }
    pub fn binding(&self, target: &SessionTarget) -> Result<String, SoftwareError> {
        let (session, registration) = self.session(target)?;
        Ok(format!(
            "software:{}:{}:{}:{}:{}",
            registration.id,
            registration.revision,
            session.id,
            session.generation,
            self.authorization_epoch()
        ))
    }
}
#[cfg(test)]
mod tests;
