use crate::db::Database;
use crate::integrations::secret::{SecretRef, SecretStore};
use rusqlite::{params, OptionalExtension};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

#[derive(Clone, Serialize)]
pub struct BrowserCredentialEntry {
    pub id: String,
    pub label: String,
    pub origin: String,
    pub authorized: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserCredentialInput {
    pub id: Option<String>,
    pub label: String,
    pub origin: String,
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

/// Keeps authorization stable through the native fill. Revocation waits for an
/// already-dispatched fill to finish and prevents all subsequent fills.
pub struct AuthorizedBrowserCredential<'a> {
    secret: SecretString,
    _guard: MutexGuard<'a, ()>,
}

impl AuthorizedBrowserCredential<'_> {
    pub fn expose(&self) -> Result<(String, String), String> {
        let value: Credentials = serde_json::from_str(self.secret.expose_secret())
            .map_err(|_| "密码箱条目无法读取".to_owned())?;
        Ok((value.username, value.password))
    }
}

pub struct BrowserCredentialVault {
    db: Database,
    store: Arc<dyn SecretStore>,
    gate: Mutex<()>,
    allow_test_loopback: bool,
}

const STORAGE_ERROR: &str = "无法访问密码箱记录";
const SECRET_ERROR: &str = "系统凭据存储不可用";

