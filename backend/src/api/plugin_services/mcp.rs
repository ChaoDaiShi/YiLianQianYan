use super::*;
use crate::integrations::mcp::{McpRuntimeError, McpRuntimeStatus, McpTransportConfig};
use std::sync::Weak;

/// Associates the HTTP adapter with the same instance opened by Browser.
/// The weak server reference keeps the MCP manager outside the service lifetime.
pub(crate) struct PluginMcpBinding {
    pub server: Weak<AppServer>,
    pub plugin: String,
    pub service: String,
    pub instance: Uuid,
    configuration: ServiceConfiguration,
}

impl PluginMcpBinding {
    pub(crate) fn cloud_bearer(&self) -> Option<crate::plugin::cloud_auth::CloudBearer> {
        let server = self.server.upgrade()?;
        credential::bearer(&server, &self.plugin, &self.service, &self.configuration)
    }

    pub(crate) fn is_local(&self) -> bool {
        self.configuration.deployment == ServiceDeployment::Local
    }

    pub(crate) fn current(&self, server: &AppServer) -> bool {
        configuration(server, &self.plugin, &self.service)
            .ok()
            .as_ref()
            == Some(&self.configuration)
            && server
                .plugin_services
                .status(&self.plugin, &self.service)
                .ok()
                .is_some_and(|state| state.instance_id == Some(self.instance))
    }

    pub(crate) async fn acquire(&self) -> Result<TaskServiceUse, McpRuntimeError> {
        let server = self
            .server
            .upgrade()
            .ok_or(McpRuntimeError::ServerNotFound)?;
        let map_error = |(_, body): Error| {
            McpRuntimeError::Transport(
                body.0["error"]
                    .as_str()
                    .unwrap_or("插件服务不可用")
                    .to_owned(),
            )
        };
        let usage = acquire_configured(
            &server,
            &self.plugin,
            &self.service,
            self.instance,
            &self.configuration,
        )
        .map_err(map_error)?;
        wait_for_health(
            &server,
            &self.plugin,
            &self.service,
            &usage,
            &self.configuration,
            false,
        )
        .await
        .map_err(map_error)?;
        if !self.current(&server) {
            return Err(McpRuntimeError::Transport("插件服务配置已变化".into()));
        }
        Ok(usage)
    }

    pub(crate) fn source(
        &self,
        server_id: &str,
        name: &str,
        config: &McpTransportConfig,
    ) -> Option<crate::db::McpServer> {
        let server = self.server.upgrade()?;
        if !self.current(&server) {
            return None;
        }
        let McpTransportConfig::StreamableHttp { url, .. } = config else {
            return None;
        };
        Some(crate::db::McpServer {
            id: server_id.to_owned(),
            name: name.to_owned(),
            transport: "streamable_http".into(),
            command: None,
            args: None,
            url: Some(url.clone()),
            env: None,
            env_secret_refs: BTreeMap::new(),
            enabled: true,
            created_at: 0,
            updated_at: i64::from_be_bytes(
                Sha256::digest(serde_json::to_vec(&self.configuration).ok()?)[..8]
                    .try_into()
                    .ok()?,
            ),
        })
    }
}

pub(super) async fn refresh_if_needed(
    server: &Arc<AppServer>,
    plugin: &str,
    service: &str,
    config: &ServiceConfiguration,
    usage: &TaskServiceUse,
) -> Result<(), Error> {
    let Some(url) = &config.mcp_url else {
        return Ok(());
    };
    let server_id = managed_mcp_id(plugin, service);
    let manager = &server.mcp_runtime_manager;
    if let Some(current) = manager.get_server(&server_id) {
        if current.plugin_instance() == Some(usage.instance_id)
            && current.status == McpRuntimeStatus::Ready
        {
            return Ok(());
        }
    }
    let (definition, _) = resolve(server, plugin, service)?;
    manager.register_plugin_server(
        server_id.clone(),
        definition.title,
        url.clone(),
        PluginMcpBinding {
            server: Arc::downgrade(server),
            plugin: plugin.to_owned(),
            service: service.to_owned(),
            instance: usage.instance_id,
            configuration: config.clone(),
        },
    );
    if manager.refresh_server(&server_id).await.is_err() {
        manager.remove_plugin_server(&server_id);
        let message = "服务健康检查已通过，MCP 握手或目录发现失败";
        server
            .plugin_services
            .health(plugin, service, usage.instance_id, Err(message.into()));
        return Err(error(StatusCode::SERVICE_UNAVAILABLE, message));
    }
    server.invalidate_capability_registry();
    Ok(())
}
