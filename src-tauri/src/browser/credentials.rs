use super::*;
use serde::Deserialize;
use yilian_backend::browser::{BrowserCredentialEntry, BrowserCredentialInput};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserCredentialCommand {
    List,
    Save { entry: BrowserCredentialInput },
    Authorize { id: String, authorized: bool },
    Delete { id: String },
}

#[tauri::command]
pub async fn native_browser_credential_vault(
    view: Webview,
    runtime: tauri::State<'_, Arc<BrowserRuntime>>,
    request: BrowserCredentialCommand,
) -> Result<Vec<BrowserCredentialEntry>, String> {
    if view.label() != "main" && !view.label().starts_with("browser-shell-") {
        return Err("密码箱仅可从浏览器界面管理".into());
    }
    trusted_app_origin(&view)?;
    if runtime.is_quiesced() {
        return Err("浏览器会话已撤销，请重新启动软件".into());
    }
    let vault = runtime.credential_vault()?;
    match request {
        BrowserCredentialCommand::List => {}
        BrowserCredentialCommand::Save { entry } => {
            vault.save(entry).await?;
        }
        BrowserCredentialCommand::Authorize { id, authorized } => {
            vault.set_authorized(&id, authorized).await?
        }
        BrowserCredentialCommand::Delete { id } => vault.delete(&id).await?,
    }
    vault.list()
}

impl NativeBrowserHost {
    pub(super) async fn fill_credential(
        &self,
        view: Webview,
        target: BrowserTarget,
        url: tauri::Url,
        cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or_else(|| error("permission_denied", "密码箱尚未就绪"))?;
        let origin = url.origin().ascii_serialization();
        let host = self.clone();
        // A dispatched evaluation retains its vault lease even if the calling
        // agent task stops awaiting the result.
        tokio::spawn(async move {
            let vault = runtime
                .credential_vault()
                .map_err(|e| error("permission_denied", e))?;
            let credential = vault
                .authorized_for(&origin)
                .await
                .map_err(|e| error("permission_denied", e))?;
            let state = host.lookup(Some(&target))?;
            if cancel.is_cancelled() || runtime.is_quiesced() {
                return Err(error("cancelled_before_execution", "凭据填充已取消"));
            }
            if state.tab.loading
                || !state.visible
                || !view.window().is_visible().unwrap_or(false)
                || view.url().ok().as_ref() != Some(&url)
            {
                return Err(error("stale_navigation", "当前登录页面已经变化"));
            }
            let (username, password) = credential
                .expose()
                .map_err(|e| error("permission_denied", e))?;
            host.update(&target.tab_id, |state| {
                state.credential_filled = true;
                state.observation = None;
            });
            let script = scripts::fill_credential(url.as_str(), &origin, &username, &password);
            let filled = platform::evaluate(&view, script, cancel)
                .await
                .map_err(|_| error("native_action_unknown", "无法确认登录凭据已填入"))?
                .as_bool()
                .unwrap_or(false);
            drop(credential);
            if !filled && !state.credential_filled {
                host.update(&target.tab_id, |state| {
                    if state.tab.navigation_id == target.navigation_id {
                        state.credential_filled = false;
                    }
                });
            }
            Ok(BrowserReply::verified(json!({"filled":filled})))
        })
        .await
        .map_err(|_| error("native_action_unknown", "无法确认登录凭据已填入"))?
    }
}
