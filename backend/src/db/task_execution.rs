//! Append-only Task Harness attempt persistence.
//!
//! One row represents one attempt. Status changes update that attempt's
//! evidence, while retries always insert a new execution id and therefore
//! never erase the previous attempt.

use super::{migrations::MigrationOwner, migrations::ProductMigrationSpec, Database};
use crate::modules::task::{
    NodeExecution, NodeExecutionError, NodeExecutionId, NodeExecutionStatus, TaskGraphId,
    TaskNodeId,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

const TASK_EXECUTION_SCHEMA_SQL: &str = "CREATE TABLE task_node_executions (
    execution_id TEXT PRIMARY KEY,
    graph_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    status TEXT NOT NULL,
    executor_ref TEXT,
    context_json TEXT NOT NULL,
    output_json TEXT,
    validation_json TEXT,
    audit_ref TEXT,
    approval_ref TEXT,
    failure_code TEXT,
    error TEXT,
    retry_policy_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    UNIQUE(graph_id, node_id, attempt)
);
CREATE INDEX idx_task_node_executions_graph_node
    ON task_node_executions(graph_id, node_id, attempt);
CREATE INDEX idx_task_node_executions_active
    ON task_node_executions(status);";

pub(crate) const TASK_EXECUTION_SCHEMA_MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1002,
    "1002_task_node_executions",
    MigrationOwner::V1TaskWorld,
    TASK_EXECUTION_SCHEMA_SQL,
);

const EXTERNAL_REVIEW_MIGRATION: ProductMigrationSpec = ProductMigrationSpec::new(
    1101,
    "1101_task_execution_external_review",
    MigrationOwner::V1TaskWorld,
    "ALTER TABLE task_node_executions ADD COLUMN external_review_json TEXT;",
);

