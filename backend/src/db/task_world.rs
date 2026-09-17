use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::task::{
    GraphRevision, TaskCheckpoint, TaskCheckpointId, TaskExecutionControl,
    TaskExecutionControlState, TaskGraph, TaskGraphId, TaskNodeId, TaskNodeState, TaskSupervisor,
    TaskSupervisorError,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

const TASK_WORLD_SCHEMA_SQL: &str = "CREATE TABLE task_world_supervisor_snapshots (
    graph_id TEXT PRIMARY KEY,
    graph_revision INTEGER NOT NULL CHECK (graph_revision > 0),
    graph_json TEXT NOT NULL,
    node_states_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE task_world_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    graph_id TEXT NOT NULL,
    graph_revision INTEGER NOT NULL CHECK (graph_revision > 0),
    checkpoint_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_task_world_checkpoints_graph_revision
    ON task_world_checkpoints(graph_id, graph_revision);";

const TASK_EXECUTION_CONTROL_SCHEMA_SQL: &str = "CREATE TABLE task_world_execution_controls (
    graph_id TEXT PRIMARY KEY,
    state TEXT NOT NULL CHECK (state IN ('running', 'paused', 'cancelled')),
    generation INTEGER NOT NULL CHECK (generation >= 0),
    paused_nodes_json TEXT NOT NULL DEFAULT '[]',
    updated_at INTEGER NOT NULL
);";

pub(crate) const TASK_WORLD_SCHEMA_MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1000,
    "1000_task_world_persistence",
    MigrationOwner::V1TaskWorld,
    TASK_WORLD_SCHEMA_SQL,
);

pub(crate) const TASK_EXECUTION_CONTROL_SCHEMA_MIGRATION: ProductMigrationSpec =
    ProductMigrationSpec::new(
        1003,
        "1003_task_execution_controls",
        MigrationOwner::V1TaskWorld,
        TASK_EXECUTION_CONTROL_SCHEMA_SQL,
    );

#[derive(Debug, Error)]
pub enum TaskWorldPersistenceError {
    #[error("task world database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("task world JSON error in {field}: {message}")]
    Json {
        field: &'static str,
        message: String,
    },
    #[error("task world state validation failed: {0}")]
    Validation(#[from] TaskSupervisorError),
    #[error("task world storage integrity error: {0}")]
    Integrity(String),
}

/// Persisted supervisor state together with its storage timestamp.
#[derive(Debug)]
pub struct TaskSupervisorSnapshot {
    pub(crate) supervisor: TaskSupervisor,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct SupervisorSnapshotPayload {
    graph: TaskGraph,
    node_states: Vec<TaskNodeState>,
}

fn encode_json<T: Serialize>(
    field: &'static str,
    value: &T,
) -> Result<String, TaskWorldPersistenceError> {
    serde_json::to_string(value).map_err(|error| TaskWorldPersistenceError::Json {
        field,
        message: error.to_string(),
    })
}

fn decode_json<T: DeserializeOwned>(
    field: &'static str,
    value: &str,
) -> Result<T, TaskWorldPersistenceError> {
    serde_json::from_str(value).map_err(|error| TaskWorldPersistenceError::Json {
        field,
        message: error.to_string(),
    })
}

fn revision_to_i64(revision: GraphRevision) -> Result<i64, TaskWorldPersistenceError> {
    i64::try_from(revision.value()).map_err(|_| {
        TaskWorldPersistenceError::Integrity(format!(
            "graph revision {} cannot be represented by SQLite INTEGER",
            revision.value()
        ))
    })
}

fn revision_from_i64(value: i64) -> Result<GraphRevision, TaskWorldPersistenceError> {
    let value = u64::try_from(value).map_err(|_| {
        TaskWorldPersistenceError::Integrity(format!(
            "stored graph revision {value} must be a positive integer"
        ))
    })?;
    GraphRevision::new(value)
        .map_err(|error| TaskWorldPersistenceError::Integrity(error.to_string()))
}

pub(crate) fn validate_checkpoint(
    checkpoint: &TaskCheckpoint,
) -> Result<(), TaskWorldPersistenceError> {
    if checkpoint.id.as_str().is_empty() {
        return Err(TaskWorldPersistenceError::Integrity(
            "checkpoint id must not be empty".to_string(),
        ));
    }

    let mut supervisor = TaskSupervisor::new(checkpoint.graph.clone(), checkpoint.created_at)?;
    // Snapshot validation may inspect an in-flight command projection. Live
    // restores reject active requests, but persistence must still be able to
    // round-trip the reservation so startup can fail it closed if the provider
    // state is unavailable.
    supervisor.restore_exact(checkpoint)?;
    Ok(())
}

fn restore_supervisor(
    graph: TaskGraph,
    node_states: Vec<TaskNodeState>,
    updated_at: i64,
) -> Result<TaskSupervisor, TaskWorldPersistenceError> {
    let checkpoint = TaskCheckpoint {
        id: TaskCheckpointId::generate(),
        graph_id: graph.id.clone(),
        graph_revision: graph.revision,
        graph: graph.clone(),
        node_states,
        created_at: updated_at,
    };
    let mut supervisor = TaskSupervisor::new(graph, updated_at)?;
    supervisor.restore_exact(&checkpoint)?;
    Ok(supervisor)
}

impl Database {
    /// Explicitly install the v1 Task World persistence schema.  Database::new
    /// intentionally does not call this method.
    pub(crate) fn initialize_task_world_schema(&self) -> Result<(), TaskWorldPersistenceError> {
        self.apply_product_migration(&TASK_WORLD_SCHEMA_MIGRATION)?;
        self.initialize_task_canvas_schema()?;
        self.initialize_task_execution_schema()
            .map_err(|error| TaskWorldPersistenceError::Integrity(error.to_string()))?;
        self.apply_product_migration(&TASK_EXECUTION_CONTROL_SCHEMA_MIGRATION)?;
        Ok(())
    }

