use super::*;

struct ClosingHost {
    tabs: parking_lot::Mutex<Vec<BrowserTab>>,
    calls: AtomicUsize,
    closed: AtomicUsize,
    release: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl BrowserHost for ClosingHost {
    fn tabs(&self) -> Vec<BrowserTab> {
        self.tabs.lock().clone()
    }
    async fn dispatch(
        &self,
        _request: BrowserRequest,
        _cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.release.notified().await;
        Ok(BrowserReply::verified(
            json!({"tab":tab(),"text":"old profile data"}),
        ))
    }
    async fn shutdown(&self) -> Result<(), BrowserError> {
        self.closed.fetch_add(1, Ordering::SeqCst);
        self.tabs.lock().clear();
        self.release.notify_waiters();
        Ok(())
    }
}
#[tokio::test]
async fn visible_runtime_quiesce_revokes_inflight_read_and_navigation_races() {
    for action in [BrowserAction::Observe, BrowserAction::Navigate] {
        let runtime = std::sync::Arc::new(BrowserRuntime::new());
        let host = std::sync::Arc::new(ClosingHost {
            tabs: parking_lot::Mutex::new(vec![tab()]),
            calls: AtomicUsize::new(0),
            closed: AtomicUsize::new(0),
            release: tokio::sync::Notify::new(),
        });
        runtime.install_host(host.clone()).unwrap();
        let mut pending = request("scope-changing", action.clone());
        if action == BrowserAction::Navigate {
            pending.payload = json!({"url":"https://example.org/next"});
        }
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.execute(pending, CancellationToken::new()).await }
        });
        while host.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        runtime
            .quiesce(std::time::Duration::from_millis(200))
            .await
            .unwrap();
        let result = task.await.unwrap();
        if action == BrowserAction::Navigate {
            assert_eq!(result.unwrap().state, "result_unknown");
            assert_eq!(runtime.unknown_operations().len(), 1);
        } else {
            assert_eq!(result.unwrap_err().code, "observation_interrupted");
        }
        assert!(host.tabs().is_empty());
        assert!(runtime.tabs().is_empty());
        assert!(!runtime.available());
        assert_eq!(host.closed.load(Ordering::SeqCst), 1);
        assert_eq!(
            runtime
                .execute(
                    request("after-revocation", BrowserAction::Observe),
                    CancellationToken::new()
                )
                .await
                .unwrap_err()
                .code,
            "browser_quiesced"
        );
        assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn visible_runtime_quiesce_without_shutdown_confirmation_stays_fail_closed() {
    let runtime = BrowserRuntime::new();
    let host = host(false, false);
    runtime.install_host(host.clone()).unwrap();
    assert_eq!(
        runtime
            .quiesce(std::time::Duration::from_millis(100))
            .await
            .unwrap_err()
            .code,
        "native_shutdown_unsupported"
    );
    assert!(!runtime.available());
    assert!(runtime.tabs().is_empty());
    assert!(runtime.install_host(host.clone()).is_err());
    assert_eq!(
        runtime
            .execute(
                request("after-failed-shutdown", BrowserAction::Observe),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code,
        "browser_quiesced"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn visible_browser_profile_quiesce_failure_blocks_profile_transition() {
    use crate::modules::task::{
        TaskExecutionControlState, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind,
    };

    let root =
        std::env::temp_dir().join(format!("browser-profile-quiesce-{}", uuid::Uuid::new_v4()));
    let server = crate::AppServer::new_with_control_session_and_store(
        &root.join("state.db"),
        root.to_str().unwrap(),
        crate::safety::ControlSession::new("browser-profile-quiesce-test-000000000000000000")
            .unwrap(),
        std::sync::Arc::new(crate::integrations::secret::InMemorySecretStore::new()),
    )
    .unwrap();
    server
        .browser_runtime
        .install_host(host(false, false))
        .unwrap();
    let graph = TaskGraphId::new("active-before-profile-change").unwrap();
    let node = TaskNodeId::new("work").unwrap();
    server
        .task_world
        .create_graph(
            graph.clone(),
            vec![TaskNode::new(
                node.clone(),
                TaskNodeKind::Work,
                "Local fixture",
                serde_json::json!({}),
            )
            .unwrap()],
            vec![],
            1,
        )
        .unwrap();
    let attempt = server
        .task_world
        .start_execution(&graph, &node, 1, 2)
        .unwrap();
    let task_cancel = server
        .task_world
        .execution_cancellation_token(&attempt.id)
        .unwrap();
    let result = crate::modules::cloud::profile::ProfileManager::quiesce_runtime(
        &server,
        std::time::Duration::from_millis(100),
    )
    .await;
    assert!(matches!(
        result,
        Err(crate::modules::cloud::contracts::ScopeError::QuiesceTimeout)
    ));
    assert!(server.browser_runtime.is_quiesced());
    assert!(task_cancel.is_cancelled());
    assert_eq!(
        server
            .db
            .load_task_execution_control(&graph)
            .unwrap()
            .unwrap()
            .state,
        TaskExecutionControlState::Paused
    );
}
