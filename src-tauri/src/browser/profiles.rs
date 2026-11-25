//! Persistent browsing shares one process/proxy per account and workspace.
//! Explicit local-origin grants belong to that shared context, not a single tab.
use super::*;

pub(super) fn webview_directory(profile: PathBuf) -> PathBuf {
    #[cfg(windows)]
    if let Some(std::path::Component::Prefix(prefix)) = profile.components().next() {
        if let std::path::Prefix::VerbatimDisk(drive) = prefix.kind() {
            let mut directory = PathBuf::from(format!("{}:", drive as char));
            directory.extend(profile.components().skip(1));
            return directory;
        }
    }
    profile
}

pub(super) struct BrowserProcessLifetime {
    active: Mutex<std::collections::HashSet<u32>>,
    changed: tokio::sync::watch::Sender<usize>,
}

impl Default for BrowserProcessLifetime {
    fn default() -> Self {
        Self {
            active: Mutex::new(std::collections::HashSet::new()),
            changed: tokio::sync::watch::channel(0).0,
        }
    }
}

impl BrowserProcessLifetime {
    /// A shared environment registers one exit handler for each real process.
    pub(super) fn started(&self, pid: u32) -> bool {
        let mut active = self.active.lock().unwrap();
        let added = active.insert(pid);
        if added {
            self.changed.send_replace(active.len());
        }
        added
    }

    pub(super) fn exited(&self, pid: u32) {
        let mut active = self.active.lock().unwrap();
        if active.remove(&pid) {
            self.changed.send_replace(active.len());
        }
    }

    pub(super) async fn wait_stopped(&self) -> Result<(), BrowserError> {
        let mut changed = self.changed.subscribe();
        loop {
            if *changed.borrow_and_update() == 0 {
                return Ok(());
            }
            changed.changed().await.map_err(|_| {
                error(
                    "native_shutdown_failed",
                    "Browser process exit notification was lost",
                )
            })?;
        }
    }
}

pub(super) struct BrowserContext {
    pub(super) profile: PathBuf,
    pub(super) proxy: Arc<network::BrowserProxy>,
    pub(super) ephemeral: bool,
    pub(super) process: Arc<BrowserProcessLifetime>,
}

impl Drop for BrowserContext {
    fn drop(&mut self) {
        self.proxy.shutdown();
        if self.ephemeral {
            let profile = self.profile.clone();
            // The final content lease is released after controller disposal.
            // Native browser files can remain briefly locked; cleanup stays
            // restricted to the existing owned temporary-directory checker.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                let _ = remove_temporary_profile(&profile);
            });
        }
    }
}

fn persistent_requested(request: &BrowserRequest) -> Result<bool, BrowserError> {
    // AI tools do not gain persistent website sessions through a payload flag.
    if request.owner.is_some() || request.payload.get("_source_tool_call_id").is_some() {
        return Ok(false);
    }
    match request.payload.get("ephemeral") {
        None | Some(Value::Bool(true)) => Ok(false),
        Some(Value::Bool(false)) => Ok(true),
        Some(_) => Err(error(
            "invalid_mode",
            "Browser ephemeral mode must be a boolean",
        )),
    }
}

async fn acquire_persistent_context(
    pool: &tokio::sync::Mutex<HashMap<PathBuf, Arc<BrowserContext>>>,
    profile: PathBuf,
    url: &tauri::Url,
    forbidden: Vec<u16>,
    closing: &AtomicBool,
) -> Result<Arc<BrowserContext>, BrowserError> {
    let mut contexts = pool.lock().await;
    if closing.load(Ordering::Acquire) {
        return Err(error(
            "permission_denied",
            "Browser sessions were revoked; restart the application",
        ));
    }
    let context = match contexts.get(&profile) {
        Some(context) => context.clone(),
        None => {
            let proxy = Arc::new(
                network::start(vec![url.origin().ascii_serialization()], forbidden)
                    .await
                    .map_err(|reason| error("blocked_origin", reason))?,
            );
            std::fs::create_dir_all(&profile).map_err(|_| {
                error(
                    "native_profile_failed",
                    "Persistent browser profile could not be created",
                )
            })?;
            let context = Arc::new(BrowserContext {
                profile: profile.clone(),
                proxy,
                ephemeral: false,
                process: Arc::new(BrowserProcessLifetime::default()),
            });
            contexts.insert(profile, context.clone());
            context
        }
    };
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    if host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok() {
        context
            .proxy
            .authorize_local_origin(url.as_str())
            .map_err(|reason| error("blocked_origin", reason))?;
    }
    Ok(context)
}

impl NativeBrowserHost {
    pub(super) async fn acquire_browser_profile(
        &self,
        request: &BrowserRequest,
        url: &tauri::Url,
        forbidden: Vec<u16>,
        session_id: &str,
    ) -> Result<Arc<BrowserContext>, BrowserError> {
        if !persistent_requested(request)? {
            let proxy = Arc::new(
                network::start(vec![url.origin().ascii_serialization()], forbidden)
                    .await
                    .map_err(|reason| error("blocked_origin", reason))?,
            );
            let profile = std::env::temp_dir().join(format!("yilian-browser-{session_id}"));
            std::fs::create_dir(&profile).map_err(|_| {
                error(
                    "native_profile_failed",
                    "Temporary browser profile could not be created",
                )
            })?;
            return Ok(Arc::new(BrowserContext {
                profile,
                proxy,
                ephemeral: true,
                process: Arc::new(BrowserProcessLifetime::default()),
            }));
        }
        let trusted = self.profile_context.as_ref().ok_or_else(|| {
            error(
                "browser_profile_unavailable",
                "The active account profile is unavailable for persistent browsing",
            )
        })?;
        let workspace = request.payload.get("workspace_id").and_then(Value::as_str);
        let profile = trusted.directory_for_workspace(workspace);
        let context = acquire_persistent_context(
            &self.persistent_contexts,
            profile,
            url,
            forbidden,
            &self.closing,
        )
        .await?;
        Ok(context)
    }

