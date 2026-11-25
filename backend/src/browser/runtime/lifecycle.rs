use super::*;

impl BrowserRuntime {
    pub fn new() -> Self {
        Self {
            host: RwLock::new(None),
            profile_context: std::sync::OnceLock::new(),
            credential_vault: std::sync::OnceLock::new(),
            local_fixture_bundled_skills: None,
            quiesced: AtomicBool::new(false),
            authorization_epoch: AtomicU64::new(0),
            authorization_changes: tokio::sync::watch::channel(0).0,
            lifecycle_gate: Mutex::new(()),
            shutdown_gate: tokio::sync::Mutex::new(()),
            operations: Arc::new(Mutex::new(HashMap::new())),
            slots: Arc::new(Semaphore::new(4)),
            timeout: Duration::from_secs(20),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            evidence: RwLock::new(None),
        }
    }
    pub(crate) fn configure_profile_context(
        &self,
        db_path: &std::path::Path,
    ) -> Result<(), BrowserError> {
        let context = BrowserProfileContext::from_database_path(db_path)?;
        match self.profile_context.set(context.clone()) {
            Ok(()) => Ok(()),
            Err(_) if self.profile_context.get() == Some(&context) => Ok(()),
            Err(_) => Err(BrowserError::new(
                "browser_profile_immutable",
                "Browser account profile cannot change without restarting",
            )),
        }
    }
    pub fn profile_context(&self) -> Option<&BrowserProfileContext> {
        self.profile_context.get()
    }
    /// Explicit, narrowing-only fixture construction. No environment or HOME
    /// override is consulted; the native debug fixture supplies this trusted root.
    pub fn for_local_fixture(bundled_skills_root: std::path::PathBuf) -> Self {
        let mut runtime = Self::new();
        runtime.local_fixture_bundled_skills = Some(bundled_skills_root);
        runtime
    }
    pub fn local_fixture_bundled_skills(&self) -> Option<&std::path::Path> {
        self.local_fixture_bundled_skills.as_deref()
    }
    pub fn install_host(&self, host: Arc<dyn BrowserHost>) -> Result<(), BrowserError> {
        let _lifecycle = self.lifecycle_gate.lock();
        if self.is_quiesced() {
            return Err(BrowserError::new(
                "browser_quiesced",
                "Browser sessions were revoked; restart is required",
            ));
        }
        let mut current = self.host.write();
        if current.is_some() {
            return Err(BrowserError::new(
                "host_already_installed",
                "Native browser host is already installed",
            ));
        }
        *current = Some(host);
        Ok(())
    }
    pub fn available(&self) -> bool {
        !self.is_quiesced() && self.host.read().is_some()
    }
    pub fn tabs(&self) -> Vec<BrowserTab> {
        if self.is_quiesced() {
            return vec![];
        }
        let tabs = self
            .host
            .read()
            .as_ref()
            .map(|host| host.tabs())
            .unwrap_or_default();
        if self.is_quiesced() {
            vec![]
        } else {
            tabs
        }
    }
    pub fn authorization_epoch(&self) -> u64 {
        self.authorization_epoch.load(Ordering::SeqCst)
    }
    /// Revoke pending agent authorizations without interrupting human browsing.
    /// Old queued requests cannot adopt the new epoch after a permissions edit.
    pub fn invalidate_authorizations(&self) -> u64 {
        let _lifecycle = self.lifecycle_gate.lock();
        let epoch = self
            .authorization_epoch
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        for pending in self.in_flight.lock().values() {
            if pending.agent {
                pending.cancel.cancel();
            }
        }
        self.authorization_changes.send_replace(epoch);
        epoch
    }
    pub fn is_quiesced(&self) -> bool {
        self.quiesced.load(Ordering::SeqCst)
    }
    /// A profile transition is one-way until process restart. Cancellation alone
    /// does not prove a native view stopped; activation waits for host shutdown.
    pub async fn quiesce(&self, timeout: Duration) -> Result<(), BrowserError> {
        {
            let _lifecycle = self.lifecycle_gate.lock();
            self.quiesced.store(true, Ordering::SeqCst);
            for pending in self.in_flight.lock().values() {
                pending.cancel.cancel();
            }
        }
        let result = tokio::time::timeout(timeout, async {
            let _shutdown = self.shutdown_gate.lock().await;
            let host = self.host.read().clone();
            if let Some(host) = host {
                host.shutdown().await?;
                if !host.tabs().is_empty() {
                    return Err(BrowserError::new(
                        "native_shutdown_incomplete",
                        "Native browser still retains active views",
                    ));
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| {
            BrowserError::new(
                "native_quiesce_timeout",
                "Native browser did not confirm shutdown before the profile transition deadline",
            )
        })?;
        result
    }
}
