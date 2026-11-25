use super::*;
use crate::plugin::cloud_auth::{self, CloudBearer};
use secrecy::SecretString;

pub(crate) fn bearer(
    server: &AppServer,
    plugin: &str,
    id: &str,
    config: &ServiceConfiguration,
) -> Option<CloudBearer> {
    Some(CloudBearer::new(
        &server.db,
        Arc::clone(&server.secret_resolver),
        managed_mcp_id(plugin, id),
        config.bearer_origin()?,
    ))
}

fn checked_origin(
    server: &AppServer,
    plugin: &str,
    id: &str,
    expected: &str,
) -> Result<url::Origin, Error> {
    let config = configuration(server, plugin, id)?;
    if revision(&config) != expected {
        return Err(error(
            StatusCode::CONFLICT,
            "服务配置已变化，请重新核对服务来源",
        ));
    }
    config
        .bearer_origin()
        .ok_or_else(|| invalid("该服务未配置云端 Bearer 认证地址"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SaveCredential {
    configuration_revision: String,
    token: String,
}

pub(super) async fn save(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id)): Path<(String, String)>,
    Json(request): Json<SaveCredential>,
) -> Result<StatusCode, Error> {
    let origin = {
        let _operation = server.plugin_services.operations.lock();
        checked_origin(&server, &plugin, &id, &request.configuration_revision)?
    };
    let token = request.token.trim();
    cloud_auth::bearer_header(token).map_err(invalid)?;
    let server_id = managed_mcp_id(&plugin, &id);
    let reference = cloud_auth::new_reference(&server_id);
    server
        .secret_store
        .put(&reference, SecretString::from(token.to_owned()))
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "无法将服务令牌写入系统密钥库",
            )
        })?;

    let committed = (|| {
        let _operation = server.plugin_services.operations.lock();
        checked_origin(&server, &plugin, &id, &request.configuration_revision)?;
        let old = cloud_auth::replace_on(&server.db.conn(), &server_id, &origin, reference.clone())
            .map_err(invalid)?;
        server.mcp_runtime_manager.remove_plugin_server(&server_id);
        server.invalidate_capability_registry();
        Ok::<_, Error>(old)
    })();
    let old = match committed {
        Ok(previous) => previous,
        Err(error) => {
            if server.secret_store.delete(&reference).await.is_err() {
                tracing::warn!(%server_id, "Unused cloud service credential could not be removed");
            }
            return Err(error);
        }
    };
    if let Some(old) = old {
        server.secret_store.delete(&old).await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "新令牌已启用，旧密钥库条目未能清理",
            )
        })?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevokeCredential {
    configuration_revision: String,
}

pub(super) async fn revoke(
    State(server): State<Arc<AppServer>>,
    Path((plugin, id)): Path<(String, String)>,
    Json(request): Json<RevokeCredential>,
) -> Result<StatusCode, Error> {
    let reference = {
        let _operation = server.plugin_services.operations.lock();
        checked_origin(&server, &plugin, &id, &request.configuration_revision)?;
        let server_id = managed_mcp_id(&plugin, &id);
        let reference = cloud_auth::revoke_on(&server.db.conn(), &server_id).map_err(invalid)?;
        server.mcp_runtime_manager.remove_plugin_server(&server_id);
        server.invalidate_capability_registry();
        reference
    };
    if let Some(reference) = reference {
        server.secret_store.delete(&reference).await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "服务令牌已撤销，系统密钥库条目未能清理",
            )
        })?;
    }
    Ok(StatusCode::NO_CONTENT)
}