    pub(super) async fn shutdown_persistent_contexts(&self) {
        for context in self.persistent_contexts.lock().await.values() {
            context.proxy.shutdown();
        }
    }

    pub(super) async fn wait_persistent_processes(&self) -> Result<(), BrowserError> {
        let processes: Vec<_> = self
            .persistent_contexts
            .lock()
            .await
            .values()
            .map(|context| context.process.clone())
            .collect();
        tokio::time::timeout(Duration::from_secs(4), async {
            for process in processes {
                process.wait_stopped().await?;
            }
            Ok(())
        })
        .await
        .map_err(|_| {
            error(
                "native_shutdown_timeout",
                "Persistent browser processes did not confirm exit before the shutdown deadline",
            )
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn browser_process_lifetime_waits_for_last_exit_without_losing_early_exit() {
        use std::future::Future;
        use std::task::Poll;
        let process = BrowserProcessLifetime::default();
        assert!(process.started(41));
        assert!(!process.started(41));
        assert!(process.started(42));
        let stopped = process.wait_stopped();
        tokio::pin!(stopped);
        std::future::poll_fn(|cx| {
            assert!(stopped.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        process.exited(41);
        std::future::poll_fn(|cx| {
            assert!(stopped.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        process.exited(42);
        stopped.await.unwrap();
        process.exited(42);
        process.wait_stopped().await.unwrap();
        assert!(process.started(43));
        process.exited(43);
        process.wait_stopped().await.unwrap();
    }

    fn request(payload: Value) -> BrowserRequest {
        BrowserRequest {
            operation_id: "profile-mode-test".into(),
            action: BrowserAction::Open,
            target: None,
            payload,
            owner: None,
        }
    }

    #[test]
    fn only_explicit_manual_open_uses_persistent_login() {
        assert!(!persistent_requested(&request(json!({}))).unwrap());
        assert!(!persistent_requested(&request(json!({"ephemeral":true}))).unwrap());
        assert!(persistent_requested(&request(json!({"ephemeral":false}))).unwrap());
        assert!(!persistent_requested(&request(
            json!({"ephemeral":false,"_source_tool_call_id":"tool-1"})
        ))
        .unwrap());
        assert!(persistent_requested(&request(json!({"ephemeral":"false"}))).is_err());
    }

    #[tokio::test]
    async fn temporary_popup_lease_keeps_parent_proxy_until_last_controller_releases() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let profile = std::env::temp_dir().join(format!(
            "yilian-browser-browser-session-lease-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&profile).unwrap();
        let proxy = Arc::new(network::start(vec![], vec![]).await.unwrap());
        let parent = Arc::new(BrowserContext {
            profile: profile.clone(),
            proxy: proxy.clone(),
            ephemeral: true,
            process: Arc::new(BrowserProcessLifetime::default()),
        });
        let popup = parent.clone();
        drop(parent);
        assert!(profile.is_dir());
        assert!(
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, proxy.port()))
                .await
                .is_ok()
        );
        drop(popup);
        tokio::time::timeout(Duration::from_secs(4), async {
            let stopped = async {
                loop {
                    if tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, proxy.port()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            };
            let removed = async {
                while profile.exists() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            };
            // Windows can take two seconds to report a refused connection. Both independent
            // completion conditions share the same deadline instead of waiting in series.
            tokio::join!(stopped, removed);
        })
        .await
        .expect("Final temporary lease must stop its proxy and clean its owned UDF");
    }

    #[tokio::test]
    async fn persistent_contexts_share_proxy_and_explicit_local_grants_only_within_scope() {
        let pool = tokio::sync::Mutex::new(HashMap::new());
        let closing = AtomicBool::new(false);
        let root = std::env::temp_dir().join(format!(
            "browser-context-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first_url = tauri::Url::parse("http://localhost:41001").unwrap();
        let next_url = tauri::Url::parse("http://localhost:41002").unwrap();
        let first = acquire_persistent_context(
            &pool,
            root.join("account-a/project-a"),
            &first_url,
            vec![],
            &closing,
        )
        .await
        .unwrap();
        let second = acquire_persistent_context(
            &pool,
            root.join("account-a/project-a"),
            &next_url,
            vec![],
            &closing,
        )
        .await
        .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.proxy.browser_args(), second.proxy.browser_args());
        assert!(first.proxy.allows(next_url.as_str()));
        assert!(second.proxy.allows(first_url.as_str()));
        assert!(!second.proxy.allows("http://localhost:9420"));
        let other = acquire_persistent_context(
            &pool,
            root.join("account-a/project-b"),
            &next_url,
            vec![],
            &closing,
        )
        .await
        .unwrap();
        assert!(!Arc::ptr_eq(&second, &other));
        assert!(!other.proxy.allows(first_url.as_str()));
        let account = acquire_persistent_context(
            &pool,
            root.join("account-b/project-a"),
            &next_url,
            vec![],
            &closing,
        )
        .await
        .unwrap();
        assert!(!account.proxy.allows(first_url.as_str()));
        drop(first);
        assert!(tokio::net::TcpStream::connect((
            std::net::Ipv4Addr::LOCALHOST,
            second.proxy.port()
        ))
        .await
        .is_ok());
        closing.store(true, Ordering::Release);
        assert!(
            acquire_persistent_context(&pool, root.join("new"), &next_url, vec![], &closing)
                .await
                .is_err()
        );
        for context in pool.lock().await.values() {
            context.proxy.shutdown();
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
