// ============================================================
// Workflow runtime persistence — workflow_graphs + workflow_runs.
//
// Separate from the legacy `workflows` (prompt-template) table. The executable
// DAG definition and run state are stored here. Graph definitions are
// validated before they are written; a run snapshots its definition so that
// later edits to the source graph never change an in-flight run.
// ============================================================

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::Database;
use crate::execution::{ExecutionContext, ExecutionId};
use crate::workflow::{
    NodeRunState, WorkflowGraphDefinition, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
};

/// A persisted executable workflow graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowGraphRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub definition: WorkflowGraphDefinition,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A persisted workflow run together with the graph id it was launched from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredWorkflowRun {
    pub workflow_graph_id: String,
    pub run: WorkflowRun,
}

fn encode_json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn decode_json<T: DeserializeOwned>(value: &str) -> Result<T, String> {
    serde_json::from_str(value).map_err(|error| error.to_string())
}

fn run_status_from_str(value: &str) -> Result<WorkflowRunStatus, String> {
    match value {
        "created" => Ok(WorkflowRunStatus::Created),
        "running" => Ok(WorkflowRunStatus::Running),
        "waiting_approval" => Ok(WorkflowRunStatus::WaitingApproval),
        "completed" => Ok(WorkflowRunStatus::Completed),
        "failed" => Ok(WorkflowRunStatus::Failed),
        "cancelled" => Ok(WorkflowRunStatus::Cancelled),
        other => Err(format!("unknown workflow run status: {other}")),
    }
}

fn reconstruct_run(
    run_id: String,
    execution_id: String,
    subject_id: String,
    agent_name: String,
    parent_execution_id: Option<String>,
    status: String,
    definition_json: String,
    state_json: String,
    created_at: i64,
    updated_at: i64,
) -> Result<WorkflowRun, String> {
    let run_id = WorkflowRunId::new(run_id).map_err(|error| error.to_string())?;
    let execution_id = ExecutionId::new(execution_id).map_err(|error| error.to_string())?;
    let parent_execution_id = parent_execution_id
        .map(ExecutionId::new)
        .transpose()
        .map_err(|error| error.to_string())?;
    // The execution context shares the run's creation timestamp.
    let execution_context = ExecutionContext::new(
        execution_id,
        subject_id,
        agent_name,
        parent_execution_id,
        created_at,
    );
    let definition = decode_json::<WorkflowGraphDefinition>(&definition_json)?;
    let node_states = decode_json::<Vec<NodeRunState>>(&state_json)?;
    let status = run_status_from_str(&status)?;

    Ok(WorkflowRun {
        run_id,
        execution_context,
        definition,
        status,
        node_states,
        created_at,
        updated_at,
    })
}

impl Database {
    // ---- workflow_graphs -------------------------------------------------