impl BrowserCredentialVault {
    pub(super) fn new(
        db: Database,
        store: Arc<dyn SecretStore>,
        allow_test_loopback: bool,
    ) -> Result<Self, String> {
        db.conn()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS browser_credentials (
                id TEXT PRIMARY KEY, label TEXT NOT NULL, origin TEXT NOT NULL,
                secret_ref TEXT NOT NULL, authorized INTEGER NOT NULL DEFAULT 0
            );
            CREATE UNIQUE INDEX IF NOT EXISTS browser_credential_authorized_origin
                ON browser_credentials(origin) WHERE authorized=1;",
            )
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        Ok(Self {
            db,
            store,
            gate: Mutex::new(()),
            allow_test_loopback,
        })
    }

    fn origin(&self, value: &str) -> Result<String, String> {
        let url = super::validate_browser_url(value.trim()).map_err(|_| "请输入有效的网站地址")?;
        let test_origin = self.allow_test_loopback
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        if url.scheme() != "https" && !test_origin {
            return Err("密码箱只支持 HTTPS 网站".into());
        }
        Ok(url.origin().ascii_serialization())
    }

    pub fn list(&self) -> Result<Vec<BrowserCredentialEntry>, String> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare("SELECT id,label,origin,authorized FROM browser_credentials ORDER BY label,id")
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BrowserCredentialEntry {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    origin: row.get(2)?,
                    authorized: row.get(3)?,
                })
            })
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| STORAGE_ERROR.to_owned())
    }

    fn secret_ref(&self, id: &str) -> Result<String, String> {
        self.db
            .conn()
            .query_row(
                "SELECT secret_ref FROM browser_credentials WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| STORAGE_ERROR.to_owned())?
            .ok_or_else(|| "密码箱条目不存在".into())
    }

    pub async fn save(
        &self,
        input: BrowserCredentialInput,
    ) -> Result<BrowserCredentialEntry, String> {
        let origin = self.origin(&input.origin)?;
        let label = input.label.trim().to_owned();
        if label.is_empty()
            || label.chars().count() > 100
            || input.username.is_empty()
            || input.password.is_empty()
        {
            return Err("请输入条目名称、账号和密码".into());
        }
        let encoded = serde_json::to_string(&Credentials {
            username: input.username,
            password: input.password,
        })
        .map_err(|_| "无法保存凭据")?;
        if encoded.encode_utf16().count() > 1200 {
            return Err("账号和密码超过系统存储长度".into());
        }
        let _guard = self.gate.lock().await;
        let previous = input
            .id
            .as_deref()
            .map(|id| self.secret_ref(id))
            .transpose()?;
        let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let reference = SecretRef::new(format!("browser.vault.{}", uuid::Uuid::new_v4()));
        self.store
            .put(&reference, SecretString::from(encoded))
            .await
            .map_err(|_| SECRET_ERROR.to_owned())?;
        let saved = self.db.conn().execute(
            "INSERT INTO browser_credentials(id,label,origin,secret_ref) VALUES (?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET label=excluded.label, origin=excluded.origin,
                 secret_ref=excluded.secret_ref, authorized=0",
            params![id, label, origin, reference.key],
        );
        if saved.is_err() {
            let _ = self.store.delete(&reference).await;
            return Err(STORAGE_ERROR.into());
        }
        if let Some(previous) = previous {
            if self.store.delete(&SecretRef::new(previous)).await.is_err() {
                tracing::warn!(
                    "Could not remove the replaced browser credential from the system store"
                );
            }
        }
        Ok(BrowserCredentialEntry {
            id,
            label,
            origin,
            authorized: false,
        })
    }

    pub async fn set_authorized(&self, id: &str, authorized: bool) -> Result<(), String> {
        let _guard = self.gate.lock().await;
        let updated = self.db.conn().execute(
            "UPDATE browser_credentials SET authorized=?2 WHERE id=?1",
            params![id, authorized],
        );
        match updated {
            Ok(1) => Ok(()),
            Ok(_) => Err("密码箱条目不存在".into()),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err("这个网站已有已授权的凭据，请先撤销原授权".into())
            }
            Err(_) => Err(STORAGE_ERROR.into()),
        }
    }

    pub async fn delete(&self, id: &str) -> Result<(), String> {
        let _guard = self.gate.lock().await;
        let reference = self.secret_ref(id)?;
        self.db
            .conn()
            .execute(
                "UPDATE browser_credentials SET authorized=0 WHERE id=?1",
                [id],
            )
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        self.store
            .delete(&SecretRef::new(reference))
            .await
            .map_err(|_| SECRET_ERROR.to_owned())?;
        self.db
            .conn()
            .execute("DELETE FROM browser_credentials WHERE id=?1", [id])
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        Ok(())
    }

    pub async fn authorized_for(
        &self,
        origin: &str,
    ) -> Result<AuthorizedBrowserCredential<'_>, String> {
        let origin = self.origin(origin)?;
        let guard = self.gate.lock().await;
        let reference: Option<String> = self
            .db
            .conn()
            .query_row(
                "SELECT secret_ref FROM browser_credentials WHERE origin=?1 AND authorized=1",
                [origin],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        let reference =
            reference.ok_or_else(|| "该网站没有已授权的凭据，请在浏览器密码箱中授权".to_owned())?;
        let secret = self
            .store
            .get(&SecretRef::new(reference))
            .await
            .map_err(|_| SECRET_ERROR.to_owned())?
            .ok_or_else(|| "系统存储中没有此凭据，请重新保存".to_owned())?;
        Ok(AuthorizedBrowserCredential {
            secret,
            _guard: guard,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::secret::InMemorySecretStore;

    #[tokio::test]
    async fn credential_mapping_saves_revokes_and_checks_exact_origin() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let store = Arc::new(InMemorySecretStore::new());
        let vault = BrowserCredentialVault::new(db.clone(), store.clone(), false).unwrap();
        let entry = vault
            .save(BrowserCredentialInput {
                id: None,
                label: "验收条目".into(),
                origin: "https://login.example.test/sign-in".into(),
                username: "fixture-account-value".into(),
                password: "fixture-password-value".into(),
            })
            .await
            .unwrap();
        assert!(!entry.authorized);
        assert_eq!(entry.origin, "https://login.example.test");
        let public = serde_json::to_string(&vault.list().unwrap()).unwrap();
        assert!(!public.contains("fixture-account-value"));
        assert!(!public.contains("fixture-password-value"));
        let persisted: String = db
            .conn()
            .query_row(
                "SELECT label || origin || secret_ref FROM browser_credentials",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!persisted.contains("fixture-password-value"));
        assert!(!persisted.contains("fixture-account-value"));
        assert!(vault.authorized_for(&entry.origin).await.is_err());

        vault.set_authorized(&entry.id, true).await.unwrap();
        assert!(vault
            .authorized_for("https://other.login.example.test")
            .await
            .is_err());
        assert!(vault
            .authorized_for("https://login.example.test:444")
            .await
            .is_err());
        assert!(vault
            .authorized_for("http://login.example.test")
            .await
            .is_err());
        {
            let lease = vault
                .authorized_for("https://login.example.test/another-path")
                .await
                .unwrap();
            assert!(
                lease.expose().unwrap()
                    == (
                        "fixture-account-value".into(),
                        "fixture-password-value".into()
                    )
            );
        }
        vault.set_authorized(&entry.id, false).await.unwrap();
        assert!(vault.authorized_for(&entry.origin).await.is_err());
        assert!(!vault.list().unwrap()[0].authorized);

        let reference = SecretRef::new(vault.secret_ref(&entry.id).unwrap());
        vault.delete(&entry.id).await.unwrap();
        assert!(vault.list().unwrap().is_empty());
        assert!(store.get(&reference).await.unwrap().is_none());
    }
}
