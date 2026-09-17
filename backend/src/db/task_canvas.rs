use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::task::{
    CanvasView, CanvasViewError, TaskCheckpointSummary, TaskGraph, TaskGraphId,
    TaskRevisionSummary, TaskSupervisor, MAX_TASK_REVISION_HISTORY,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Serialize};

const TASK_CANVAS_SCHEMA_SQL: &str = "CREATE TABLE task_canvas_views (
    graph_id TEXT PRIMARY KEY,
    view_revision INTEGER NOT NULL CHECK (view_revision > 0),
    graph_revision_seen INTEGER NOT NULL CHECK (graph_revision_seen > 0),
    viewport_json TEXT NOT NULL,
    node_layouts_json TEXT NOT NULL,
    selection_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE task_graph_revision_history (
    graph_id TEXT NOT NULL,
    graph_revision INTEGER NOT NULL CHECK (graph_revision > 0),
    change_summary TEXT NOT NULL,
    node_count INTEGER NOT NULL CHECK (node_count >= 0),
    edge_count INTEGER NOT NULL CHECK (edge_count >= 0),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (graph_id, graph_revision)
);
CREATE INDEX idx_task_graph_revision_history_graph
    ON task_graph_revision_history(graph_id, graph_revision);";

pub(crate) const TASK_CANVAS_SCHEMA_MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1001,
    "1001_task_canvas_views",
    MigrationOwner::V1TaskWorld,
    TASK_CANVAS_SCHEMA_SQL,
);

fn encode_json<T: Serialize>(
    field: &'static str,
    value: &T,
) -> Result<String, super::TaskWorldPersistenceError> {
    serde_json::to_string(value).map_err(|error| super::TaskWorldPersistenceError::Json {
        field,
        message: error.to_string(),
    })
}

fn decode_json<T: DeserializeOwned>(
    field: &'static str,
    value: &str,
) -> Result<T, super::TaskWorldPersistenceError> {
    serde_json::from_str(value).map_err(|error| super::TaskWorldPersistenceError::Json {
        field,
        message: error.to_string(),
    })
}

fn positive_i64(value: u64, field: &str) -> Result<i64, super::TaskWorldPersistenceError> {
    i64::try_from(value).map_err(|_| {
        super::TaskWorldPersistenceError::Integrity(format!(
            "{field} {value} cannot be represented by SQLite INTEGER"
        ))
    })
}

fn positive_u64(value: i64, field: &str) -> Result<u64, super::TaskWorldPersistenceError> {
    let value = u64::try_from(value).map_err(|_| {
        super::TaskWorldPersistenceError::Integrity(format!(
            "stored {field} {value} must be a positive integer"
        ))
    })?;
    if value == 0 {
        return Err(super::TaskWorldPersistenceError::Integrity(format!(
            "stored {field} must be a positive integer"
        )));
    }
    Ok(value)
}

fn canvas_error(error: CanvasViewError) -> super::TaskWorldPersistenceError {
    super::TaskWorldPersistenceError::Integrity(error.to_string())
}

impl Database {
    pub(crate) fn initialize_task_canvas_schema(
        &self,
    ) -> Result<(), super::TaskWorldPersistenceError> {
        self.apply_product_migration(&TASK_CANVAS_SCHEMA_MIGRATION)?;
        Ok(())
    }

