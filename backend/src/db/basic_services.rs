use super::Database;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BasicServiceProfile {
    pub id: String,
    pub label: String,
    pub provider_account_id: String,
    pub service_kind: String,
    pub model: String,
    pub adapter: String,
    pub parameters: serde_json::Value,
    pub revision: u64,
    pub test: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct BasicServiceInput {
    pub label: String,
    pub provider_account_id: String,
    pub service_kind: String,
    pub model: String,
    pub adapter: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
    pub expected_revision: Option<u64>,
}
impl Database {
    pub fn init_basic_services(&self) -> Result<(), String> {
        self.conn().execute_batch("CREATE TABLE IF NOT EXISTS basic_service_profiles(id TEXT PRIMARY KEY,revision INTEGER NOT NULL,payload TEXT NOT NULL);CREATE TABLE IF NOT EXISTS basic_service_defaults(service_kind TEXT PRIMARY KEY,profile_id TEXT NOT NULL)").map_err(|e|e.to_string())
    }
    pub fn list_basic_service_profiles(&self) -> Result<Vec<BasicServiceProfile>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT payload FROM basic_service_profiles ORDER BY rowid")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn get_basic_service_profile(
        &self,
        id: &str,
    ) -> Result<Option<BasicServiceProfile>, String> {
        self.conn()
            .query_row(
                "SELECT payload FROM basic_service_profiles WHERE id=?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .transpose()
    }
    pub fn save_basic_service_profile(
        &self,
        id: &str,
        input: BasicServiceInput,
    ) -> Result<BasicServiceProfile, String> {
        if input.label.trim().is_empty() || input.model.trim().is_empty() {
            return Err("名称和模型不能为空".into());
        }
        if !matches!(input.service_kind.as_str(), "vision" | "image") {
            return Err("服务用途必须是识图或生图".into());
        }
        if input.service_kind == "vision" && input.adapter != "openai"
            || input.service_kind == "image"
                && !matches!(input.adapter.as_str(), "openai" | "dashscope" | "minimax")
        {
            return Err("服务用途与协议不匹配".into());
        }
        if !self
            .list_provider_accounts()?
            .iter()
            .any(|a| a.id == input.provider_account_id)
        {
            return Err("厂商账户不存在".into());
        }
        let old = self.get_basic_service_profile(id)?;
        if old.as_ref().map(|p| p.revision) != input.expected_revision {
            return Err("revision_conflict".into());
        }
        let profile = BasicServiceProfile {
            id: id.into(),
            label: input.label,
            provider_account_id: input.provider_account_id,
            service_kind: input.service_kind,
            model: input.model,
            adapter: input.adapter,
            parameters: input.parameters,
            revision: old.map_or(1, |p| p.revision + 1),
            test: None,
        };
        let payload = serde_json::to_string(&profile).map_err(|e| e.to_string())?;
        let conn = self.conn();
        if let Some(expected) = input.expected_revision {
            if conn.execute("UPDATE basic_service_profiles SET revision=?1,payload=?2 WHERE id=?3 AND revision=?4",params![profile.revision,payload,id,expected]).map_err(|e|e.to_string())?!=1{return Err("revision_conflict".into());}
        } else {
            conn.execute(
                "INSERT INTO basic_service_profiles VALUES(?1,?2,?3)",
                params![id, profile.revision, payload],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(profile)
    }
    pub fn set_basic_service_test(
        &self,
        id: &str,
        revision: u64,
        ok: bool,
        message: &str,
    ) -> Result<(), String> {
        let mut p = self
            .get_basic_service_profile(id)?
            .ok_or("模型档案不存在")?;
        if p.revision != revision {
            return Err("revision_conflict".into());
        }
        p.test = Some(
            serde_json::json!({"revision":revision,"ok":ok,"tested_at":chrono::Utc::now().to_rfc3339(),"message":message}),
        );
        self.conn()
            .execute(
                "UPDATE basic_service_profiles SET payload=?1 WHERE id=?2 AND revision=?3",
                params![
                    serde_json::to_string(&p).map_err(|e| e.to_string())?,
                    id,
                    revision
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn delete_basic_service_profile(&self, id: &str) -> Result<(), String> {
        if self.list_model_pools()?.iter().any(|p| {
            p.tiers
                .iter()
                .any(|t| t.profile_ids.iter().any(|p| p == id))
        }) {
            return Err("服务档案仍被模型池引用，请先移除引用".into());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let count: i64 = tx
            .query_row(
                "SELECT count(*) FROM basic_service_defaults WHERE profile_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if count > 0 {
            return Err("该档案仍是默认服务，请先更换默认配置".into());
        }
        let deleted = tx
            .execute("DELETE FROM basic_service_profiles WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        if deleted > 0 {
            crate::db::connectors::remove_connector_entity_on(&tx, "model", id)
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn set_basic_service_default(&self, kind: &str, id: &str) -> Result<(), String> {
        let p = self
            .get_basic_service_profile(id)?
            .ok_or("服务档案不存在")?;
        if p.service_kind != kind {
            return Err("服务用途不匹配".into());
        }
        self.conn().execute("INSERT INTO basic_service_defaults VALUES(?1,?2) ON CONFLICT(service_kind) DO UPDATE SET profile_id=excluded.profile_id",params![kind,id]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn get_basic_service_default(&self, kind: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(
                "SELECT profile_id FROM basic_service_defaults WHERE service_kind=?1",
                [kind],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
}
impl Database {
    pub fn save_search_provider(
        &self,
        provider: &str,
        base: &str,
        key: Option<&crate::integrations::secret::SecretRef>,
    ) -> Result<(), String> {
        let secret = key
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| e.to_string())?;
        self.conn().execute("INSERT INTO cloud_search_config(singleton,provider,base_url,api_key_ref_json,updated_at) VALUES(1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET provider=excluded.provider,base_url=excluded.base_url,api_key_ref_json=excluded.api_key_ref_json,updated_at=excluded.updated_at",params![provider,base,secret,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
}
impl Database {
    pub fn record_basic_pool_usage(
        &self,
        id: &str,
        usage: &serde_json::Value,
    ) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE model_pool_attempts SET usage=?1 WHERE id=?2",
                params![usage.to_string(), id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