    pub fn create_workflow_graph(&self, record: &WorkflowGraphRecord) -> Result<(), String> {
        record
            .definition
            .validate()
            .map_err(|error| format!("invalid workflow graph: {error}"))?;
        let definition_json = encode_json(&record.definition)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO workflow_graphs
                (id, name, description, schema_version, definition_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                record.id,
                record.name,
                record.description,
                record.definition.schema_version as i64,
                definition_json,
                record.created_at,
                record.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_workflow_graph(&self, id: &str) -> Result<Option<WorkflowGraphRecord>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, definition_json, created_at, updated_at
                 FROM workflow_graphs WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => {
                let (id, name, description, definition_json, created_at, updated_at) =
                    row.map_err(|error| error.to_string())?;
                let definition = decode_json(&definition_json)?;
                Ok(Some(WorkflowGraphRecord {
                    id,
                    name,
                    description,
                    definition,
                    created_at,
                    updated_at,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn list_workflow_graphs(&self) -> Result<Vec<WorkflowGraphRecord>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, definition_json, created_at, updated_at
                 FROM workflow_graphs ORDER BY created_at ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let (id, name, description, definition_json, created_at, updated_at) =
                row.map_err(|error| error.to_string())?;
            let definition = decode_json(&definition_json)?;
            Ok(WorkflowGraphRecord {
                id,
                name,
                description,
                definition,
                created_at,
                updated_at,
            })
        })
        .collect()
    }

    pub fn update_workflow_graph(
        &self,
        id: &str,
        record: &WorkflowGraphRecord,
    ) -> Result<(), String> {
        record
            .definition
            .validate()
            .map_err(|error| format!("invalid workflow graph: {error}"))?;
        let definition_json = encode_json(&record.definition)?;
        let conn = self.conn();
        conn.execute(
            "UPDATE workflow_graphs
                SET name=?2, description=?3, schema_version=?4, definition_json=?5, updated_at=?6
              WHERE id=?1",
            rusqlite::params![
                id,
                record.name,
                record.description,
                record.definition.schema_version as i64,
                definition_json,
                record.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn delete_workflow_graph(&self, id: &str) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM workflow_graphs WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    // ---- workflow_runs ---------------------------------------------------

    pub fn create_workflow_run(
        &self,
        workflow_graph_id: &str,
        run: &WorkflowRun,
    ) -> Result<(), String> {
        let definition_snapshot_json = encode_json(&run.definition)?;
        let state_json = encode_json(&run.node_states)?;
        let execution_id = run.execution_context.execution_id.to_string();
        let subject_id = run.execution_context.subject_id.as_str();
        let agent_name = run.execution_context.agent_name.as_str();
        let parent_execution_id = run
            .execution_context
            .parent_execution_id
            .as_ref()
            .map(|id| id.to_string());

        let conn = self.conn();
        conn.execute(
            "INSERT INTO workflow_runs
                (run_id, workflow_graph_id, execution_id, subject_id, agent_name,
                 parent_execution_id, status, definition_snapshot_json, state_json,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                run.run_id.as_str(),
                workflow_graph_id,
                execution_id,
                subject_id,
                agent_name,
                parent_execution_id,
                run.status.to_string(),
                definition_snapshot_json,
                state_json,
                run.created_at,
                run.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_workflow_run(
        &self,
        run_id: &WorkflowRunId,
    ) -> Result<Option<StoredWorkflowRun>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT run_id, workflow_graph_id, execution_id, subject_id, agent_name,
                        parent_execution_id, status, definition_snapshot_json, state_json,
                        created_at, updated_at
                 FROM workflow_runs WHERE run_id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![run_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => {
                let (
                    run_id,
                    workflow_graph_id,
                    execution_id,
                    subject_id,
                    agent_name,
                    parent_execution_id,
                    status,
                    definition_json,
                    state_json,
                    created_at,
                    updated_at,
                ) = row.map_err(|error| error.to_string())?;
                let run = reconstruct_run(
                    run_id,
                    execution_id,
                    subject_id,
                    agent_name,
                    parent_execution_id,
                    status,
                    definition_json,
                    state_json,
                    created_at,
                    updated_at,
                )?;
                Ok(Some(StoredWorkflowRun {
                    workflow_graph_id: workflow_graph_id.unwrap_or_default(),
                    run,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn update_workflow_run(
        &self,
        workflow_graph_id: &str,
        run: &WorkflowRun,
    ) -> Result<(), String> {
        let definition_snapshot_json = encode_json(&run.definition)?;
        let state_json = encode_json(&run.node_states)?;
        let conn = self.conn();
        conn.execute(
            "UPDATE workflow_runs
                SET workflow_graph_id=?2, status=?3, definition_snapshot_json=?4,
                    state_json=?5, updated_at=?6
              WHERE run_id=?1",
            rusqlite::params![
                run.run_id.as_str(),
                workflow_graph_id,
                run.status.to_string(),
                definition_snapshot_json,
                state_json,
                run.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::Database;
    use super::WorkflowGraphRecord;
    use crate::execution::{ExecutionContext, ExecutionId};
    use crate::workflow::{
        NodeRunStatus, WorkflowEdgeDefinition, WorkflowGraphDefinition, WorkflowNodeDefinition,
        WorkflowNodeId, WorkflowNodeKind, WorkflowRun, WorkflowRunId,
        WORKFLOW_GRAPH_SCHEMA_VERSION,
    };

    struct TempDatabase(PathBuf);

    impl TempDatabase {
        fn new(label: &str) -> Self {
            Self(std::env::temp_dir().join(format!("yilian-{label}-{}.db", uuid::Uuid::new_v4())))
        }
    }

    impl Drop for TempDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn node(id: &str) -> WorkflowNodeDefinition {
        WorkflowNodeDefinition {
            id: WorkflowNodeId::new(id).unwrap(),
            kind: WorkflowNodeKind::Agent,
        }
    }

    fn edge(from: &str, to: &str) -> WorkflowEdgeDefinition {
        WorkflowEdgeDefinition {
            from: WorkflowNodeId::new(from).unwrap(),
            to: WorkflowNodeId::new(to).unwrap(),
        }
    }

    fn linear_graph() -> WorkflowGraphDefinition {
        WorkflowGraphDefinition {
            schema_version: WORKFLOW_GRAPH_SCHEMA_VERSION,
            entry_node_id: WorkflowNodeId::new("start").unwrap(),
            nodes: vec![node("start"), node("a"), node("output")],
            edges: vec![edge("start", "a"), edge("a", "output")],
        }
    }

    fn graph_record(id: &str, definition: &WorkflowGraphDefinition) -> WorkflowGraphRecord {
        WorkflowGraphRecord {
            id: id.to_string(),
            name: format!("graph {id}"),
            description: String::new(),
            definition: definition.clone(),
            created_at: 1_000,
            updated_at: 1_000,
        }
    }

    fn make_run() -> WorkflowRun {
        let ctx = ExecutionContext::new(
            ExecutionId::new("exec-1").unwrap(),
            "local-user",
            "researcher",
            None,
            1_000,
        );
        WorkflowRun::new(WorkflowRunId::generate(), ctx, linear_graph(), 1_000).unwrap()
    }

    #[test]
    fn workflow_graph_roundtrips() {
        let temp = TempDatabase::new("wf-graph-roundtrip");
        let db = Database::new(&temp.0).unwrap();
        db.create_workflow_graph(&graph_record("g1", &linear_graph()))
            .unwrap();

        let got = db.get_workflow_graph("g1").unwrap().unwrap();
        assert_eq!(got.name, "graph g1");
        assert_eq!(got.definition, linear_graph());
    }

    #[test]
    fn workflow_run_roundtrips() {
        let temp = TempDatabase::new("wf-run-roundtrip");
        let db = Database::new(&temp.0).unwrap();
        let run = make_run();
        let run_id = run.run_id.clone();
        db.create_workflow_run("g1", &run).unwrap();

        let stored = db.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(stored.workflow_graph_id, "g1");
        assert_eq!(stored.run.run_id, run.run_id);
        assert_eq!(stored.run.status, run.status);
        assert_eq!(stored.run.definition, run.definition);
        assert_eq!(stored.run.node_states, run.node_states);
        assert_eq!(stored.run.execution_context.subject_id, "local-user");
        assert_eq!(stored.run.execution_context.agent_name, "researcher");
    }

    #[test]
    fn definition_snapshot_is_preserved_across_graph_edits() {
        let temp = TempDatabase::new("wf-snapshot");
        let db = Database::new(&temp.0).unwrap();
        db.create_workflow_graph(&graph_record("g1", &linear_graph()))
            .unwrap();
        let run = make_run();
        let run_id = run.run_id.clone();
        db.create_workflow_run("g1", &run).unwrap();

        // Edit the graph: add a node and edge.
        let mut edited = linear_graph();
        edited.nodes.push(node("extra"));
        edited.edges.push(edge("output", "extra"));
        db.update_workflow_graph("g1", &graph_record("g1", &edited))
            .unwrap();

        // The stored run still reflects the original snapshot.
        let stored = db.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(stored.run.definition, linear_graph());
        assert_eq!(stored.run.definition.nodes.len(), 3);
    }

    #[test]
    fn invalid_graph_is_not_persisted() {
        let temp = TempDatabase::new("wf-invalid");
        let db = Database::new(&temp.0).unwrap();
        let mut bad = linear_graph();
        bad.edges.push(edge("a", "a")); // self-edge is invalid
        assert!(db
            .create_workflow_graph(&graph_record("bad", &bad))
            .is_err());
        assert!(db.get_workflow_graph("bad").unwrap().is_none());
    }

    #[test]
    fn run_state_is_persisted_after_update() {
        let temp = TempDatabase::new("wf-state");
        let db = Database::new(&temp.0).unwrap();
        let mut run = make_run();
        let run_id = run.run_id.clone();
        db.create_workflow_run("g1", &run).unwrap();

        let start = WorkflowNodeId::new("start").unwrap();
        run.transition_node(&start, NodeRunStatus::Running, 2)
            .unwrap();
        run.transition_node(&start, NodeRunStatus::Completed, 3)
            .unwrap();
        run.updated_at = 3;
        db.update_workflow_run("g1", &run).unwrap();

        let stored = db.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(stored.run.status, run.status);
        assert_eq!(stored.run.node_states, run.node_states);
    }

    #[test]
    fn legacy_workflows_table_still_works() {
        let temp = TempDatabase::new("wf-legacy");
        let db = Database::new(&temp.0).unwrap();
        let wf = crate::db::Workflow {
            id: "legacy-1".to_string(),
            name: "legacy".to_string(),
            description: String::new(),
            nodes: vec!["a".to_string()],
            tags: vec![],
            system_prompt_extra: String::new(),
            is_builtin: false,
            created_at: 1_000,
            updated_at: 1_000,
        };
        db.create_workflow(&wf).unwrap();
        let list = db.list_workflows().unwrap();
        assert!(list.iter().any(|w| w.id == "legacy-1"));
    }

    #[test]
    fn builtin_templates_still_exist() {
        let temp = TempDatabase::new("wf-builtins");
        let db = Database::new(&temp.0).unwrap();
        let list = db.list_workflows().unwrap();
        assert_eq!(list.len(), 6);
        assert!(list.iter().all(|w| w.is_builtin));
    }
}
