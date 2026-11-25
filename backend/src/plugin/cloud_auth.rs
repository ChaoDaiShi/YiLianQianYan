//! Service-owned references. Only the SecretStore holds the Bearer token.
use crate::{
    db::Database,
    integrations::secret::{mcp_env_ref, SecretRef, SecretResolver, MAX_SECRET_VALUE_BYTES},
};
use reqwest::header::HeaderValue;
use rusqlite::{Connection, OptionalExtension};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(PartialEq, Eq, Serialize, Deserialize)]
struct Binding {
    origin: String,
    reference: SecretRef,
}

fn key(server_id: &str) -> String {
    format!("plugin_cloud_credential:{server_id}")
}

fn binding_on(conn: &Connection, server_id: &str) -> Result<Option<Binding>, String> {
    let row: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [key(server_id)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "无法读取服务认证绑定")?;
    row.map(|body| serde_json::from_str(&body).map_err(|_| "服务认证绑定无效".into()))
        .transpose()
}

pub(crate) fn new_reference(server_id: &str) -> SecretRef {
    mcp_env_ref(server_id, &format!("cloud-bearer-{}", uuid::Uuid::new_v4()))
}

pub(crate) fn replace_on(
    conn: &Connection,
    server_id: &str,
    origin: &url::Origin,
    reference: SecretRef,
) -> Result<Option<SecretRef>, String> {
    let old = binding_on(conn, server_id)?;
    let binding = Binding {
        origin: origin.ascii_serialization(),
        reference,
    };
    let value = serde_json::to_string(&binding).map_err(|_| "服务认证绑定无效")?;
    conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![key(server_id), value])
        .map_err(|_| "无法保存服务认证绑定")?;
    Ok(old.map(|binding| binding.reference))
}

pub(crate) fn revoke_on(conn: &Connection, server_id: &str) -> Result<Option<SecretRef>, String> {
    let old = binding_on(conn, server_id)?;
    conn.execute("DELETE FROM settings WHERE key=?1", [key(server_id)])
        .map_err(|_| "无法撤销服务认证绑定")?;
    Ok(old.map(|binding| binding.reference))
}

pub(crate) fn bearer_header(token: &str) -> Result<HeaderValue, String> {
    if token.is_empty()
        || token.len() > MAX_SECRET_VALUE_BYTES
        || token.bytes().any(|byte| !byte.is_ascii_graphic())
    {
        return Err("请输入有效的 Bearer 令牌".into());
    }
    let mut header = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| "请输入有效的 Bearer 令牌")?;
    header.set_sensitive(true);
    Ok(header)
}

pub(crate) struct CloudBearer {
    db: Database,
    resolver: Arc<SecretResolver>,
    pub(crate) server_id: String,
    pub(crate) origin: url::Origin,
}

impl CloudBearer {
    pub(crate) fn new(
        db: &Database,
        resolver: Arc<SecretResolver>,
        server_id: String,
        origin: url::Origin,
    ) -> Self {
        Self {
            db: db.clone_connection(),
            resolver,
            server_id,
            origin,
        }
    }

    fn binding(&self) -> Result<Binding, String> {
        binding_on(&self.db.conn(), &self.server_id)?
            .filter(|binding| binding.origin == self.origin.ascii_serialization())
            .ok_or_else(|| "请配置此 HTTPS 来源的服务令牌".into())
    }

    pub(crate) async fn authorization(&self, target: &str) -> Result<HeaderValue, String> {
        let url = url::Url::parse(target).map_err(|_| "服务认证地址无效")?;
        if url.scheme() != "https" || url.origin() != self.origin {
            return Err("服务令牌只允许发送到配置的 HTTPS 来源".into());
        }
        let binding = self.binding()?;
        let token = self
            .resolver
            .resolve_ref(&binding.reference)
            .await
            .map_err(|_| "服务令牌暂不可读取")?
            .ok_or("请配置服务令牌")?;
        if self.binding()? != binding {
            return Err("服务令牌已更新，请重新请求".into());
        }
        bearer_header(token.expose_secret())
    }

    pub(crate) async fn configured(&self) -> bool {
        let Ok(binding) = self.binding() else {
            return false;
        };
        self.resolver
            .resolve_ref(&binding.reference)
            .await
            .ok()
            .flatten()
            .is_some()
    }
}
