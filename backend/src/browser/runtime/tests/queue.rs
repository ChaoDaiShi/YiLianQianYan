use super::*;
use async_trait::async_trait;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{Barrier, Semaphore};
use tokio_util::sync::CancellationToken;

struct ProbeHost {
    entered: Barrier,
    first: Semaphore,
    others: Semaphore,
    navigation_calls: AtomicUsize,
}
fn tab(id: &str) -> BrowserTab {
    BrowserTab {
        session_id: format!("session-{id}"),
        tab_id: id.into(),
        workspace_id: None,
        site_id: None,
        url: "https://example.org/".into(),
        title: "Controlled audit".into(),
        navigation_id: 1,
        mode: "embedded".into(),
        loading: false,
        can_go_back: false,
        can_go_forward: false,
        error: None,
        ephemeral: true,
    }
}
fn request(id: &str, action: BrowserAction, tab_id: &str) -> BrowserRequest {
    BrowserRequest {
        operation_id: id.into(),
        action,
        target: Some(tab(tab_id).target()),
        payload: json!({"url":"https://example.org/next"}),
        owner: None,
    }
}
#[async_trait]
impl BrowserHost for ProbeHost {
    fn tabs(&self) -> Vec<BrowserTab> {
        vec![tab("one"), tab("other")]
    }
    async fn dispatch(
        &self,
        request: BrowserRequest,
        _: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        match request.action {
            BrowserAction::Navigate => {
                self.navigation_calls.fetch_add(1, Ordering::SeqCst);
                Ok(BrowserReply::verified(json!({"navigated":true})))
            }
            BrowserAction::Stop => Ok(BrowserReply::verified(json!({"stopped":true}))),
            BrowserAction::Click => {
                self.entered.wait().await;
                let _permit = self.first.acquire().await.unwrap();
                Ok(BrowserReply::unknown(
                    &request.operation_id,
                    "controlled uncertain click",
                ))
            }
            BrowserAction::Observe => {
                self.entered.wait().await;
                let _permit = self.others.acquire().await.unwrap();
                Ok(BrowserReply::verified(json!({})))
            }
            _ => unreachable!(),
        }
    }
}
fn fixture() -> (Arc<BrowserRuntime>, Arc<ProbeHost>) {
    let runtime = Arc::new(BrowserRuntime::new());
    let host = Arc::new(ProbeHost {
        entered: Barrier::new(5),
        first: Semaphore::new(0),
        others: Semaphore::new(0),
        navigation_calls: AtomicUsize::new(0),
    });
    runtime.install_host(host.clone()).unwrap();
    (runtime, host)
}

#[tokio::test]
async fn queued_navigation_must_not_bypass_an_earlier_unknown_click() {
    let (runtime, host) = fixture();
    let rt = runtime.clone();
    let first = tokio::spawn(async move {
        rt.execute(
            request("unknown-click", BrowserAction::Click, "one"),
            CancellationToken::new(),
        )
        .await
    });
    let mut blockers = vec![];
    for i in 0..3 {
        let rt = runtime.clone();
        blockers.push(tokio::spawn(async move {
            rt.execute(
                request(&format!("read-{i}"), BrowserAction::Observe, "other"),
                CancellationToken::new(),
            )
            .await
        }));
    }
    host.entered.wait().await;
    let queued = runtime.execute(
        request("queued-nav", BrowserAction::Navigate, "one"),
        CancellationToken::new(),
    );
    tokio::pin!(queued);
    assert!(futures::poll!(queued.as_mut()).is_pending());
    host.first.add_permits(1);
    let first_reply = first.await.unwrap().unwrap();
    assert_eq!(first_reply.state, "result_unknown");
    assert_eq!(runtime.unknown_operations().len(), 1);
    let queued_reply = queued.await;
    host.others.add_permits(3);
    for blocker in blockers {
        blocker.await.unwrap().unwrap();
    }
    eprintln!(
        "AUDIT unknown_click={} queued_reply={queued_reply:?} native_navigation_calls={}",
        first_reply.state,
        host.navigation_calls.load(Ordering::SeqCst)
    );
    assert_eq!(
        host.navigation_calls.load(Ordering::SeqCst),
        0,
        "queued navigation dispatched without reviewing the earlier uncertain click"
    );
    assert_eq!(queued_reply.unwrap_err().code, "unknown_requires_review");
}