#[derive(Debug, Error)]
pub enum TaskExecutionPersistenceError {
    #[error("task execution database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("task execution JSON error in {field}: {message}")]
    Json {
        field: &'static str,
        message: String,
    },
    #[error("task execution identity or storage integrity error: {0}")]
    Integrity(String),
    #[error("task execution state validation failed: {0}")]
    Execution(#[from] NodeExecutionError),
}

fn encode_json<T: Serialize>(
    field: &'static str,
    value: &T,
) -> Result<String, TaskExecutionPersistenceError> {
    serde_json::to_string(value).map_err(|error| TaskExecutionPersistenceError::Json {
        field,
        message: error.to_string(),
    })
}

fn decode_json<T: DeserializeOwned>(
    field: &'static str,
    value: &str,
) -> Result<T, TaskExecutionPersistenceError> {
    serde_json::from_str(value).map_err(|error| TaskExecutionPersistenceError::Json {
        field,
        message: error.to_string(),
    })
}

fn parse_status(raw: &str) -> Result<NodeExecutionStatus, TaskExecutionPersistenceError> {
    Ok(match raw {
        "pending" => NodeExecutionStatus::Pending,
        "ready" => NodeExecutionStatus::Ready,
        "dispatching" => NodeExecutionStatus::Dispatching,
        "waiting_approval" => NodeExecutionStatus::WaitingApproval,
        "running" => NodeExecutionStatus::Running,
        "validating" => NodeExecutionStatus::Validating,
        "succeeded" => NodeExecutionStatus::Succeeded,
        "failed" => NodeExecutionStatus::Failed,
        "blocked" => NodeExecutionStatus::Blocked,
        "cancelled" => NodeExecutionStatus::Cancelled,
        "stale" => NodeExecutionStatus::Stale,
        other => {
            return Err(TaskExecutionPersistenceError::Integrity(format!(
                "unknown node execution status {other:?}"
            )))
        }
    })
}

fn to_i64(value: u32, field: &'static str) -> Result<i64, TaskExecutionPersistenceError> {
    i64::try_from(value).map_err(|_| {
        TaskExecutionPersistenceError::Integrity(format!("{field} does not fit SQLite INTEGER"))
    })
}

fn row_to_execution(
    row: (
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
        Option<String>,
    ),
) -> Result<NodeExecution, TaskExecutionPersistenceError> {
    let (
        execution_id,
        graph_id,
        node_id,
        attempt,
        status,
        executor_ref,
        context_json,
        output_json,
        validation_json,
        audit_ref,
        approval_ref,
        failure_code,
        error,
        retry_policy_json,
        created_at,
        updated_at,
        started_at,
        finished_at,
        external_review_json,
    ) = row;
    let execution_id = NodeExecutionId::new(execution_id)
        .map_err(|error| TaskExecutionPersistenceError::Integrity(error.to_string()))?;
    let graph_id = TaskGraphId::new(graph_id)
        .map_err(|error| TaskExecutionPersistenceError::Integrity(error.to_string()))?;
    let node_id = TaskNodeId::new(node_id)
        .map_err(|error| TaskExecutionPersistenceError::Integrity(error.to_string()))?;
    let attempt = u32::try_from(attempt).map_err(|_| {
        TaskExecutionPersistenceError::Integrity("stored attempt is not a positive u32".to_string())
    })?;
    if attempt == 0 {
        return Err(TaskExecutionPersistenceError::Integrity(
            "stored attempt is not a positive integer".to_string(),
        ));
    }
    let status = parse_status(&status)?;
    let context: crate::modules::task::NodeContext = decode_json("context_json", &context_json)?;
    let validation: Option<crate::modules::task::ValidationResult> = validation_json
        .as_deref()
        .map(|value| decode_json("validation_json", value))
        .transpose()?;
    let output: Option<serde_json::Value> = output_json
        .as_deref()
        .map(|value| decode_json("output_json", value))
        .transpose()?;
    let external_review = external_review_json
        .as_deref()
        .map(|value| decode_json("external_review_json", value))
        .transpose()?;
    let retry_policy: crate::modules::task::execution::ExecutionRetryPolicy =
        decode_json("retry_policy_json", &retry_policy_json)?;
    let executor_ref = executor_ref
        .map(|value| {
            value.parse().map_err(|error| {
                TaskExecutionPersistenceError::Integrity(format!("invalid executor_ref: {error}"))
            })
        })
        .transpose()?;
    let execution = NodeExecution {
        id: execution_id,
        graph_id,
        node_id,
        attempt,
        status,
        executor_ref,
        context,
        output,
        external_review,
        validation,
        audit_ref,
        approval_ref,
        failure_code,
        error,
        retry_policy,
        created_at,
        updated_at,
        started_at,
        finished_at,
    };
    execution
        .context
        .validate()
        .map_err(TaskExecutionPersistenceError::Execution)?;
    Ok(execution)
}

pub(crate) fn insert_node_execution_on(
    conn: &rusqlite::Connection,
    execution: &NodeExecution,
) -> Result<(), TaskExecutionPersistenceError> {
    execution.context.validate()?;
    let attempt = to_i64(execution.attempt, "attempt")?;
    let context_json = encode_json("context_json", &execution.context)?;
    let output_json = execution
        .output
        .as_ref()
        .map(|value| encode_json("output_json", value))
        .transpose()?;
    let validation_json = execution
        .validation
        .as_ref()
        .map(|value| encode_json("validation_json", value))
        .transpose()?;
    let retry_policy_json = encode_json("retry_policy_json", &execution.retry_policy)?;
    let external_review_json = execution
        .external_review
        .as_ref()
        .map(|value| encode_json("external_review_json", value))
        .transpose()?;
    conn.execute(
        "INSERT INTO task_node_executions (
            execution_id, graph_id, node_id, attempt, status, executor_ref,
            context_json, output_json, validation_json, audit_ref, approval_ref,
            failure_code, error, retry_policy_json, created_at, updated_at,
            started_at, finished_at, external_review_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                   ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            execution.id.as_str(),
            execution.graph_id.as_str(),
            execution.node_id.as_str(),
            attempt,
            execution.status.to_string(),
            execution.executor_ref.as_ref().map(ToString::to_string),
            context_json,
            output_json,
            validation_json,
            execution.audit_ref,
            execution.approval_ref,
            execution.failure_code,
            execution.error,
            retry_policy_json,
            execution.created_at,
            execution.updated_at,
            execution.started_at,
            execution.finished_at,
            external_review_json,
        ],
    )?;
    Ok(())
}

