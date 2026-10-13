//! Exact artifact lookup for ProofLoop deliverables.

use super::{Database, ProofLoopDbError};
use rusqlite::params;

impl Database {
    pub fn find_proofloop_artifact_ids(
        &self,
        workspace_id: &str,
        graph_id: &str,
        node_id: &str,
        execution_id: &str,
        deliverable_id: &str,
        title: &str,
    ) -> Result<Vec<String>, ProofLoopDbError> {
        self.initialize_artifact_provenance()
            .map_err(ProofLoopDbError::Integrity)?;
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT a.id
             FROM artifacts a
             JOIN artifact_provenance p ON p.artifact_id = a.id
             JOIN task_node_executions e ON e.execution_id = a.task_execution_id
             WHERE a.workspace_id = ?1
               AND e.graph_id = ?2
               AND e.node_id = ?3
               AND e.execution_id = ?4
               AND e.status = 'succeeded'
               AND json_extract(e.validation_json, '$.status') = 'accepted'
               AND json_extract(p.source_json, '$.kind') = 'node'
               AND json_extract(p.source_json, '$.workspace_id') = ?1
               AND json_extract(p.source_json, '$.graph_id') = ?2
               AND json_extract(p.source_json, '$.node_id') = ?3
               AND json_extract(p.source_json, '$.execution_id') = ?4
               AND (a.id = ?5 OR a.name = ?6)
             ORDER BY (a.id = ?5) DESC, p.version DESC, a.created_at DESC, a.id DESC",
        )?;
        let rows = statement.query_map(
            params![
                workspace_id,
                graph_id,
                node_id,
                execution_id,
                deliverable_id,
                title,
            ],
            |row| row.get(0),
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(ProofLoopDbError::from)
    }
}