    pub(crate) fn save_task_execution_control(
        &self,
        control: &TaskExecutionControl,
    ) -> Result<(), TaskWorldPersistenceError> {
        let generation = i64::try_from(control.generation).map_err(|_| {
            TaskWorldPersistenceError::Integrity(format!(
                "execution control generation {} cannot be represented by SQLite INTEGER",
                control.generation
            ))
        })?;
        let paused_nodes = encode_json("paused_nodes_json", &control.paused_nodes)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO task_world_execution_controls
                (graph_id, state, generation, paused_nodes_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(graph_id) DO UPDATE SET
                state=excluded.state,
                generation=excluded.generation,
                paused_nodes_json=excluded.paused_nodes_json,
                updated_at=excluded.updated_at",
            params![
                control.graph_id.as_str(),
                control.state.as_str(),
                generation,
                paused_nodes,
                control.updated_at,
            ],
        )?;
        Ok(())
    }

    pub(crate) fn load_task_execution_control(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<Option<TaskExecutionControl>, TaskWorldPersistenceError> {
        let conn = self.conn();
        let row: Option<(String, String, i64, String, i64)> = conn
            .query_row(
                "SELECT graph_id, state, generation, paused_nodes_json, updated_at
                 FROM task_world_execution_controls WHERE graph_id=?1",
                params![graph_id.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        drop(conn);
        let Some((stored_graph_id, state, generation, paused_nodes_json, updated_at)) = row else {
            return Ok(None);
        };
        if stored_graph_id != graph_id.as_str() {
            return Err(TaskWorldPersistenceError::Integrity(
                "execution control graph_id column does not match lookup identity".to_string(),
            ));
        }
        let state = TaskExecutionControlState::from_str(&state).ok_or_else(|| {
            TaskWorldPersistenceError::Integrity("execution control state is invalid".to_string())
        })?;
        let generation = u64::try_from(generation).map_err(|_| {
            TaskWorldPersistenceError::Integrity(
                "execution control generation must be non-negative".to_string(),
            )
        })?;
        let paused_nodes: Vec<TaskNodeId> = decode_json("paused_nodes_json", &paused_nodes_json)?;
        Ok(Some(TaskExecutionControl {
            graph_id: graph_id.clone(),
            state,
            generation,
            paused_nodes,
            updated_at,
        }))
    }

    pub(crate) fn load_all_task_execution_controls(
        &self,
    ) -> Result<Vec<TaskExecutionControl>, TaskWorldPersistenceError> {
        let conn = self.conn();
        let graph_ids: Vec<String> = conn
            .prepare("SELECT graph_id FROM task_world_execution_controls ORDER BY graph_id")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(conn);
        graph_ids
            .into_iter()
            .map(|graph_id| {
                let graph_id_for_lookup = TaskGraphId::new(graph_id.clone()).map_err(|error| {
                    TaskWorldPersistenceError::Integrity(format!(
                        "stored execution control graph_id {graph_id:?} is invalid: {error}"
                    ))
                })?;
                self.load_task_execution_control(&graph_id_for_lookup)?
                    .ok_or_else(|| {
                        TaskWorldPersistenceError::Integrity(format!(
                            "execution control for graph_id {graph_id:?} disappeared during load"
                        ))
                    })
            })
            .collect()
    }

    pub(crate) fn save_task_supervisor_snapshot(
        &self,
        supervisor: &TaskSupervisor,
        updated_at: i64,
    ) -> Result<(), TaskWorldPersistenceError> {
        let graph = supervisor.graph();
        let graph_revision = revision_to_i64(graph.revision)?;
        let checkpoint = TaskCheckpoint {
            id: TaskCheckpointId::generate(),
            graph_id: graph.id.clone(),
            graph_revision: graph.revision,
            graph: graph.clone(),
            node_states: supervisor.node_states().to_vec(),
            created_at: updated_at,
        };
        validate_checkpoint(&checkpoint)?;

        let payload = SupervisorSnapshotPayload {
            graph: graph.clone(),
            node_states: supervisor.node_states().to_vec(),
        };
        let graph_json = encode_json("graph_json", &payload.graph)?;
        let node_states_json = encode_json("node_states_json", &payload.node_states)?;

        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO task_world_supervisor_snapshots (
                graph_id, graph_revision, graph_json, node_states_json, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(graph_id) DO UPDATE SET
                graph_revision=excluded.graph_revision,
                graph_json=excluded.graph_json,
                node_states_json=excluded.node_states_json,
                updated_at=excluded.updated_at",
            params![
                graph.id.as_str(),
                graph_revision,
                graph_json,
                node_states_json,
                updated_at,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn load_task_supervisor_snapshot(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<Option<TaskSupervisorSnapshot>, TaskWorldPersistenceError> {
        let conn = self.conn();
        let row: Option<(String, i64, String, String, i64)> = conn
            .query_row(
                "SELECT graph_id, graph_revision, graph_json, node_states_json, updated_at
                 FROM task_world_supervisor_snapshots WHERE graph_id=?1",
                params![graph_id.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        drop(conn);

        let Some((stored_graph_id, stored_revision, graph_json, node_states_json, updated_at)) =
            row
        else {
            return Ok(None);
        };
        if stored_graph_id != graph_id.as_str() {
            return Err(TaskWorldPersistenceError::Integrity(
                "snapshot graph_id column does not match lookup identity".to_string(),
            ));
        }

        let stored_revision = revision_from_i64(stored_revision)?;
        let graph: TaskGraph = decode_json("graph_json", &graph_json)?;
        if graph.id.as_str() != stored_graph_id {
            return Err(TaskWorldPersistenceError::Integrity(
                "snapshot graph_id column does not match graph JSON".to_string(),
            ));
        }
        if graph.revision != stored_revision {
            return Err(TaskWorldPersistenceError::Integrity(
                "snapshot graph_revision column does not match graph JSON".to_string(),
            ));
        }
        let node_states: Vec<TaskNodeState> = decode_json("node_states_json", &node_states_json)?;
        let supervisor = restore_supervisor(graph, node_states, updated_at)?;
        Ok(Some(TaskSupervisorSnapshot {
            supervisor,
            updated_at,
        }))
    }

    pub(crate) fn load_all_task_supervisor_snapshots(
        &self,
    ) -> Result<Vec<TaskSupervisorSnapshot>, TaskWorldPersistenceError> {
        let stored_graph_ids: Vec<String> = {
            let conn = self.conn();
            let mut statement = conn.prepare(
                "SELECT graph_id
                 FROM task_world_supervisor_snapshots
                 ORDER BY graph_id",
            )?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };

        stored_graph_ids
            .into_iter()
            .map(|stored_graph_id| {
                let graph_id = TaskGraphId::new(stored_graph_id.clone()).map_err(|error| {
                    TaskWorldPersistenceError::Integrity(format!(
                        "stored graph_id {stored_graph_id:?} is invalid: {error}"
                    ))
                })?;
                self.load_task_supervisor_snapshot(&graph_id)?
                    .ok_or_else(|| {
                        TaskWorldPersistenceError::Integrity(format!(
                            "snapshot for graph_id {stored_graph_id:?} disappeared during load"
                        ))
                    })
            })
            .collect()
    }

    pub(crate) fn save_task_checkpoint(
        &self,
        checkpoint: &TaskCheckpoint,
    ) -> Result<(), TaskWorldPersistenceError> {
        validate_checkpoint(checkpoint)?;
        let graph_revision = revision_to_i64(checkpoint.graph_revision)?;
        let checkpoint_json = encode_json("checkpoint_json", checkpoint)?;

        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO task_world_checkpoints (
                checkpoint_id, graph_id, graph_revision, checkpoint_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(checkpoint_id) DO UPDATE SET
                graph_id=excluded.graph_id,
                graph_revision=excluded.graph_revision,
                checkpoint_json=excluded.checkpoint_json,
                created_at=excluded.created_at",
            params![
                checkpoint.id.as_str(),
                checkpoint.graph_id.as_str(),
                graph_revision,
                checkpoint_json,
                checkpoint.created_at,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn load_task_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Option<TaskCheckpoint>, TaskWorldPersistenceError> {
        let conn = self.conn();
        let row: Option<(String, String, i64, String, i64)> = conn
            .query_row(
                "SELECT checkpoint_id, graph_id, graph_revision, checkpoint_json, created_at
                 FROM task_world_checkpoints WHERE checkpoint_id=?1",
                params![checkpoint_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        drop(conn);

        let Some((
            stored_checkpoint_id,
            stored_graph_id,
            stored_revision,
            checkpoint_json,
            stored_created_at,
        )) = row
        else {
            return Ok(None);
        };
        if stored_checkpoint_id != checkpoint_id {
            return Err(TaskWorldPersistenceError::Integrity(
                "checkpoint_id column does not match lookup identity".to_string(),
            ));
        }

        let stored_revision = revision_from_i64(stored_revision)?;
        let checkpoint: TaskCheckpoint = decode_json("checkpoint_json", &checkpoint_json)?;
        if checkpoint.id.as_str() != stored_checkpoint_id {
            return Err(TaskWorldPersistenceError::Integrity(
                "checkpoint_id column does not match checkpoint JSON".to_string(),
            ));
        }
        if checkpoint.graph_id.as_str() != stored_graph_id {
            return Err(TaskWorldPersistenceError::Integrity(
                "checkpoint graph_id column does not match checkpoint JSON".to_string(),
            ));
        }
        if checkpoint.graph_revision != stored_revision {
            return Err(TaskWorldPersistenceError::Integrity(
                "checkpoint graph_revision column does not match checkpoint JSON".to_string(),
            ));
        }
        if checkpoint.created_at != stored_created_at {
            return Err(TaskWorldPersistenceError::Integrity(
                "checkpoint created_at column does not match checkpoint JSON".to_string(),
            ));
        }
        validate_checkpoint(&checkpoint)?;
        Ok(Some(checkpoint))
    }
}

#[cfg(test)]
mod tests {
    use crate::task::{
        GraphRevision, TaskGraph, TaskGraphId, TaskNode, TaskNodeId, TaskNodeKind, TaskSupervisor,
    };
    use rusqlite::Connection;
    use serde_json::json;
    use std::path::Path;

    fn node_id(raw: &str) -> TaskNodeId {
        TaskNodeId::new(raw).expect("valid task node id")
    }

    fn sample_supervisor() -> TaskSupervisor {
        let graph = TaskGraph::new(
            TaskGraphId::new("persist-graph").unwrap(),
            GraphRevision::initial(),
            vec![
                TaskNode::new(
                    node_id("source"),
                    TaskNodeKind::Work,
                    "source",
                    json!({"value": "initial"}),
                )
                .unwrap(),
                TaskNode::new(
                    node_id("child"),
                    TaskNodeKind::Work,
                    "child",
                    json!({"depends_on": "source"}),
                )
                .unwrap(),
            ],
            vec![crate::task::TaskEdge::new(
                node_id("source"),
                node_id("child"),
            )],
        )
        .unwrap();
        let mut supervisor = TaskSupervisor::new(graph, 100).unwrap();
        supervisor.start_node(&node_id("source"), 101).unwrap();
        supervisor
            .succeed_node(&node_id("source"), json!({"ok": true}), 102)
            .unwrap();
        supervisor
            .update_node_input(&node_id("child"), json!({"changed": true}), 103)
            .unwrap();
        supervisor
    }

    fn database() -> crate::db::Database {
        let db = crate::db::Database::new(Path::new(":memory:")).unwrap();
        db.initialize_task_world_schema().unwrap();
        db
    }

    #[test]
    fn task_world_schema_registers_v1_owner_at_version_1000() {
        let db = crate::db::Database::new(Path::new(":memory:")).unwrap();
        {
            let conn = db.conn();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='task_world_supervisor_snapshots'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }

        db.initialize_task_world_schema().unwrap();

        let conn = db.conn();
        let metadata: (String, String) = conn
            .query_row(
                "SELECT name, owner FROM schema_migrations WHERE version=1000",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            metadata,
            ("1000_task_world_persistence".into(), "v1_task_world".into())
        );
    }

    #[test]
    fn supervisor_snapshot_roundtrip_preserves_revision_states_and_updated_at() {
        let db = database();
        let supervisor = sample_supervisor();

        db.save_task_supervisor_snapshot(&supervisor, 500).unwrap();
        let loaded = db
            .load_task_supervisor_snapshot(&supervisor.graph().id)
            .unwrap()
            .expect("snapshot must exist");

        assert_eq!(loaded.supervisor.graph(), supervisor.graph());
        assert_eq!(loaded.supervisor.node_states(), supervisor.node_states());
        assert_eq!(
            loaded.supervisor.graph_revision(),
            supervisor.graph_revision()
        );
        assert_eq!(loaded.updated_at, 500);
    }

    #[test]
    fn checkpoint_roundtrip_and_corrupt_json_fail_closed() {
        let db = database();
        let supervisor = sample_supervisor();
        let checkpoint = supervisor.checkpoint(700);

        db.save_task_checkpoint(&checkpoint).unwrap();
        let loaded = db
            .load_task_checkpoint(checkpoint.id.as_str())
            .unwrap()
            .expect("checkpoint must exist");
        assert_eq!(loaded, checkpoint);

        {
            let conn: std::sync::MutexGuard<'_, Connection> = db.conn();
            conn.execute(
                "UPDATE task_world_checkpoints SET checkpoint_json='not-json' WHERE checkpoint_id=?1",
                [checkpoint.id.as_str()],
            )
            .unwrap();
        }
        assert!(db.load_task_checkpoint(checkpoint.id.as_str()).is_err());
    }
}