pub(crate) fn update_node_execution_on(
    conn: &rusqlite::Connection,
    execution: &NodeExecution,
) -> Result<(), TaskExecutionPersistenceError> {
    execution.context.validate()?;
    let context_json = encode_json("context_json", &execution.context)?;
    let output_json = execution
        .output
        .as_ref()
        .map(|value| encode_json("output_json", value))
        .transpose()?;
    let validation_json = execution
        .validation
        .as_ref()
        .map(|value| encode_json("validation_json", value))
        .transpose()?;
    let retry_policy_json = encode_json("retry_policy_json", &execution.retry_policy)?;
    let external_review_json = execution
        .external_review
        .as_ref()
        .map(|value| encode_json("external_review_json", value))
        .transpose()?;
    let changed = conn.execute(
        "UPDATE task_node_executions SET
            graph_id=?2, node_id=?3, attempt=?4, status=?5, executor_ref=?6,
            context_json=?7, output_json=?8, validation_json=?9, audit_ref=?10,
            approval_ref=?11, failure_code=?12, error=?13, retry_policy_json=?14,
            created_at=?15, updated_at=?16, started_at=?17, finished_at=?18, external_review_json=?19
         WHERE execution_id=?1",
        params![
            execution.id.as_str(),
            execution.graph_id.as_str(),
            execution.node_id.as_str(),
            to_i64(execution.attempt, "attempt")?,
            execution.status.to_string(),
            execution.executor_ref.as_ref().map(ToString::to_string),
            context_json,
            output_json,
            validation_json,
            execution.audit_ref,
            execution.approval_ref,
            execution.failure_code,
            execution.error,
            retry_policy_json,
            execution.created_at,
            execution.updated_at,
            execution.started_at,
            execution.finished_at,
            external_review_json,
        ],
    )?;
    if changed == 0 {
        return Err(TaskExecutionPersistenceError::Integrity(format!(
            "node execution {} does not exist",
            execution.id
        )));
    }
    Ok(())
}

impl Database {
    /// Install migration 1002. The method is idempotent through the shared
    /// migration registrar and is intentionally separate from Database::new
    /// so v1 Task World remains opt-in for legacy callers.
    pub(crate) fn initialize_task_execution_schema(
        &self,
    ) -> Result<(), TaskExecutionPersistenceError> {
        self.apply_product_migration(&TASK_EXECUTION_SCHEMA_MIGRATION)
            .map_err(TaskExecutionPersistenceError::Database)?;
        self.apply_product_migration(&EXTERNAL_REVIEW_MIGRATION)
            .map_err(TaskExecutionPersistenceError::Database)
    }

    pub fn save_node_execution(
        &self,
        execution: &NodeExecution,
    ) -> Result<(), TaskExecutionPersistenceError> {
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        insert_node_execution_on(&transaction, execution)?;
        transaction.commit()?;
        Ok(())
    }

    /// Update evidence for an existing attempt without replacing its identity
    /// or allowing a retry to overwrite the previous row.
    pub fn update_node_execution(
        &self,
        execution: &NodeExecution,
    ) -> Result<(), TaskExecutionPersistenceError> {
        update_node_execution_on(&self.conn(), execution)
    }

