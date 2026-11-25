use super::*;

#[tokio::test]
async fn visible_runtime_unavailable_is_explicit() {
    let runtime = BrowserRuntime::new();
    assert!(!runtime.available());
    assert_eq!(
        runtime
            .execute(
                request("missing", BrowserAction::Observe),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code,
        "native_browser_unavailable"
    );
}
#[tokio::test]
async fn visible_runtime_duplicate_does_not_repeat_and_changed_payload_conflicts() {
    let runtime = BrowserRuntime::new();
    let host = host(false, false);
    runtime.install_host(host.clone()).unwrap();
    let request = request("once", BrowserAction::Fill);
    runtime
        .execute(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    runtime
        .execute(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    let mut changed = request;
    changed.payload = json!({"value":"different"});
    assert_eq!(
        runtime
            .execute(changed, CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "operation_conflict"
    );
}
#[tokio::test]
async fn visible_runtime_rejects_stale_revision_and_cross_session_before_host() {
    let runtime = BrowserRuntime::new();
    let host = host(false, false);
    runtime.install_host(host.clone()).unwrap();
    let mut stale = request("stale", BrowserAction::Fill);
    stale.target.as_mut().unwrap().navigation_id = 0;
    assert_eq!(
        runtime
            .execute(stale, CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "stale_navigation"
    );
    let mut wrong = request("wrong", BrowserAction::Observe);
    wrong.target.as_mut().unwrap().session_id = "other".into();
    assert_eq!(
        runtime
            .execute(wrong, CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "session_mismatch"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn visible_runtime_unknown_blocks_actions_but_not_observe_or_emergency_controls() {
    let runtime = BrowserRuntime::new();
    let host = host(false, true);
    runtime.install_host(host.clone()).unwrap();
    assert_eq!(
        runtime
            .execute(
                request("click", BrowserAction::Click),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .state,
        "result_unknown"
    );
    assert_eq!(
        runtime
            .execute(
                request("again", BrowserAction::Fill),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code,
        "unknown_requires_review"
    );
    for (id, action) in [
        ("observe", BrowserAction::Observe),
        ("stop", BrowserAction::Stop),
        ("close", BrowserAction::Close),
        ("present", BrowserAction::Present),
    ] {
        runtime
            .execute(request(id, action), CancellationToken::new())
            .await
            .unwrap();
    }
    runtime.acknowledge_unknown("click").unwrap();
    runtime
        .execute(
            request("after-review", BrowserAction::Fill),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(runtime.unknown_operations().is_empty());
    assert_eq!(
        runtime
            .execute(
                request("click", BrowserAction::Click),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .state,
        "result_unknown",
        "Review does not turn original uncertain receipt into success"
    );
}
#[tokio::test]
async fn visible_runtime_cancel_before_dispatch_does_not_call_host() {
    let runtime = BrowserRuntime::new();
    let host = host(false, false);
    runtime.install_host(host.clone()).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        runtime
            .execute(request("cancel", BrowserAction::Click), cancel)
            .await
            .unwrap_err()
            .code,
        "cancelled_before_execution"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn visible_runtime_cancel_after_dispatch_is_unknown_and_cancels_native() {
    let runtime = std::sync::Arc::new(BrowserRuntime::new());
    let host = host(true, false);
    runtime.install_host(host.clone()).unwrap();
    let cancel = CancellationToken::new();
    let child = cancel.clone();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .execute(request("in-flight", BrowserAction::Click), child)
                .await
        }
    });
    while host.calls.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    assert_eq!(task.await.unwrap().unwrap().state, "result_unknown");
    assert_eq!(runtime.unknown_operations().len(), 1);
}
#[tokio::test]
async fn visible_runtime_dropped_caller_retains_unknown_receipt() {
    let runtime = std::sync::Arc::new(BrowserRuntime::new());
    let host = host(true, false);
    runtime.install_host(host.clone()).unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .execute(
                    request("dropped", BrowserAction::Click),
                    CancellationToken::new(),
                )
                .await
        }
    });
    while host.calls.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    task.abort();
    let _ = task.await;
    assert_eq!(runtime.unknown_operations().len(), 1);
    assert_eq!(
        runtime
            .execute(
                request("dropped", BrowserAction::Click),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .state,
        "result_unknown"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn visible_runtime_timeout_keeps_unknown_without_replaying() {
    let mut runtime = BrowserRuntime::new();
    runtime.timeout = std::time::Duration::from_millis(5);
    let host = host(true, false);
    runtime.install_host(host.clone()).unwrap();
    assert_eq!(
        runtime
            .execute(
                request("timeout", BrowserAction::Fill),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .state,
        "result_unknown"
    );
    assert_eq!(
        runtime
            .execute(
                request("timeout", BrowserAction::Fill),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .state,
        "result_unknown"
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn visible_runtime_reads_and_stop_remain_available_when_mutation_ledger_full() {
    let runtime = BrowserRuntime::new();
    let host = host(false, false);
    runtime.install_host(host.clone()).unwrap();
    {
        let mut ops = runtime.operations.lock();
        for i in 0..MAX_OPERATIONS {
            let request = request(&format!("retained-{i}"), BrowserAction::Fill);
            ops.insert(
                request.operation_id.clone(),
                Operation {
                    fingerprint: "retained".into(),
                    request,
                    result: Some(Ok(BrowserReply::verified(json!({})))),
                    unknown: false,
                    acknowledged: false,
                },
            );
        }
    }
    assert_eq!(
        runtime
            .execute(
                request("full", BrowserAction::Fill),
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code,
        "operation_limit"
    );
    runtime
        .execute(
            request("still-read", BrowserAction::Observe),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    runtime
        .execute(
            request("still-stop", BrowserAction::Stop),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(host.calls.load(Ordering::SeqCst), 2);
}
#[test]
fn visible_browser_url_rejects_private_schemes_credentials_and_app_origins() {
    for value in [
        "file:///C:/Windows/",
        "https://user:pass@example.org/",
        "http://127.0.0.1:9420/api/settings",
        "http://localhost:1420/",
        "http://a.tauri.localhost/",
        "http://x.asset.localhost/",
        "https://example.org/\n",
    ] {
        assert!(validate_browser_url(value).is_err(), "{value}");
    }
    assert!(validate_browser_url("http://localhost:3000/").is_ok());
}

#[tokio::test]
async fn visible_runtime_four_pending_opens_cannot_starve_present_or_stop() {
    let runtime = std::sync::Arc::new(BrowserRuntime::new());
    let host = host(true, false);
    runtime.install_host(host.clone()).unwrap();
    let mut tasks = vec![];
    for index in 0..4 {
        let runtime = runtime.clone();
        tasks.push(tokio::spawn(async move {
            runtime
                .execute(
                    BrowserRequest {
                        operation_id: format!("open-{index}"),
                        action: BrowserAction::Open,
                        target: None,
                        payload: json!({"url":"https://example.org"}),
                        owner: None,
                    },
                    CancellationToken::new(),
                )
                .await
        }));
    }
    while host.calls.load(Ordering::SeqCst) < 4 {
        tokio::task::yield_now().await;
    }
    let mut control = request("present", BrowserAction::Present);
    control.target.as_mut().unwrap().navigation_id = 0;
    tokio::time::timeout(
        std::time::Duration::from_millis(200),
        runtime.execute(control, CancellationToken::new()),
    )
    .await
    .expect("Present must bypass occupied execution slots")
    .unwrap();
    for task in tasks {
        task.abort();
        let _ = task.await;
    }
}
#[tokio::test]
async fn visible_runtime_stop_cancels_inflight_action_on_same_tab() {
    let runtime = std::sync::Arc::new(BrowserRuntime::new());
    let host = host(true, false);
    runtime.install_host(host.clone()).unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .execute(
                    request("pending-action", BrowserAction::Click),
                    CancellationToken::new(),
                )
                .await
        }
    });
    while host.calls.load(Ordering::SeqCst) < 1 {
        tokio::task::yield_now().await;
    }
    runtime
        .execute(
            request("stop-now", BrowserAction::Stop),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_millis(200), task)
        .await
        .expect("Stop must cancel same-tab native work")
        .unwrap()
        .unwrap();
    assert_eq!(result.state, "result_unknown");
}
