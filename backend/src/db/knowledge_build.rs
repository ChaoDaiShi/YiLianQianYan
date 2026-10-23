use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::modules::knowledge_build::KnowledgeBuild;
use rusqlite::{params, OptionalExtension};

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1044, "1044_knowledge_builds", MigrationOwner::V1TaskWorld,
    "CREATE TABLE knowledge_builds(id TEXT PRIMARY KEY,status TEXT NOT NULL,payload TEXT NOT NULL,updated_at INTEGER NOT NULL);
     CREATE INDEX knowledge_builds_updated ON knowledge_builds(updated_at DESC);",
);

impl Database {
    pub fn initialize_knowledge_build_schema(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&SCHEMA)
    }

    pub fn create_knowledge_build(&self, build: &KnowledgeBuild) -> Result<(), String> {
        let payload = serde_json::to_string(build).map_err(|e| e.to_string())?;
        self.conn()
            .execute(
                "INSERT INTO knowledge_builds(id,status,payload,updated_at) VALUES(?1,?2,?3,?4)",
                params![build.id, build.status, payload, build.updated_at],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn knowledge_build(&self, id: &str) -> Result<Option<KnowledgeBuild>, String> {
        let payload: Option<String> = self
            .conn()
            .query_row(
                "SELECT payload FROM knowledge_builds WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        payload
            .map(|p| serde_json::from_str(&p).map_err(|e| e.to_string()))
            .transpose()
    }

    pub fn knowledge_build_for_document(
        &self,
        document_id: &str,
    ) -> Result<Option<String>, String> {
        self.conn().query_row("SELECT id FROM knowledge_builds WHERE status='completed' AND json_extract(payload,'$.document_id')=?1 LIMIT 1", [document_id], |row|row.get(0))
            .optional().map_err(|error|error.to_string())
    }

    pub fn knowledge_builds(&self) -> Result<Vec<KnowledgeBuild>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT payload FROM knowledge_builds ORDER BY updated_at DESC,id LIMIT 100")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut builds = Vec::new();
        for row in rows {
            builds.push(
                serde_json::from_str(&row.map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(builds)
    }

    /// A worker cannot overwrite cancellation or startup interruption with a late response.
    pub fn update_active_knowledge_build(&self, build: &KnowledgeBuild) -> Result<bool, String> {
        let payload = serde_json::to_string(build).map_err(|e| e.to_string())?;
        Ok(self.conn().execute(
            "UPDATE knowledge_builds SET status=?2,payload=?3,updated_at=?4 WHERE id=?1 AND status IN ('queued','researching','synthesizing')",
            params![build.id, build.status, payload, build.updated_at]).map_err(|e| e.to_string())? > 0)
    }

    pub fn cancel_knowledge_build(&self, id: &str) -> Result<Option<KnowledgeBuild>, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let payload: Option<String> = tx
            .query_row(
                "SELECT payload FROM knowledge_builds WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let mut build: KnowledgeBuild =
            serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        if build.is_active() {
            build.status = "cancelled".into();
            build.progress.stage = "已取消".into();
            build.updated_at = chrono::Utc::now().timestamp_millis();
            tx.execute("UPDATE knowledge_builds SET status='cancelled',payload=?2,updated_at=?3 WHERE id=?1",
                params![id, serde_json::to_string(&build).map_err(|e| e.to_string())?, build.updated_at]).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Some(build))
    }

    /// Called once at application startup, not on connection clones or individual requests.
    pub fn interrupt_knowledge_builds(&self) -> Result<usize, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let rows = {
            let mut stmt = tx.prepare("SELECT payload FROM knowledge_builds WHERE status IN ('queued','researching','synthesizing')")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        let count = rows.len();
        for payload in rows {
            let mut build: KnowledgeBuild =
                serde_json::from_str(&payload).map_err(|e| e.to_string())?;
            build.status = "interrupted".into();
            build.progress.stage = "应用重启，构建已中断".into();
            build.error = Some("应用在构建期间退出。已保留检索证据，请重新创建构建。".into());
            build.updated_at = chrono::Utc::now().timestamp_millis();
            tx.execute("UPDATE knowledge_builds SET status='interrupted',payload=?2,updated_at=?3 WHERE id=?1",
                params![build.id, serde_json::to_string(&build).map_err(|e| e.to_string())?, build.updated_at]).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub fn set_knowledge_build_document(
        &self,
        id: &str,
        document_id: &str,
    ) -> Result<KnowledgeBuild, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let payload: String = tx
            .query_row(
                "SELECT payload FROM knowledge_builds WHERE id=?1 AND status='completed'",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut build: KnowledgeBuild =
            serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        build.document_id = Some(document_id.into());
        build.updated_at = chrono::Utc::now().timestamp_millis();
        tx.execute(
            "UPDATE knowledge_builds SET payload=?2,updated_at=?3 WHERE id=?1",
            params![
                id,
                serde_json::to_string(&build).map_err(|e| e.to_string())?,
                build.updated_at
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(build)
    }
}
