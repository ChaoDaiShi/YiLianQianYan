use super::*;

mod lifecycle;
mod queue;
use parking_lot::{Mutex, RwLock};
use queue::OperationGuard;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

const MAX_OPERATIONS: usize = 2048;
const MAX_REQUEST_BYTES: usize = 65_536;
const MAX_REPLY_BYTES: usize = 8 * 1024 * 1024;
struct InFlight {
    target: Option<BrowserTarget>,
    cancel: CancellationToken,
    agent: bool,
    mutation_sequence: Option<Arc<tokio::sync::Mutex<()>>>,
}
#[derive(Clone)]
struct Operation {
    fingerprint: String,
    request: BrowserRequest,
    result: Option<Result<BrowserReply, BrowserError>>,
    unknown: bool,
    acknowledged: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownBrowserOperation {
    pub operation_id: String,
    pub action: BrowserAction,
    pub target: Option<BrowserTarget>,
}
/// Process-lifetime bridge only. Native host remains the authority for real tabs,
/// navigation revisions and observations. No tools, grants or tasks are created here.
pub struct BrowserRuntime {
    host: RwLock<Option<Arc<dyn BrowserHost>>>,
    profile_context: std::sync::OnceLock<BrowserProfileContext>,
    credential_vault: std::sync::OnceLock<BrowserCredentialVault>,
    local_fixture_bundled_skills: Option<std::path::PathBuf>,
    quiesced: AtomicBool,
    authorization_epoch: AtomicU64,
    authorization_changes: tokio::sync::watch::Sender<u64>,
    lifecycle_gate: Mutex<()>,
    shutdown_gate: tokio::sync::Mutex<()>,
    operations: Arc<Mutex<HashMap<String, Operation>>>,
    slots: Arc<Semaphore>,
    timeout: Duration,
    in_flight: Arc<Mutex<HashMap<String, InFlight>>>,
    pub(super) evidence: RwLock<Option<super::evidence::BrowserEvidence>>,
}
impl Default for BrowserRuntime {
    fn default() -> Self {
        Self::new()
    }
}
impl BrowserRuntime {
    pub(crate) fn configure_credential_vault(
        &self,
        db: crate::db::Database,
        store: Arc<dyn crate::integrations::secret::SecretStore>,
    ) -> Result<(), String> {
        let vault =
            BrowserCredentialVault::new(db, store, self.local_fixture_bundled_skills().is_some())?;
        self.credential_vault
            .set(vault)
            .map_err(|_| "密码箱已初始化".to_owned())
    }

    pub fn credential_vault(&self) -> Result<&BrowserCredentialVault, String> {
        self.credential_vault
            .get()
            .ok_or_else(|| "密码箱尚未就绪".to_owned())
    }