#[tokio::test]
async fn stop_must_cancel_navigation_already_queued_for_the_same_tab() {
    let (runtime, host) = fixture();
    let mut blockers = vec![];
    for i in 0..4 {
        let rt = runtime.clone();
        blockers.push(tokio::spawn(async move {
            rt.execute(
                request(&format!("block-{i}"), BrowserAction::Observe, "other"),
                CancellationToken::new(),
            )
            .await
        }));
    }
    host.entered.wait().await;
    let queued = runtime.execute(
        request("queued-before-stop", BrowserAction::Navigate, "one"),
        CancellationToken::new(),
    );
    tokio::pin!(queued);
    assert!(futures::poll!(queued.as_mut()).is_pending());
    let stop = runtime
        .execute(
            request("stop-one", BrowserAction::Stop, "one"),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(stop.state, "verified");
    host.others.add_permits(4);
    for blocker in blockers {
        blocker.await.unwrap().unwrap();
    }
    let queued_reply = queued.await;
    eprintln!(
        "AUDIT stop={} queued_reply={queued_reply:?} native_navigation_calls={}",
        stop.state,
        host.navigation_calls.load(Ordering::SeqCst)
    );
    assert_eq!(
        host.navigation_calls.load(Ordering::SeqCst),
        0,
        "navigation queued before successful Stop still dispatched afterward"
    );
    assert_eq!(queued_reply.unwrap_err().code, "cancelled_before_execution");
}

struct DocumentGateHost {
    gate: tokio::sync::Mutex<()>,
    entered: tokio::sync::Notify,
    release: Semaphore,
    calls: AtomicUsize,
    mutations: AtomicUsize,
}
#[async_trait]
impl BrowserHost for DocumentGateHost {
    fn tabs(&self) -> Vec<BrowserTab> {
        vec![tab("one"), tab("other")]
    }
    async fn dispatch(
        &self,
        request: BrowserRequest,
        _: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if matches!(
            request.action,
            BrowserAction::Observe
                | BrowserAction::Capture
                | BrowserAction::Stop
                | BrowserAction::Close
                | BrowserAction::Present
        ) {
            return Ok(BrowserReply::verified(json!({})));
        }
        let _gate = self.gate.lock().await;
        self.mutations.fetch_add(1, Ordering::SeqCst);
        if request.action == BrowserAction::Click {
            self.entered.notify_one();
            self.release.acquire().await.unwrap().forget();
            return Ok(BrowserReply::unknown(
                &request.operation_id,
                "document click had an uncertain outcome",
            ));
        }
        Ok(BrowserReply::verified(json!({})))
    }
}
fn document_fixture() -> (Arc<BrowserRuntime>, Arc<DocumentGateHost>) {
    let runtime = Arc::new(BrowserRuntime::new());
    let host = Arc::new(DocumentGateHost {
        gate: tokio::sync::Mutex::new(()),
        entered: tokio::sync::Notify::new(),
        release: Semaphore::new(0),
        calls: AtomicUsize::new(0),
        mutations: AtomicUsize::new(0),
    });
    runtime.install_host(host.clone()).unwrap();
    (runtime, host)
}
fn click_task(
    runtime: &Arc<BrowserRuntime>,
) -> tokio::task::JoinHandle<Result<BrowserReply, BrowserError>> {
    let runtime = runtime.clone();
    tokio::spawn(async move {
        runtime
            .execute(
                request("click", BrowserAction::Click, "one"),
                CancellationToken::new(),
            )
            .await
    })
}

#[tokio::test]
async fn document_queue_waits_for_receipt_before_native_dispatch_and_preserves_review_replay() {
    let (runtime, host) = document_fixture();
    let click = click_task(&runtime);
    host.entered.notified().await;
    let navigation = request("next", BrowserAction::Navigate, "one");
    let queued = runtime.execute(navigation.clone(), CancellationToken::new());
    tokio::pin!(queued);
    assert!(futures::poll!(queued.as_mut()).is_pending());
    assert_eq!(
        host.calls.load(Ordering::SeqCst),
        1,
        "same-tab mutation must not enter a native gate before prior receipt settlement"
    );
    assert_eq!(
        runtime
            .execute(navigation.clone(), CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "operation_in_progress"
    );
    host.release.add_permits(1);
    assert_eq!(click.await.unwrap().unwrap().state, "result_unknown");
    assert_eq!(queued.await.unwrap_err().code, "unknown_requires_review");
    for action in [
        BrowserAction::Observe,
        BrowserAction::Capture,
        BrowserAction::Stop,
        BrowserAction::Close,
        BrowserAction::Present,
    ] {
        runtime
            .execute(
                request(&format!("control-{}", action.name()), action, "one"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
    }
    runtime.acknowledge_unknown("click").unwrap();
    assert_eq!(
        runtime
            .execute(navigation, CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "unknown_requires_review",
        "acknowledgment must not replay the rejected queued operation"
    );
    runtime
        .execute(
            request("after-review", BrowserAction::Navigate, "one"),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(host.mutations.load(Ordering::SeqCst), 2);
    assert_eq!(
        runtime
            .execute(
                request("click", BrowserAction::Click, "one"),
                CancellationToken::new()
            )
            .await
            .unwrap()
            .state,
        "result_unknown"
    );
    assert_eq!(host.mutations.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn stop_cancels_mutation_gate_waiter_and_keeps_other_tab_running() {
    let (runtime, host) = document_fixture();
    let click = click_task(&runtime);
    host.entered.notified().await;
    let queued = runtime.execute(
        request("queued-mutation", BrowserAction::Navigate, "one"),
        CancellationToken::new(),
    );
    tokio::pin!(queued);
    assert!(futures::poll!(queued.as_mut()).is_pending());
    let other = runtime.execute(
        request("other-tab", BrowserAction::Navigate, "other"),
        CancellationToken::new(),
    );
    tokio::pin!(other);
    assert!(futures::poll!(other.as_mut()).is_pending());
    let other_cancel = runtime.in_flight.lock()["other-tab"].cancel.clone();
    runtime
        .execute(
            request("stop", BrowserAction::Stop, "one"),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(queued.await.unwrap_err().code, "cancelled_before_execution");
    assert_eq!(click.await.unwrap().unwrap().state, "result_unknown");
    assert!(
        !other_cancel.is_cancelled(),
        "Stop must not cancel another tab"
    );
    other.await.unwrap();
    assert!(runtime.in_flight.lock().is_empty());
    assert_eq!(host.mutations.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn dropping_queued_caller_settles_receipt_and_removes_cancellation_registration() {
    let (runtime, host) = document_fixture();
    let click = click_task(&runtime);
    host.entered.notified().await;
    let navigation = request("dropped-queue", BrowserAction::Navigate, "one");
    let mut queued = Box::pin(runtime.execute(navigation.clone(), CancellationToken::new()));
    assert!(futures::poll!(queued.as_mut()).is_pending());
    assert!(runtime.in_flight.lock().contains_key("dropped-queue"));
    drop(queued);
    assert!(!runtime.in_flight.lock().contains_key("dropped-queue"));
    assert_eq!(
        runtime
            .execute(navigation, CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "cancelled_before_execution"
    );
    click.abort();
    let _ = click.await;
    assert!(runtime.in_flight.lock().is_empty());
    assert_eq!(runtime.unknown_operations().len(), 1);
    assert_eq!(host.mutations.load(Ordering::SeqCst), 1);
}
