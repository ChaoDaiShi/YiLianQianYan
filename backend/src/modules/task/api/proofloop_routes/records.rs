//! Export a dossier from persisted attempts, artifact links and actual acceptance snapshots.
use super::*;
use std::collections::{HashMap, HashSet};
pub async fn execution_record_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Response {
    let graph_id = match crate::modules::task::TaskGraphId::new(&id) {
        Ok(v) => v,
        Err(e) => return proof_error_response(ProofError::new("invalid_graph_id", e.to_string())),
    };
    let _graph = match server.task_world.get_graph(&graph_id) {
        Some(v) => v,
        None => return proof_error_response(ProofError::new("graph_not_found", "任务图不存在")),
    };
    let execution_ids = (|| -> Result<Vec<String>, String> {
        let conn = server.db.conn();
        let mut query=conn.prepare("SELECT execution_id FROM task_node_executions WHERE graph_id=?1 ORDER BY created_at,execution_id").map_err(|e|e.to_string())?;
        let rows = query
            .query_map([id.as_str()], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })();
    let execution_ids = match execution_ids {
        Ok(v) => v,
        Err(e) => return proof_error_response(ProofError::new("execution_history_unavailable", e)),
    };
    let mut executions = Vec::new();
    for id in execution_ids {
        let execution_id = match crate::modules::task::NodeExecutionId::new(&id) {
            Ok(v) => v,
            Err(e) => {
                return proof_error_response(ProofError::new(
                    "execution_history_unavailable",
                    e.to_string(),
                ))
            }
        };
        match server.db.load_node_execution(&execution_id) {
            Ok(Some(value)) => executions.push(value),
            Ok(None) => {}
            Err(e) => {
                return proof_error_response(ProofError::new(
                    "execution_history_unavailable",
                    e.to_string(),
                ))
            }
        }
    }
    if let Err(e) = server.db.initialize_proofloop_eval_schema() {
        return proof_error_response(ProofError::new(
            "execution_history_unavailable",
            e.to_string(),
        ));
    }
    let mut artifacts: HashMap<String, Vec<String>> = HashMap::new();
    let mut reports: HashMap<String, Vec<String>> = HashMap::new();
    let mut revision_sets: HashMap<String, HashSet<u64>> = HashMap::new();
    let collected = (|| -> Result<(), String> {
        let conn = server.db.conn();
        for execution in &executions {
            let mut query = conn
                .prepare("SELECT id FROM artifacts WHERE task_execution_id=?1 ORDER BY id")
                .map_err(|e| e.to_string())?;
            let rows = query
                .query_map([execution.id.as_str()], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            artifacts.insert(
                execution.id.to_string(),
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?,
            );
        }
        let mut query=conn.prepare("SELECT report_id,evidence_json FROM proofloop_acceptance_reports WHERE graph_id=?1 ORDER BY created_at,report_id").map_err(|e|e.to_string())?;
        let rows = query
            .query_map([id.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (report_id, evidence) = row.map_err(|e| e.to_string())?;
            let snapshot: crate::modules::task::proofloop::EvidenceSnapshot =
                serde_json::from_str(&evidence).map_err(|e| e.to_string())?;
            for execution in snapshot.executions {
                reports
                    .entry(execution.execution_id.clone())
                    .or_default()
                    .push(report_id.clone());
                revision_sets
                    .entry(execution.execution_id)
                    .or_default()
                    .insert(execution.graph_revision);
            }
        }
        Ok(())
    })();
    if let Err(e) = collected {
        return proof_error_response(ProofError::new("execution_history_unavailable", e));
    }
    let revisions = revision_sets
        .into_iter()
        .filter_map(|(id, set)| {
            if set.len() == 1 {
                Some((id, *set.iter().next().unwrap()))
            } else {
                None
            }
        })
        .collect();
    let records = crate::modules::task::execution_record::project_execution_record(
        &executions,
        &artifacts,
        &reports,
        &revisions,
    );
    Json(json!({"graph_id":id,"records":records,"revision_note":"graph_revision来自已保存的验收快照；无唯一证据时为null"})).into_response()
}
