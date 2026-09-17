use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::task::{artifact::ArtifactSource, Artifact};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

pub(crate) fn validate_completed_source(
    conn: &rusqlite::Connection,
    source: &ArtifactSource,
) -> Result<(), String> {
    let valid = match source {
        ArtifactSource::Task { task_id, execution_id } => conn.query_row("SELECT EXISTS(SELECT 1 FROM tasks t JOIN task_executions e ON e.task_id=t.id WHERE t.id=?1 AND e.id=?2 AND t.status='completed' AND e.status='completed')", params![task_id,execution_id], |row| row.get::<_, bool>(0)).map_err(|_| "任务证据不可用")?,
        ArtifactSource::Node { graph_id, node_id, execution_id, .. } => {
            let row: Option<(String,Option<String>)> = conn.query_row("SELECT status,validation_json FROM task_node_executions WHERE execution_id=?1 AND graph_id=?2 AND node_id=?3", params![execution_id,graph_id,node_id], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(|_| "执行证据不可用")?;
            row.is_some_and(|(status,validation)| status == "succeeded" && validation.and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok()).is_some_and(|v| v["status"] == "accepted"))
        }
    };
    if valid {
        Ok(())
    } else {
        Err("只接受已完成且通过验证的执行证据".into())
    }
}

const ARTIFACT_PROVENANCE: ProductMigrationSpec = ProductMigrationSpec::new(1011, "1011_artifact_provenance", MigrationOwner::V1TaskWorld,
    "CREATE TABLE artifact_provenance (artifact_id TEXT PRIMARY KEY REFERENCES artifacts(id), source_key TEXT NOT NULL, source_json TEXT NOT NULL, version INTEGER NOT NULL CHECK(version>0), sha256 TEXT NOT NULL, created_at INTEGER NOT NULL, UNIQUE(source_key,version));");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactProvenance {
    pub artifact_id: String,
    pub source: ArtifactSource,
    pub version: u32,
    pub sha256: String,
    pub created_at: i64,
}
impl Database {
    pub fn initialize_artifact_provenance(&self) -> Result<(), String> {
        self.apply_product_migration(&ARTIFACT_PROVENANCE)
            .map_err(|error| error.to_string())
    }
    pub fn create_materialized_artifact(
        &self,
        artifact: &Artifact,
        source: &ArtifactSource,
        hash: &str,
    ) -> Result<ArtifactProvenance, String> {
        self.initialize_artifact_provenance()?;
        let source_json = serde_json::to_string(source).map_err(|_| "产物来源无效")?;
        let source_key = format!("{source_json}|{}", artifact.name);
        let mut conn = self.conn();
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "产物事务不可用")?;
        validate_completed_source(&transaction, source)?;
        let current: u32 = transaction
            .query_row(
                "SELECT COALESCE(MAX(version),0) FROM artifact_provenance WHERE source_key=?1",
                [&source_key],
                |row| row.get(0),
            )
            .map_err(|_| "产物版本不可用")?;
        let version = current.checked_add(1).ok_or("产物版本达到上限")?;
        transaction.execute("INSERT INTO artifacts(id,workspace_id,task_id,task_execution_id,name,artifact_type,path,mime_type,size,summary,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![artifact.id.as_str(),artifact.workspace_id.as_str(),artifact.task_id.as_str(),artifact.task_execution_id.as_str(),artifact.name,artifact.artifact_type.to_string(),artifact.path,artifact.mime_type,artifact.size,artifact.summary,artifact.created_at,artifact.updated_at]).map_err(|_| "产物记录保存失败")?;
        transaction.execute("INSERT INTO artifact_provenance(artifact_id,source_key,source_json,version,sha256,created_at) VALUES (?1,?2,?3,?4,?5,?6)", params![artifact.id.as_str(),source_key,source_json,version,hash,artifact.created_at]).map_err(|_| "产物来源保存失败")?;
        transaction.commit().map_err(|_| "产物提交失败")?;
        Ok(ArtifactProvenance {
            artifact_id: artifact.id.to_string(),
            source: source.clone(),
            version,
            sha256: hash.into(),
            created_at: artifact.created_at,
        })
    }
    pub fn artifact_provenance(&self, id: &str) -> Result<Option<ArtifactProvenance>, String> {
        self.initialize_artifact_provenance()?;
        let row: Option<(String,u32,String,i64)> = self.conn().query_row("SELECT source_json,version,sha256,created_at FROM artifact_provenance WHERE artifact_id=?1", [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(|_| "产物来源读取失败")?;
        row.map(|(source, version, sha256, created_at)| {
            Ok(ArtifactProvenance {
                artifact_id: id.into(),
                source: serde_json::from_str(&source).map_err(|_| "产物来源无效")?,
                version,
                sha256,
                created_at,
            })
        })
        .transpose()
    }
    pub fn validate_completed_evidence(&self, source: &ArtifactSource) -> Result<(), String> {
        validate_completed_source(&self.conn(), source)
    }
}