    pub(super) fn pending_visit_projections(
        &self,
        workspace: Option<&str>,
        site: Option<&str>,
    ) -> Vec<(BrowserRequest, BrowserReply)> {
        self.operations
            .lock()
            .values()
            .filter_map(|operation| {
                let reply = operation.result.as_ref()?.as_ref().ok()?;
                if reply.state != "verified"
                    || reply.data["history_projection"]["status"] != "failed"
                    || workspace.is_some_and(|id| {
                        operation.request.payload["workspace_id"].as_str() != Some(id)
                    })
                    || site
                        .is_some_and(|id| operation.request.payload["site_id"].as_str() != Some(id))
                {
                    return None;
                }
                Some((operation.request.clone(), reply.clone()))
            })
            .collect()
    }
    pub(super) fn update_cached_visit_reply(&self, request: &BrowserRequest, reply: &BrowserReply) {
        if let Some(operation) = self.operations.lock().get_mut(&request.operation_id) {
            if operation.result.as_ref().is_some_and(|result| {
                result
                    .as_ref()
                    .is_ok_and(|stored| stored.state == "verified")
            }) {
                operation.result = Some(Ok(reply.clone()));
            }
        }
    }
    pub fn unknown_operations(&self) -> Vec<UnknownBrowserOperation> {
        self.operations
            .lock()
            .iter()
            .filter(|(_, op)| op.unknown && !op.acknowledged)
            .map(|(id, op)| UnknownBrowserOperation {
                operation_id: id.clone(),
                action: op.request.action.clone(),
                target: op.request.target.clone(),
            })
            .collect()
    }
    /// Only a trusted, explicit human review command may call this. The receipt
    /// stays immutable; acknowledging does not retry or convert it to success.
    pub fn acknowledge_unknown(&self, operation_id: &str) -> Result<(), BrowserError> {
        let mut ops = self.operations.lock();
        let op = ops
            .get_mut(operation_id)
            .ok_or_else(|| BrowserError::new("operation_not_found", "No such browser operation"))?;
        if !op.unknown {
            return Err(BrowserError::new(
                "not_unknown",
                "Operation does not require review",
            ));
        }
        op.acknowledged = true;
        Ok(())
    }
    fn validate_request_target(
        tabs: &[BrowserTab],
        request: &BrowserRequest,
        target: &BrowserTarget,
    ) -> Result<BrowserTab, BrowserError> {
        if matches!(
            request.action,
            BrowserAction::Stop | BrowserAction::Close | BrowserAction::Present
        ) {
            let tab = tabs
                .iter()
                .find(|tab| tab.tab_id == target.tab_id && tab.session_id == target.session_id)
                .ok_or_else(|| {
                    BrowserError::new(
                        "tab_not_found",
                        "Browser control target no longer exists in this session",
                    )
                })?;
            return Ok(tab.clone());
        }
        validate_target(tabs, target)
    }
    pub fn validate_target(&self, target: &BrowserTarget) -> Result<BrowserTab, BrowserError> {
        validate_target(&self.tabs(), target)
    }
    pub async fn execute(
        &self,
        request: BrowserRequest,
        cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        self.execute_authorized(request, cancel, None).await
    }
    pub(crate) async fn execute_authorized(
        &self,
        request: BrowserRequest,
        cancel: CancellationToken,
        expected_authorization_epoch: Option<u64>,
    ) -> Result<BrowserReply, BrowserError> {
        let agent = request.owner.is_some() || expected_authorization_epoch.is_some();
        let (mut authorization_changes, current_authorization_epoch) = {
            let _lifecycle = self.lifecycle_gate.lock();
            (
                self.authorization_changes.subscribe(),
                self.authorization_epoch(),
            )
        };
        let authorization_epoch =
            expected_authorization_epoch.unwrap_or(current_authorization_epoch);
        if agent && self.authorization_epoch() != authorization_epoch {
            return Err(BrowserError::new(
                "permission_revoked_before_execution",
                "Browser permission evidence was revoked before runtime entry",
            ));
        }
        if self.is_quiesced() && !is_control(&request.action) {
            return Err(BrowserError::new(
                "browser_quiesced",
                "Browser sessions were revoked; restart is required",
            ));
        }
        validate_request(&request)?;
        if cancel.is_cancelled() {
            return Err(BrowserError::new(
                "cancelled_before_execution",
                "Browser operation cancelled before dispatch",
            ));
        }
        let host = self.host.read().clone().ok_or_else(|| {
            BrowserError::new(
                "native_browser_unavailable",
                "Visible browser requires the desktop application",
            )
        })?;
        let fingerprint = serde_json::to_string(&request).map_err(|_| {
            BrowserError::new("invalid_request", "Cannot serialize browser request")
        })?;
        let cached = {
            let ops = self.operations.lock();
            if let Some(previous) = ops.get(&request.operation_id) {
                if previous.fingerprint != fingerprint {
                    return Err(BrowserError::new(
                        "operation_conflict",
                        "Operation id was already used for different arguments",
                    ));
                }
                Some(previous.result.clone().unwrap_or_else(|| {
                    Err(BrowserError::new(
                        "operation_in_progress",
                        "Original operation is still pending; it was not repeated",
                    ))
                }))
            } else {
                None
            }
        };
        if let Some(mut result) = cached {
            if let Ok(reply) = &mut result {
                self.project_saved_site_visit(&request, reply);
                self.update_cached_visit_reply(&request, reply);
            }
            if (self.is_quiesced() && !is_control(&request.action))
                || (agent && self.authorization_epoch() != authorization_epoch)
            {
                return Err(BrowserError::new(
                    "permission_revoked_before_execution",
                    "Browser authorization changed before cached result delivery",
                ));
            }
            return result;
        }
        self.validate_project_context(&request)?;
        let mut guard = {
            // Acceptance, cancellation registration and Stop share this lock.
            // Stop therefore sees every request accepted before it.
            let _lifecycle = self.lifecycle_gate.lock();
            let mut ops = self.operations.lock();
            if agent && self.authorization_epoch() != authorization_epoch {
                return Err(BrowserError::new(
                    "permission_revoked_before_execution",
                    "Browser authorization changed before dispatch",
                ));
            }
            if self.is_quiesced() && !is_control(&request.action) {
                return Err(BrowserError::new(
                    "browser_quiesced",
                    "Browser sessions were revoked; restart is required",
                ));
            }
            if let Some(previous) = ops.get(&request.operation_id) {
                if previous.fingerprint != fingerprint {
                    return Err(BrowserError::new(
                        "operation_conflict",
                        "Operation id was already used for different arguments",
                    ));
                }
                return previous.result.clone().unwrap_or_else(|| {
                    Err(BrowserError::new(
                        "operation_in_progress",
                        "Original operation is still pending; it was not repeated",
                    ))
                });
            }
            // Read/control receipts may contain image bytes. Bound that cache
            // independently of the durable-in-session mutation replay ledger.
            let completed_reads = ops
                .values()
                .filter(|op| {
                    op.result.is_some()
                        && !op.unknown
                        && !op.request.action.requires_replay_barrier()
                })
                .count();
            if ops.len() >= MAX_OPERATIONS || completed_reads >= 8 {
                ops.retain(|_, op| {
                    op.result.is_none() || op.unknown || op.request.action.requires_replay_barrier()
                });
            }
            if ops.len() >= MAX_OPERATIONS && request.action.requires_replay_barrier() {
                return Err(BrowserError::new("operation_limit","Mutating-operation session limit reached; inspect results before restarting the application"));
            }
            if request.action.requires_replay_barrier()
                && ops.values().any(|op| {
                    op.unknown
                        && !op.acknowledged
                        && same_tab(op.request.target.as_ref(), request.target.as_ref())
                })
            {
                return Err(BrowserError::new("unknown_requires_review","An earlier action on this tab has an uncertain result; explicit human review is required"));
            }
            if let Some(target) = &request.target {
                Self::validate_request_target(&host.tabs(), &request, target)?;
            }
            if ops.len() < MAX_OPERATIONS {
                ops.insert(
                    request.operation_id.clone(),
                    Operation {
                        fingerprint,
                        request: request.clone(),
                        result: None,
                        unknown: false,
                        acknowledged: false,
                    },
                );
            }
            OperationGuard::register(self, &request, cancel.child_token(), agent)
        };
        if let Err(error) = guard
            .wait_for_dispatch(self, &mut authorization_changes, agent)
            .await
        {
            let result = Err(error);
            guard.finish(result.clone());
            return result;
        }
        if let Some(target) = &request.target {
            if let Err(error) = Self::validate_request_target(&host.tabs(), &request, target) {
                let result = Err(error);
                guard.finish(result.clone());
                return result;
            }
        }
        {
            let _lifecycle = self.lifecycle_gate.lock();
            if self.is_quiesced() && !is_control(&request.action) {
                let result = Err(BrowserError::new(
                    "browser_quiesced",
                    "Browser session was revoked before dispatch",
                ));
                guard.finish(result.clone());
                return result;
            }
            if agent && self.authorization_epoch() != authorization_epoch {
                let result = Err(BrowserError::new(
                    "permission_revoked_before_execution",
                    "Browser authorization changed before native dispatch",
                ));
                guard.finish(result.clone());
                return result;
            }
            if guard.cancel.is_cancelled() {
                let result = Err(BrowserError::new(
                    "cancelled_before_execution",
                    "Browser operation was cancelled while queued",
                ));
                guard.finish(result.clone());
                return result;
            }
            if let Err(error) = self.check_unknown_fence(&request) {
                let result = Err(error);
                guard.finish(result.clone());
                return result;
            }
            guard.dispatched = true;
        }
        let result = tokio::select! { biased;
            _=cancel.cancelled()=> {guard.cancel.cancel(); interrupted(&request,"Browser operation cancelled after dispatch")},
            _=guard.cancel.cancelled()=>interrupted(&request,"Browser session was revoked during the operation"),
            _=authorization_changes.changed(),if agent=>{guard.cancel.cancel();interrupted(&request,"Browser authorization was revoked during the operation")},
            _=tokio::time::sleep(self.timeout)=>{guard.cancel.cancel();interrupted(&request,"Native browser response timed out")},
            result=host.dispatch(request.clone(),guard.cancel.clone())=>{
                match result {
                    Ok(reply) if serde_json::to_vec(&reply).map(|v|v.len()).unwrap_or(usize::MAX)>MAX_REPLY_BYTES=>interrupted(&request,"Native browser reply exceeded the bounded limit"),
                    Ok(reply) if !matches!(reply.state.as_str(),"verified"|"result_unknown"|"cancelled")=>interrupted(&request,"Native host returned an unsupported result state"),
                    Err(error) if request.action.may_have_side_effects()&&!definite_rejection(&error.code)=>interrupted(&request,&error.message),
                    result=>result,
                }
            }
        };
        let result = if (self.is_quiesced() && !is_control(&request.action))
            || (agent && self.authorization_epoch() != authorization_epoch)
        {
            interrupted(
                &request,
                "Browser session was revoked before its result was delivered",
            )
        } else {
            result
        };
        let mut result = result;
        if let Ok(reply) = &mut result {
            self.project_saved_site_visit(&request, reply);
        }
        if (self.is_quiesced() && !is_control(&request.action))
            || (agent && self.authorization_epoch() != authorization_epoch)
        {
            result = interrupted(
                &request,
                "Browser authorization changed before metadata result delivery",
            );
        }
        guard.finish(result.clone());
        result
    }
}
fn is_control(action: &BrowserAction) -> bool {
    matches!(
        action,
        BrowserAction::Stop | BrowserAction::Close | BrowserAction::Present
    )
}
fn same_tab(left: Option<&BrowserTarget>, right: Option<&BrowserTarget>) -> bool {
    match (left, right) {
        (Some(a), Some(b)) => a.session_id == b.session_id && a.tab_id == b.tab_id,
        (None, None) => true,
        _ => false,
    }
}
fn interrupted(request: &BrowserRequest, reason: &str) -> Result<BrowserReply, BrowserError> {
    if request.action.may_have_side_effects() {
        Ok(BrowserReply::unknown(&request.operation_id, reason))
    } else {
        Err(BrowserError::new("observation_interrupted", reason))
    }
}
fn definite_rejection(code: &str) -> bool {
    code.starts_with("invalid_")
        || code.starts_with("stale_")
        || matches!(
            code,
            "cancelled_before_execution"
                | "blocked_origin"
                | "permission_denied"
                | "tab_not_found"
                | "session_mismatch"
                | "unsupported"
                | "native_browser_unavailable"
        )
}
fn validate_target(
    tabs: &[BrowserTab],
    target: &BrowserTarget,
) -> Result<BrowserTab, BrowserError> {
    let tab = tabs
        .iter()
        .find(|tab| tab.tab_id == target.tab_id)
        .ok_or_else(|| BrowserError::new("tab_not_found", "Visible tab no longer exists"))?;
    if tab.session_id != target.session_id {
        return Err(BrowserError::new(
            "session_mismatch",
            "Target belongs to another browser session",
        ));
    }
    if tab.navigation_id != target.navigation_id {
        return Err(BrowserError::new(
            "stale_navigation",
            "Page changed; observe the current document before acting",
        ));
    }
    Ok(tab.clone())
}
fn validate_request(request: &BrowserRequest) -> Result<(), BrowserError> {
    if request.operation_id.is_empty()
        || request.operation_id.len() > 160
        || request.operation_id.chars().any(char::is_control)
    {
        return Err(BrowserError::new(
            "invalid_operation_id",
            "A bounded operation id is required",
        ));
    }
    if serde_json::to_vec(&request.payload)
        .map(|v| v.len())
        .unwrap_or(usize::MAX)
        > MAX_REQUEST_BYTES
    {
        return Err(BrowserError::new(
            "invalid_payload",
            "Browser request exceeds 64KiB",
        ));
    }
    if !matches!(request.action, BrowserAction::Open | BrowserAction::List)
        && request.target.is_none()
    {
        return Err(BrowserError::new(
            "invalid_target",
            "Session, tab and navigation revision are required",
        ));
    }
    if matches!(
        request.action,
        BrowserAction::Open | BrowserAction::Navigate
    ) {
        let url = request
            .payload
            .get("url")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BrowserError::new("invalid_url", "URL is required"))?;
        validate_browser_url(url)?;
    }
    Ok(())
}
/// Shared initial URL validation. Native host additionally guards every actual
/// network loading path, redirects and the runtime's actual control address.
pub fn validate_browser_url(raw: &str) -> Result<url::Url, BrowserError> {
    if raw.len() > 8192 || raw.chars().any(char::is_control) || raw.trim() != raw {
        return Err(BrowserError::new("invalid_url", "URL is too long"));
    }
    let target = url::Url::parse(raw)
        .map_err(|_| BrowserError::new("invalid_url", "Absolute HTTP(S) URL required"))?;
    if !matches!(target.scheme(), "http" | "https")
        || target.host_str().is_none()
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return Err(BrowserError::new(
            "invalid_url",
            "Only HTTP(S) URLs without credentials are allowed",
        ));
    }
    let host = target
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if ["tauri.localhost", "asset.localhost", "ipc.localhost"]
        .iter()
        .any(|internal| host == *internal || host.ends_with(&format!(".{internal}")))
        || (matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1")
            && matches!(target.port_or_known_default(), Some(9420 | 1420)))
    {
        return Err(BrowserError::new(
            "blocked_origin",
            "Application control origins are not browser destinations",
        ));
    }
    Ok(target)
}
#[cfg(test)]
mod tests;