    /// A user statement cannot overtake a committed result or a newer state.
    pub(crate) fn record_external_effect_review(
        &self,
        original: &NodeExecution,
        reviewed: &NodeExecution,
    ) -> Result<bool, TaskExecutionPersistenceError> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let unchanged: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_node_executions
             WHERE execution_id=?1 AND updated_at=?2 AND status=?3 AND failure_code=?4)",
            params![
                original.id.as_str(),
                original.updated_at,
                original.status.to_string(),
                original.failure_code
            ],
            |row| row.get(0),
        )?;
        let completed_child: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE parent_execution_id=?1
             AND agent_name='task-harness-workflow' AND status='completed')",
            [original.id.as_str()],
            |row| row.get(0),
        )?;
        if !unchanged || completed_child {
            return Ok(false);
        }
        update_node_execution_on(&tx, reviewed)?;
        tx.commit()?;
        Ok(true)
    }

    /// Recovery changes every affected row or none of them.
    pub(crate) fn update_node_executions_atomically(
        &self,
        executions: &[NodeExecution],
        new_executions: &[NodeExecution],
    ) -> Result<(), TaskExecutionPersistenceError> {
        let mut conn = self.conn();
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for execution in executions {
            update_node_execution_on(&transaction, execution)?;
        }
        for execution in new_executions {
            insert_node_execution_on(&transaction, execution)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn load_node_execution(
        &self,
        execution_id: &NodeExecutionId,
    ) -> Result<Option<NodeExecution>, TaskExecutionPersistenceError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                SELECT_NODE_EXECUTION,
                params![execution_id.as_str()],
                |row| row_from_sql(row),
            )
            .optional()?;
        row.transpose()
    }

    pub fn load_node_executions(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
    ) -> Result<Vec<NodeExecution>, TaskExecutionPersistenceError> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT execution_id, graph_id, node_id, attempt, status, executor_ref,
                    context_json, output_json, validation_json, audit_ref, approval_ref,
                    failure_code, error, retry_policy_json, created_at, updated_at,
                    started_at, finished_at, external_review_json
             FROM task_node_executions
             WHERE graph_id=?1 AND node_id=?2
             ORDER BY attempt, execution_id",
        )?;
        let rows = statement
            .query_map(params![graph_id.as_str(), node_id.as_str()], |row| {
                row_from_sql(row)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().collect()
    }

    pub fn load_all_node_executions(
        &self,
    ) -> Result<Vec<NodeExecution>, TaskExecutionPersistenceError> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT execution_id, graph_id, node_id, attempt, status, executor_ref,
                    context_json, output_json, validation_json, audit_ref, approval_ref,
                    failure_code, error, retry_policy_json, created_at, updated_at,
                    started_at, finished_at, external_review_json
             FROM task_node_executions
             ORDER BY graph_id, node_id, attempt, execution_id",
        )?;
        let rows = statement
            .query_map([], row_from_sql)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().collect()
    }

    /// Whether the existing gateway journal records a possible mutation for
    /// this attempt or its owned workflow. Legacy events retain their recorded
    /// permission-based classification when detailed effect metadata is absent.
    pub(crate) fn node_execution_has_started_effect(
        &self,
        execution_id: &NodeExecutionId,
    ) -> Result<bool, TaskExecutionPersistenceError> {
        Ok(self.conn().query_row(
            "SELECT EXISTS(
               SELECT 1 FROM security_audit_events e
               WHERE e.event_type='execution_started'
                 AND (e.tool_call_id=?1 OR e.conversation_id IN (
                   SELECT execution_id FROM workflow_runs WHERE parent_execution_id=?1
                 ))
                 AND COALESCE(json_extract(e.details_json,'$.context.effect_may_change_state'),
                   EXISTS(SELECT 1 FROM json_each(e.actions_json) a WHERE a.value IN (
                     'filesystem.write','shell.execute','process.control','desktop.interact',
                     'network.request','agent.memory.write','mcp.invoke','agent.delegate','messaging.send'
                   )))=1
             )",
            [execution_id.as_str()], |row| row.get(0),
        )?)
    }

    /// Normalize interrupted and cancelled-with-unknown-effect rows at startup.
    pub fn recover_node_executions(
        &self,
        now: i64,
    ) -> Result<usize, TaskExecutionPersistenceError> {
        let mut recovered = 0;
        for mut execution in self.load_all_node_executions()? {
            if execution.status.is_active() {
                execution.recover_after_restart(now);
                self.update_node_execution(&execution)?;
                recovered += 1;
            } else if execution.status == NodeExecutionStatus::Cancelled
                && matches!(execution.failure_code(), None | Some("paused"))
                && self.node_execution_has_started_effect(&execution.id)?
                && execution.record_uncertain_pause(now)
            {
                self.update_node_execution(&execution)?;
                recovered += 1;
            }
        }
        Ok(recovered)
    }
}

const SELECT_NODE_EXECUTION: &str = "SELECT execution_id, graph_id, node_id, attempt, status,
    executor_ref, context_json, output_json, validation_json, audit_ref, approval_ref,
    failure_code, error, retry_policy_json, created_at, updated_at, started_at, finished_at, external_review_json
    FROM task_node_executions WHERE execution_id=?1";

fn row_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<NodeExecution, TaskExecutionPersistenceError>> {
    Ok(row_to_execution((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
        row.get(15)?,
        row.get(16)?,
        row.get(17)?,
        row.get(18)?,
    )))
}

#[cfg(test)]
mod tests;
