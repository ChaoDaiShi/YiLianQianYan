//! History is a projection of verified native navigation, never service ownership.
use super::*;
use serde_json::{json, Value};
impl BrowserRuntime {
    pub(super) fn project_saved_site_visit(
        &self,
        request: &BrowserRequest,
        reply: &mut BrowserReply,
    ) {
        if request.action != BrowserAction::Open
            || reply.state != "verified"
            || request
                .payload
                .get("site_id")
                .and_then(Value::as_str)
                .is_none()
        {
            return;
        }
        if reply
            .data
            .get("history_projection")
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            == Some("recorded")
        {
            return;
        }
        let prior = reply.data.get("history_projection");
        let observed_at = prior
            .and_then(|value| value.get("observed_at"))
            .and_then(Value::as_i64)
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let attempts = prior
            .and_then(|value| value.get("attempts"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .saturating_add(1);
        let result = (|| -> Result<Value, String> {
            if self.is_quiesced() {
                return Err("browser_scope_revoked".into());
            }
            let db = self
                .browser_database()
                .ok_or("history_storage_unavailable")?;
            let site_id = request.payload["site_id"]
                .as_str()
                .ok_or("invalid_saved_site")?;
            let workspace = request
                .payload
                .get("workspace_id")
                .and_then(Value::as_str)
                .ok_or("invalid_saved_site_workspace")?;
            let site = db
                .get_browser_site(site_id)
                .map_err(|_| "saved_site_unavailable")?;
            let tab: BrowserTab =
                serde_json::from_value(reply.data.get("tab").cloned().ok_or("native_tab_missing")?)
                    .map_err(|_| "native_tab_invalid")?;
            if tab.site_id.as_deref() != Some(site_id)
                || tab.workspace_id.as_deref() != Some(workspace)
                || site.workspace_id != workspace
            {
                return Err("native_tab_site_scope_mismatch".into());
            }
            if tab.loading
                || tab.navigation_id == 0
                || tab
                    .error
                    .as_deref()
                    .is_some_and(|error| error.starts_with("Navigation failed"))
            {
                return Err("native_navigation_not_ready".into());
            }
            let actual = validate_browser_url(&tab.url).map_err(|_| "native_url_invalid")?;
            if actual.origin().ascii_serialization() != site.origin {
                return Ok(
                    json!({"status":"skipped","reason":"navigation_left_registered_origin","observed_at":observed_at}),
                );
            }
            let visit = db
                .note_browser_visit(
                    &crate::db::browser_sites::BrowserVisitInput {
                        workspace_id: Some(workspace.into()),
                        site_id: Some(site_id.into()),
                        session_id: tab.session_id,
                        tab_id: tab.tab_id,
                        navigation_id: tab.navigation_id.to_string(),
                        url: actual.to_string(),
                        title: tab.title,
                    },
                    observed_at,
                )
                .map_err(|_| "history_storage_failed")?;
            Ok(
                json!({"status":"recorded","visit_id":visit.id,"observed_at":observed_at,"attempts":attempts,"service_identity":"unverified"}),
            )
        })();
        reply.data["history_projection"] = match result {
            Ok(value) => value,
            Err(error) => {
                json!({"status":"failed","error":crate::utils::text::truncate_chars(&error,160),"operation_id":request.operation_id,"observed_at":observed_at,"attempts":attempts,"recovery":"Refresh saved-site history to retry metadata only; do not reopen the browser"})
            }
        };
    }
    /// Bounded, local-only history repair. The original native operation is never
    /// dispatched again. Each explicit history query repairs at most 16 receipts;
    /// there is no background retry loop or model/tool invocation.
    pub fn reconcile_saved_site_visits(
        &self,
        workspace: Option<&str>,
        site: Option<&str>,
        limit: usize,
    ) -> Value {
        if self.is_quiesced() {
            return json!({"issues":[],"pending_count":0,"truncated":false});
        }
        let pending = self.pending_visit_projections(workspace, site);
        let pending_count = pending.len();
        let limit = limit.clamp(1, 16);
        let mut issues = vec![];
        for (request, mut reply) in pending.into_iter().take(limit) {
            self.project_saved_site_visit(&request, &mut reply);
            self.update_cached_visit_reply(&request, &reply);
            if reply.data["history_projection"]["status"] == "failed" {
                issues.push(reply.data["history_projection"].clone());
            }
        }
        let remaining = self.pending_visit_projections(workspace, site).len();
        json!({"issues":issues,"pending_count":remaining,"truncated":pending_count>limit})
    }
}
