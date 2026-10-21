use super::Database;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPoolTier {
    pub id: String,
    pub label: String,
    pub profile_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPool {
    pub id: String,
    pub label: String,
    pub service_kind: String,
    pub enabled: bool,
    pub revision: u64,
    pub attempt_timeout_ms: u64,
    pub total_timeout_ms: u64,
    pub tiers: Vec<ModelPoolTier>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPoolInput {
    pub label: String,
    pub service_kind: String,
    pub enabled: bool,
    pub attempt_timeout_ms: u64,
    pub total_timeout_ms: u64,
    pub tiers: Vec<ModelPoolTier>,
    pub expected_revision: Option<u64>,
}
impl Database {
    pub fn init_model_pools(&self) -> Result<(), String> {
        self.conn().execute_batch("CREATE TABLE IF NOT EXISTS model_pool_route_state(pool_id TEXT PRIMARY KEY,tier INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS model_pool_attempts(id TEXT PRIMARY KEY,request_id TEXT NOT NULL,model_id TEXT NOT NULL,tier INTEGER NOT NULL,started_at INTEGER NOT NULL,ended_at INTEGER,failure TEXT,usage TEXT); CREATE TABLE IF NOT EXISTS model_pools(id TEXT PRIMARY KEY, revision INTEGER NOT NULL, payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS model_pool_bindings(subject_id TEXT PRIMARY KEY, pool_id TEXT NOT NULL REFERENCES model_pools(id));").map_err(|e|e.to_string())
    }
    pub fn list_model_pools(&self) -> Result<Vec<ModelPool>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT payload FROM model_pools ORDER BY rowid")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn get_model_pool(&self, id: &str) -> Result<Option<ModelPool>, String> {
        self.conn()
            .query_row("SELECT payload FROM model_pools WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .transpose()
    }
    pub fn save_model_pool(&self, id: &str, input: ModelPoolInput) -> Result<ModelPool, String> {
        if input.label.trim().is_empty()
            || !matches!(input.service_kind.as_str(), "chat" | "vision" | "image")
        {
            return Err("模型池用途必须是对话、识图或生图，名称不能为空".into());
        }
        if input.attempt_timeout_ms == 0 || input.total_timeout_ms < input.attempt_timeout_ms {
            return Err("总时限必须不小于候选时限".into());
        }
        let mut seen = std::collections::HashSet::new();
        for tier in &input.tiers {
            if tier.profile_ids.is_empty() {
                return Err("每层至少需要一个模型".into());
            }
            for profile in &tier.profile_ids {
                if !seen.insert(profile) {
                    return Err("同一模型在池中只能出现一次".into());
                }
                if input.service_kind == "chat" {
                    if self.get_llm_model(profile)?.is_none() {
                        return Err("候选模型不存在".into());
                    }
                } else {
                    let p = self
                        .get_basic_service_profile(profile)?
                        .ok_or("候选服务档案不存在")?;
                    if p.service_kind != input.service_kind {
                        return Err("模型用途与池不匹配".into());
                    }
                }
            }
        }
        if seen.is_empty() {
            return Err("模型池至少需要一个候选".into());
        }
        let old = self.get_model_pool(id)?;
        if old.as_ref().map(|p| p.revision) != input.expected_revision {
            return Err("revision_conflict".into());
        }
        let pool = ModelPool {
            id: id.into(),
            label: input.label,
            service_kind: input.service_kind,
            enabled: input.enabled,
            revision: old.map_or(1, |p| p.revision + 1),
            attempt_timeout_ms: input.attempt_timeout_ms,
            total_timeout_ms: input.total_timeout_ms,
            tiers: input.tiers,
        };
        let payload = serde_json::to_string(&pool).map_err(|e| e.to_string())?;
        let conn = self.conn();
        if let Some(revision) = input.expected_revision {
            if conn
                .execute(
                    "UPDATE model_pools SET revision=?1,payload=?2 WHERE id=?3 AND revision=?4",
                    params![pool.revision, payload, id, revision],
                )
                .map_err(|e| e.to_string())?
                != 1
            {
                return Err("revision_conflict".into());
            }
        } else {
            conn.execute(
                "INSERT INTO model_pools VALUES(?1,?2,?3)",
                params![id, pool.revision, payload],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(pool)
    }
    pub fn delete_model_pool(&self, id: &str, revision: u64) -> Result<(), String> {
        let conn = self.conn();
        let connector_count:i64=conn.query_row("SELECT count(*) FROM connector_bindings WHERE model_source='pool' AND target_id=?1",[format!("model:{id}")],|r|r.get(0)).map_err(|e|e.to_string())?;
        if connector_count > 0 {
            return Err("模型池仍被连接器引用，请先解除绑定".into());
        }
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM model_pool_bindings WHERE pool_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if count > 0 {
            return Err("模型池仍被默认配置或智能体引用，请先解除绑定".into());
        }
        if conn
            .execute(
                "DELETE FROM model_pools WHERE id=?1 AND revision=?2",
                params![id, revision],
            )
            .map_err(|e| e.to_string())?
            != 1
        {
            return Err("revision_conflict".into());
        }
        Ok(())
    }
    pub fn set_model_pool_binding(
        &self,
        subject: &str,
        pool_id: Option<&str>,
    ) -> Result<(), String> {
        if let Some(id) = pool_id {
            let pool = self.get_model_pool(id)?.ok_or("模型池不存在")?;
            if !pool.enabled {
                return Err("模型池未启用".into());
            }
            self.conn().execute("INSERT INTO model_pool_bindings VALUES(?1,?2) ON CONFLICT(subject_id) DO UPDATE SET pool_id=excluded.pool_id",params![subject,id]).map_err(|e|e.to_string())?;
        } else {
            self.conn()
                .execute(
                    "DELETE FROM model_pool_bindings WHERE subject_id=?1",
                    [subject],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn get_model_pool_binding(&self, subject: &str) -> Result<Option<String>, String> {
        self.conn()
            .query_row(
                "SELECT pool_id FROM model_pool_bindings WHERE subject_id=?1",
                [subject],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAccount {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub credential_id: Option<String>,
    pub id: String,
    pub provider_id: String,
    pub label: String,
    pub profile_ids: Vec<String>,
}
impl Database {
    pub fn init_provider_accounts(&self) -> Result<(), String> {
        self.conn().execute_batch("CREATE TABLE IF NOT EXISTS provider_accounts(id TEXT PRIMARY KEY,payload TEXT NOT NULL)").map_err(|e|e.to_string())
    }
    pub fn list_provider_accounts(&self) -> Result<Vec<ProviderAccount>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT payload FROM provider_accounts ORDER BY rowid")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn validate_provider_account(&self, account: &ProviderAccount) -> Result<(), String> {
        if account.label.trim().is_empty() || account.provider_id.trim().is_empty() {
            return Err("厂商和账户名称不能为空".into());
        }
        let accounts = self.list_provider_accounts()?;
        if accounts.iter().any(|other| {
            other.id != account.id
                && other
                    .profile_ids
                    .iter()
                    .any(|id| account.profile_ids.contains(id))
        }) {
            return Err("模型档案已属于其他账户".into());
        }
        for id in &account.profile_ids {
            let model = self.get_llm_model(id)?.ok_or("账户引用的模型不存在")?;
            if model.provider != account.provider_id {
                return Err("模型与账户的厂商不匹配".into());
            }
        }
        Ok(())
    }
    pub fn save_provider_account(&self, account: &ProviderAccount) -> Result<(), String> {
        self.validate_provider_account(account)?;
        let payload = serde_json::to_string(account).map_err(|e| e.to_string())?;
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO provider_accounts VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![account.id,payload]).map_err(|e|e.to_string())?;
        if let Some(key) = &account.credential_id {
            for id in &account.profile_ids {
                tx.execute("UPDATE llm_models SET provider=?1,base_url=?2,api_key_ref=?3,api_key_env='',verified_at=NULL,last_error=NULL,updated_at=?4 WHERE id=?5",params![account.provider_id,account.base_url,key,chrono::Utc::now().timestamp_millis(),id]).map_err(|e|e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn delete_provider_account(&self, id: &str) -> Result<(), String> {
        if self
            .list_basic_service_profiles()?
            .iter()
            .any(|p| p.provider_account_id == id)
        {
            return Err("账户仍被识图或生图档案引用，请先移除引用".into());
        }
        if self
            .list_provider_accounts()?
            .iter()
            .any(|a| a.id == id && !a.profile_ids.is_empty())
        {
            return Err("账户仍有模型档案，请先移除或重新分配档案".into());
        }
        if let Some(account) = self
            .list_provider_accounts()?
            .into_iter()
            .find(|a| a.id == id)
        {
            if let Some(key) = account.credential_id {
                if self.list_llm_models()?.iter().any(|m| m.api_key_ref == key) {
                    return Err("仍有模型使用账户凭据，请先删除模型或重新分配账户".into());
                }
            }
        }
        self.conn()
            .execute("DELETE FROM provider_accounts WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    fn setup() -> Database {
        let path = std::env::temp_dir().join(format!("yilian-pools-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&path).unwrap();
        db.init_model_pools().unwrap();
        db.init_provider_accounts().unwrap();
        for id in ["first", "second"] {
            db.create_llm_model(&crate::db::LlmModelInput {
                id: id.into(),
                provider: "test".into(),
                label: id.into(),
                model: id.into(),
                base_url: "http://127.0.0.1:1/v1".into(),
                api_format: "openai".into(),
                api_key_ref: format!("llm.{id}"),
                api_key_env: String::new(),
                temperature: 0.0,
                max_tokens: 50,
                invoke_timeout_ms: 1000,
            })
            .unwrap();
        }
        db
    }
    fn input() -> ModelPoolInput {
        ModelPoolInput {
            label: "test".into(),
            service_kind: "chat".into(),
            enabled: true,
            attempt_timeout_ms: 100,
            total_timeout_ms: 500,
            tiers: vec![ModelPoolTier {
                id: "tier1".into(),
                label: "1".into(),
                profile_ids: vec!["first".into(), "second".into()],
            }],
            expected_revision: None,
        }
    }
    #[test]
    fn pool_round_trip_order_revision_and_bindings() {
        let db = setup();
        let first = db.save_model_pool("pool", input()).unwrap();
        assert_eq!(first.tiers[0].profile_ids, vec!["first", "second"]);
        let mut update = input();
        assert_eq!(
            db.save_model_pool("pool", update.clone()).unwrap_err(),
            "revision_conflict"
        );
        update.expected_revision = Some(1);
        update.tiers[0].profile_ids.reverse();
        assert_eq!(db.save_model_pool("pool", update).unwrap().revision, 2);
        db.set_model_pool_binding("default", Some("pool")).unwrap();
        assert!(db.delete_model_pool("pool", 2).is_err());
        db.set_model_pool_binding("default", None).unwrap();
        db.delete_model_pool("pool", 2).unwrap();
        assert!(db.get_model_pool("pool").unwrap().is_none());
    }
    #[test]
    fn account_projects_one_credential_to_multiple_models_and_prevents_duplicate_ownership() {
        let db = setup();
        let account = ProviderAccount {
            id: "account".into(),
            provider_id: "test".into(),
            label: "test account".into(),
            base_url: "https://example.invalid/v1".into(),
            credential_id: Some("llm.account.test".into()),
            profile_ids: vec!["first".into(), "second".into()],
        };
        db.save_provider_account(&account).unwrap();
        for id in ["first", "second"] {
            let row = db.get_llm_model(id).unwrap().unwrap();
            assert_eq!(row.api_key_ref, "llm.account.test");
            assert_eq!(row.base_url, "https://example.invalid/v1");
        }
        let mut other = account.clone();
        other.id = "other".into();
        assert!(db.save_provider_account(&other).is_err());
        assert!(db.delete_provider_account("account").is_err());
    }
    #[test]
    fn rejects_duplicate_missing_and_wrong_use() {
        let db = setup();
        let mut i = input();
        i.tiers[0].profile_ids.push("first".into());
        assert!(db.save_model_pool("pool", i).is_err());
        let mut i = input();
        i.tiers[0].profile_ids = vec!["missing".into()];
        assert!(db.save_model_pool("pool", i).is_err());
        let mut i = input();
        i.service_kind = "embedding".into();
        assert!(db.save_model_pool("pool", i).is_err());
    }
}
impl Database {
    pub fn begin_pool_attempt(
        &self,
        id: &str,
        request: &str,
        model_id: &str,
        tier: usize,
    ) -> Result<(), String> {
        self.conn().execute("INSERT INTO model_pool_attempts(id,request_id,model_id,tier,started_at) VALUES(?1,?2,?3,?4,?5)",params![id,request,model_id,tier,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn finish_pool_attempt(&self, id: &str, failure: Option<&str>) -> Result<(), String> {
        self.conn()
            .execute(
                "UPDATE model_pool_attempts SET ended_at=?1,failure=?2 WHERE id=?3",
                params![chrono::Utc::now().timestamp_millis(), failure, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn record_pool_attempt_usage(
        &self,
        id: &str,
        input: &crate::db::LlmUsageInput,
    ) -> Result<(), String> {
        let conn = self.conn();
        conn.execute("UPDATE model_pool_attempts SET usage=?1 WHERE id=?2",params![serde_json::json!({"prompt_tokens":input.prompt_tokens,"completion_tokens":input.completion_tokens,"total_tokens":input.total_tokens}).to_string(),id]).map_err(|e|e.to_string())?;
        conn.execute("INSERT INTO llm_usage_events(id,model_id,provider,model,prompt_tokens,completion_tokens,total_tokens,recorded_at,source) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET prompt_tokens=excluded.prompt_tokens,completion_tokens=excluded.completion_tokens,total_tokens=excluded.total_tokens",params![id,input.model_id,input.provider,input.model,input.prompt_tokens,input.completion_tokens,input.total_tokens,input.recorded_at,input.source]).map_err(|e|e.to_string())?;
        Ok(())
    }
}
impl Database {
    pub fn list_pool_attempts(
        &self,
        request_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let conn = self.conn();
        let mut stmt=conn.prepare("SELECT id,request_id,model_id,tier,started_at,ended_at,failure,usage FROM model_pool_attempts WHERE (?1 IS NULL OR request_id=?1) ORDER BY started_at DESC,rowid DESC LIMIT 500").map_err(|e|e.to_string())?;
        let rows=stmt.query_map([request_id],|r|{let usage:Option<String>=r.get(7)?;Ok(serde_json::json!({"attempt_id":r.get::<_,String>(0)?,"request_id":r.get::<_,String>(1)?,"profile_id":r.get::<_,String>(2)?,"tier":r.get::<_,usize>(3)?,"started_at":r.get::<_,i64>(4)?,"ended_at":r.get::<_,Option<i64>>(5)?,"failure":r.get::<_,Option<String>>(6)?,"usage":usage.and_then(|s|serde_json::from_str::<serde_json::Value>(&s).ok())}))}).map_err(|e|e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}
impl Database {
    pub fn model_pool_last_tier(&self, id: &str) -> Result<usize, String> {
        Ok(self
            .conn()
            .query_row(
                "SELECT tier FROM model_pool_route_state WHERE pool_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or(0))
    }
    pub fn set_model_pool_last_tier(&self, id: &str, tier: usize) -> Result<(), String> {
        self.conn().execute("INSERT INTO model_pool_route_state VALUES(?1,?2) ON CONFLICT(pool_id) DO UPDATE SET tier=excluded.tier",params![id,tier]).map_err(|e|e.to_string())?;
        Ok(())
    }
}