    pub(crate) fn save_task_canvas_view(
        &self,
        view: &CanvasView,
        graph: &TaskGraph,
    ) -> Result<(), super::TaskWorldPersistenceError> {
        view.validate_for_graph(graph).map_err(canvas_error)?;
        let view_revision = positive_i64(view.view_revision, "view revision")?;
        let graph_revision_seen = positive_i64(view.graph_revision_seen, "graph revision")?;
        let viewport_json = encode_json("viewport_json", &view.viewport)?;
        let node_layouts_json = encode_json("node_layouts_json", &view.node_layouts)?;
        let selection_json = encode_json("selection_json", &view.selection)?;

        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO task_canvas_views (
                graph_id, view_revision, graph_revision_seen, viewport_json,
                node_layouts_json, selection_json, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(graph_id) DO UPDATE SET
                view_revision=excluded.view_revision,
                graph_revision_seen=excluded.graph_revision_seen,
                viewport_json=excluded.viewport_json,
                node_layouts_json=excluded.node_layouts_json,
                selection_json=excluded.selection_json,
                updated_at=excluded.updated_at",
            params![
                view.graph_id.as_str(),
                view_revision,
                graph_revision_seen,
                viewport_json,
                node_layouts_json,
                selection_json,
                view.updated_at,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn load_task_canvas_view(
        &self,
        graph: &TaskGraph,
    ) -> Result<Option<CanvasView>, super::TaskWorldPersistenceError> {
        let conn = self.conn();
        let row: Option<(String, i64, i64, String, String, String, i64)> = conn
            .query_row(
                "SELECT graph_id, view_revision, graph_revision_seen, viewport_json,
                        node_layouts_json, selection_json, updated_at
                 FROM task_canvas_views WHERE graph_id=?1",
                params![graph.id.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        drop(conn);

        let Some((
            stored_graph_id,
            stored_view_revision,
            stored_graph_revision,
            viewport_json,
            node_layouts_json,
            selection_json,
            updated_at,
        )) = row
        else {
            return Ok(None);
        };
        if stored_graph_id != graph.id.as_str() {
            return Err(super::TaskWorldPersistenceError::Integrity(
                "canvas view graph_id column does not match lookup identity".to_string(),
            ));
        }
        let view_revision = positive_u64(stored_view_revision, "view revision")?;
        let graph_revision_seen = positive_u64(stored_graph_revision, "graph revision seen")?;
        let view = CanvasView {
            schema_version: crate::task::CANVAS_VIEW_SCHEMA_VERSION,
            graph_id: graph.id.clone(),
            view_revision,
            graph_revision_seen,
            viewport: decode_json("viewport_json", &viewport_json)?,
            node_layouts: decode_json("node_layouts_json", &node_layouts_json)?,
            selection: decode_json("selection_json", &selection_json)?,
            updated_at,
        };
        view.validate_shape().map_err(canvas_error)?;
        Ok(Some(view))
    }

    /// Atomically persist a semantic supervisor snapshot and one bounded
    /// revision summary.  Canvas view writes never use this method.
    pub(crate) fn save_task_supervisor_snapshot_with_revision(
        &self,
        supervisor: &TaskSupervisor,
        updated_at: i64,
        change_summary: &str,
    ) -> Result<(), super::TaskWorldPersistenceError> {
        let graph = supervisor.graph();
        let checkpoint = crate::task::TaskCheckpoint {
            id: crate::task::TaskCheckpointId::generate(),
            graph_id: graph.id.clone(),
            graph_revision: graph.revision,
            graph: graph.clone(),
            node_states: supervisor.node_states().to_vec(),
            created_at: updated_at,
        };
        // Reuse the existing fail-closed checkpoint validator through the
        // regular persistence entrypoint's rules before opening the write.
        super::task_world::validate_checkpoint(&checkpoint)?;
        let graph_revision = positive_i64(graph.revision.value(), "graph revision")?;
        let graph_json = encode_json("graph_json", graph)?;
        let node_states_json = encode_json("node_states_json", &checkpoint.node_states)?;
        let change_summary = change_summary.trim();
        if change_summary.is_empty() || change_summary.chars().count() > 200 {
            return Err(super::TaskWorldPersistenceError::Integrity(
                "semantic change summary must contain 1-200 characters".to_string(),
            ));
        }

        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if change_summary == "created" {
            // Initial control, definition and revision are one atomic creation.
            // A duplicate identity fails before the snapshot's update path.
            transaction.execute(
                "INSERT INTO task_world_execution_controls (graph_id, state, generation, paused_nodes_json, updated_at) VALUES (?1, 'running', 0, '[]', ?2)",
                params![graph.id.as_str(), updated_at],
            )?;
        }
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
        transaction.execute(
            "INSERT INTO task_graph_revision_history (
                graph_id, graph_revision, change_summary, node_count, edge_count, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(graph_id, graph_revision) DO UPDATE SET
                change_summary=excluded.change_summary,
                node_count=excluded.node_count,
                edge_count=excluded.edge_count,
                created_at=excluded.created_at",
            params![
                graph.id.as_str(),
                graph_revision,
                change_summary,
                i64::try_from(graph.nodes.len()).unwrap_or(i64::MAX),
                i64::try_from(graph.edges.len()).unwrap_or(i64::MAX),
                updated_at,
            ],
        )?;
        transaction.execute(
            "DELETE FROM task_graph_revision_history
             WHERE graph_id=?1 AND graph_revision NOT IN (
                 SELECT graph_revision FROM task_graph_revision_history
                 WHERE graph_id=?1 ORDER BY graph_revision DESC LIMIT ?2
             )",
            params![graph.id.as_str(), MAX_TASK_REVISION_HISTORY as i64],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn load_task_revision_history(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<Vec<TaskRevisionSummary>, super::TaskWorldPersistenceError> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT graph_id, graph_revision, change_summary, node_count, edge_count, created_at
             FROM task_graph_revision_history WHERE graph_id=?1 ORDER BY graph_revision ASC",
        )?;
        let rows = statement
            .query_map(params![graph_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        drop(conn);

        rows.into_iter()
            .map(
                |(stored_graph_id, revision, change, node_count, edge_count, created_at)| {
                    if stored_graph_id != graph_id.as_str() {
                        return Err(super::TaskWorldPersistenceError::Integrity(
                            "revision history graph_id does not match lookup identity".to_string(),
                        ));
                    }
                    let node_count = usize::try_from(node_count).map_err(|_| {
                        super::TaskWorldPersistenceError::Integrity(
                            "revision history node_count must be non-negative".to_string(),
                        )
                    })?;
                    let edge_count = usize::try_from(edge_count).map_err(|_| {
                        super::TaskWorldPersistenceError::Integrity(
                            "revision history edge_count must be non-negative".to_string(),
                        )
                    })?;
                    Ok(TaskRevisionSummary {
                        graph_id: graph_id.clone(),
                        revision: positive_u64(revision, "graph revision")?,
                        change,
                        node_count,
                        edge_count,
                        created_at,
                    })
                },
            )
            .collect()
    }

    /// Backfill a compact baseline summary for graphs created before the
    /// 1001 migration. This never rewrites the graph snapshot or its revision.
    pub(crate) fn ensure_task_revision_history(
        &self,
        graph: &TaskGraph,
        created_at: i64,
    ) -> Result<(), super::TaskWorldPersistenceError> {
        graph
            .validate()
            .map_err(|error| super::TaskWorldPersistenceError::Integrity(error.to_string()))?;
        let graph_revision = positive_i64(graph.revision.value(), "graph revision")?;
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<i64> = transaction
            .query_row(
                "SELECT graph_revision FROM task_graph_revision_history
                 WHERE graph_id=?1 LIMIT 1",
                params![graph.id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_none() {
            transaction.execute(
                "INSERT INTO task_graph_revision_history (
                    graph_id, graph_revision, change_summary, node_count, edge_count, created_at
                 ) VALUES (?1, ?2, 'baseline', ?3, ?4, ?5)",
                params![
                    graph.id.as_str(),
                    graph_revision,
                    i64::try_from(graph.nodes.len()).unwrap_or(i64::MAX),
                    i64::try_from(graph.edges.len()).unwrap_or(i64::MAX),
                    created_at,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn load_task_checkpoint_summaries(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<Vec<TaskCheckpointSummary>, super::TaskWorldPersistenceError> {
        let checkpoint_ids: Vec<String> = {
            let conn = self.conn();
            let mut statement = conn.prepare(
                "SELECT checkpoint_id FROM task_world_checkpoints
                 WHERE graph_id=?1 ORDER BY graph_revision ASC, checkpoint_id ASC",
            )?;
            let rows = statement
                .query_map(params![graph_id.as_str()], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };

        checkpoint_ids
            .into_iter()
            .map(|checkpoint_id| {
                let checkpoint = self.load_task_checkpoint(&checkpoint_id)?.ok_or_else(|| {
                    super::TaskWorldPersistenceError::Integrity(format!(
                        "checkpoint {checkpoint_id} disappeared during detail load"
                    ))
                })?;
                if checkpoint.graph_id != *graph_id {
                    return Err(super::TaskWorldPersistenceError::Integrity(
                        "checkpoint graph_id does not match requested graph".to_string(),
                    ));
                }
                Ok(TaskCheckpointSummary {
                    checkpoint_id: checkpoint.id.to_string(),
                    graph_id: checkpoint.graph_id,
                    graph_revision: checkpoint.graph_revision.value(),
                    created_at: checkpoint.created_at,
                })
            })
            .collect()
    }
}
