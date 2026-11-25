//! Popups use ordinary runtime ownership and the opener's browser context.
use super::*;

impl NativeBrowserHost {
    pub(super) async fn open_popup_request(
        &self,
        id: String,
        parent: BrowserTarget,
        url: String,
    ) -> Result<(), BrowserError> {
        let state = self.lookup(Some(&parent))?;
        // A blank native popup is assigned by SetNewWindow, not by public URL navigation.
        let entry = if url.is_empty() || url == "about:blank" {
            if state.tab.url == "about:blank" {
                state.entry_url.clone()
            } else {
                state.tab.url.clone()
            }
        } else {
            let parsed = yilian_backend::browser::validate_browser_url(&url)?;
            if !state.proxy.permits_navigation(parsed.as_str()) {
                return Err(error(
                    "blocked_origin",
                    "The popup address is outside this browser context",
                ));
            }
            parsed.to_string()
        };
        let runtime = self
            .runtime
            .upgrade()
            .ok_or_else(|| error("native_closed", "Browser runtime has stopped"))?;
        let request = BrowserRequest {
            operation_id: self.id("browser-popup-open"),
            action: BrowserAction::Open,
            target: Some(parent.clone()),
            payload: json!({
                "url": entry,
                "workspace_id": state.tab.workspace_id,
                "ephemeral": state.tab.ephemeral,
                "_native_popup_id": id,
                "_native_parent_tab_id": parent.tab_id,
                "_native_popup_blank": url.is_empty() || url == "about:blank",
            }),
            owner: None,
        };
        let reply = runtime.execute(request, CancellationToken::new()).await?;
        if !matches!(reply.state.as_str(), "verified" | "result_unknown") {
            return Err(error(
                "native_popup_failed",
                "The popup could not be opened",
            ));
        }
        Ok(())
    }
}
