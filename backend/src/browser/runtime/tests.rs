use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Host {
    tabs: parking_lot::Mutex<Vec<BrowserTab>>,
    calls: AtomicUsize,
    wait: bool,
    unknown: bool,
}
fn tab() -> BrowserTab {
    BrowserTab {
        session_id: "session".into(),
        tab_id: "tab".into(),
        workspace_id: None,
        site_id: None,
        url: "https://example.org/".into(),
        title: "Fixture".into(),
        navigation_id: 1,
        mode: "embedded".into(),
        loading: false,
        can_go_back: false,
        can_go_forward: false,
        error: None,
        ephemeral: true,
    }
}
fn host(wait: bool, unknown: bool) -> std::sync::Arc<Host> {
    std::sync::Arc::new(Host {
        tabs: parking_lot::Mutex::new(vec![tab()]),
        calls: AtomicUsize::new(0),
        wait,
        unknown,
    })
}
#[async_trait::async_trait]
impl BrowserHost for Host {
    fn tabs(&self) -> Vec<BrowserTab> {
        self.tabs.lock().clone()
    }
    async fn dispatch(
        &self,
        request: BrowserRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.wait
            && !matches!(
                request.action,
                BrowserAction::Stop | BrowserAction::Close | BrowserAction::Present
            )
        {
            cancel.cancelled().await;
            return Err(BrowserError::new("interrupted", "in flight"));
        }
        if self.unknown && request.action == BrowserAction::Click {
            return Ok(BrowserReply::unknown(
                &request.operation_id,
                "uncertain submission",
            ));
        }
        Ok(BrowserReply::verified(json!({"tab":tab()})))
    }
}
fn request(id: &str, action: BrowserAction) -> BrowserRequest {
    BrowserRequest {
        operation_id: id.into(),
        action,
        target: Some(tab().target()),
        payload: json!({}),
        owner: None,
    }
}

mod authorizations;
mod execution;
mod queue;
mod quiescence;
