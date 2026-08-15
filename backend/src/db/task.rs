// ============================================================
// Task domain persistence — tasks, executions, plans, events,
// artifacts, agent definitions/teams/executions, decisions.
// ============================================================

use std::str::FromStr;

use super::Database;
use crate::execution::{ExecutionContext, ExecutionId};
use crate::task::*;
use crate::workflow::WorkflowRunId;
use crate::workspace::WorkspaceId;

fn encode_json(value: &impl serde::Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn decode_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, String> {
    serde_json::from_str(value).map_err(|error| error.to_string())
}

fn parse_opt_json<T: serde::de::DeserializeOwned>(opt: Option<String>) -> Vec<T> {
    opt.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn status_from_str<T: std::str::FromStr>(value: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    value.parse().map_err(|error: T::Err| error.to_string())
}

impl FromStr for TaskStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "draft" => Self::Draft,
            "ready" => Self::Ready,
            "running" => Self::Running,
            "waiting_approval" => Self::WaitingApproval,
            "waiting_user" => Self::WaitingUser,
            "blocked" => Self::Blocked,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            other => return Err(format!("unknown task status: {other}")),
        })
    }
}

impl FromStr for TaskPriority {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "low" => Self::Low,
            "normal" => Self::Normal,
            "high" => Self::High,
            other => return Err(format!("unknown task priority: {other}")),
        })
    }
}

impl FromStr for TaskExecutionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "created" => Self::Created,
            "running" => Self::Running,
            "waiting_approval" => Self::WaitingApproval,
            "waiting_user" => Self::WaitingUser,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "interrupted" => Self::Interrupted,
            other => return Err(format!("unknown execution status: {other}")),
        })
    }
}

impl FromStr for AgentExecutionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "created" => Self::Created,
            "running" => Self::Running,
            "waiting_approval" => Self::WaitingApproval,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "interrupted" => Self::Interrupted,
            other => return Err(format!("unknown agent execution status: {other}")),
        })
    }
}

impl FromStr for AgentSource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "builtin" => Self::Builtin,
            "local_file" => Self::LocalFile,
            "database" => Self::Database,
            other => return Err(format!("unknown agent source: {other}")),
        })
    }
}

impl FromStr for ArtifactType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "file" => Self::File,
            "document" => Self::Document,
            "code" => Self::Code,
            "report" => Self::Report,
            "image" => Self::Image,
            "data" => Self::Data,
            "text" => Self::Text,
            "other" => Self::Other,
            other => return Err(format!("unknown artifact type: {other}")),
        })
    }
}

impl FromStr for TaskDecisionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "pending" => Self::Pending,
            "resolved" => Self::Resolved,
            other => return Err(format!("unknown task decision status: {other}")),
        })
    }
}

impl FromStr for TaskEventType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "task_created" => Self::TaskCreated,
            "task_updated" => Self::TaskUpdated,
            "task_started" => Self::TaskStarted,
            "plan_created" => Self::PlanCreated,
            "execution_started" => Self::ExecutionStarted,
            "execution_completed" => Self::ExecutionCompleted,
            "execution_failed" => Self::ExecutionFailed,
            "execution_cancelled" => Self::ExecutionCancelled,
            "execution_interrupted" => Self::ExecutionInterrupted,
            "agent_delegated" => Self::AgentDelegated,
            "agent_completed" => Self::AgentCompleted,
            "agent_failed" => Self::AgentFailed,
            "workflow_started" => Self::WorkflowStarted,
            "workflow_completed" => Self::WorkflowCompleted,
            "approval_required" => Self::ApprovalRequired,
            "approval_resolved" => Self::ApprovalResolved,
            "user_decision_required" => Self::UserDecisionRequired,
            "user_decision_resolved" => Self::UserDecisionResolved,
            "artifact_created" => Self::ArtifactCreated,
            "task_completed" => Self::TaskCompleted,
            "task_failed" => Self::TaskFailed,
            "task_cancelled" => Self::TaskCancelled,
            other => return Err(format!("unknown task event type: {other}")),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskQuery {
    pub workspace_id: Option<WorkspaceId>,
    pub status: Option<TaskStatus>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactQuery {
    pub workspace_id: Option<WorkspaceId>,
    pub task_id: Option<TaskId>,
    pub task_execution_id: Option<TaskExecutionId>,
    pub limit: Option<usize>,
}

impl Database {
    // ── Tasks ──

    pub fn create_task(&self, task: &Task) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO tasks (id, workspace_id, title, description, status, priority,
                workflow_graph_id, agent_team_id, created_at, updated_at, completed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![
                task.id.as_str(),
                task.workspace_id.as_str(),
                task.title,
                task.description,
                task.status.to_string(),
                task.priority.to_string(),
                task.workflow_graph_id,
                task.agent_team_id.as_ref().map(|id| id.as_str()),
                task.created_at,
                task.updated_at,
                task.completed_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_task(&self, id: &TaskId) -> Result<Option<Task>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_id, title, description, status, priority,
                    workflow_graph_id, agent_team_id, created_at, updated_at, completed_at
                 FROM tasks WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_task_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_tasks(&self, query: &TaskQuery) -> Result<Vec<Task>, String> {
        let conn = self.conn();
        let mut sql = String::from(
            "SELECT id, workspace_id, title, description, status, priority,
                    workflow_graph_id, agent_team_id, created_at, updated_at, completed_at
             FROM tasks WHERE 1 = 1",
        );
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(workspace_id) = &query.workspace_id {
            sql.push_str(&format!(" AND workspace_id = ?{}", values.len() + 1));
            values.push(Box::new(workspace_id.as_str()));
        }
        if let Some(status) = &query.status {
            sql.push_str(&format!(" AND status = ?{}", values.len() + 1));
            values.push(Box::new(status.to_string()));
        }
        sql.push_str(" ORDER BY updated_at DESC, id DESC");
        let limit = query.limit.unwrap_or(50).clamp(1, 200) as i64;
        sql.push_str(&format!(" LIMIT ?{}", values.len() + 1));
        values.push(Box::new(limit));
        let offset = query.offset.unwrap_or(0) as i64;
        sql.push_str(&format!(" OFFSET ?{}", values.len() + 1));
        values.push(Box::new(offset));

        let refs = values.iter().map(|v| v.as_ref()).collect::<Vec<_>>();
        let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(refs.as_slice(), map_task_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    pub fn delete_task(&self, id: &TaskId) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM tasks WHERE id = ?1",
            rusqlite::params![id.as_str()],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn update_task(&self, task: &Task) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE tasks SET title=?2, description=?3, status=?4, priority=?5,
                workflow_graph_id=?6, agent_team_id=?7, updated_at=?8, completed_at=?9
             WHERE id=?1",
            rusqlite::params![
                task.id.as_str(),
                task.title,
                task.description,
                task.status.to_string(),
                task.priority.to_string(),
                task.workflow_graph_id,
                task.agent_team_id.as_ref().map(|id| id.as_str()),
                task.updated_at,
                task.completed_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    // ── Task executions ──

    pub fn create_task_execution(&self, execution: &TaskExecution) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO task_executions (id, task_id, execution_id, subject_id, agent_name,
                parent_execution_id, workflow_run_id, agent_id, agent_team_id, status, attempt,
                started_at, finished_at, error, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            rusqlite::params![
                execution.id.as_str(),
                execution.task_id.as_str(),
                execution.execution_context.execution_id.as_str(),
                execution.execution_context.subject_id,
                execution.execution_context.agent_name,
                execution
                    .execution_context
                    .parent_execution_id
                    .as_ref()
                    .map(|id| id.as_str()),
                execution.workflow_run_id.as_ref().map(|id| id.as_str()),
                execution.agent_id.as_ref().map(|id| id.as_str()),
                execution.agent_team_id.as_ref().map(|id| id.as_str()),
                execution.status.to_string(),
                execution.attempt,
                execution.started_at,
                execution.finished_at,
                execution.error,
                execution.created_at,
                execution.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_task_execution(
        &self,
        id: &TaskExecutionId,
    ) -> Result<Option<TaskExecution>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, execution_id, subject_id, agent_name, parent_execution_id,
                    workflow_run_id, agent_id, agent_team_id, status, attempt, started_at,
                    finished_at, error, created_at, updated_at
                 FROM task_executions WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_task_execution_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_task_executions(&self, task_id: &TaskId) -> Result<Vec<TaskExecution>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, execution_id, subject_id, agent_name, parent_execution_id,
                    workflow_run_id, agent_id, agent_team_id, status, attempt, started_at,
                    finished_at, error, created_at, updated_at
                 FROM task_executions WHERE task_id = ?1 ORDER BY created_at ASC, id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![task_id.as_str()], map_task_execution_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    pub fn update_task_execution(&self, execution: &TaskExecution) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE task_executions SET workflow_run_id=?2, agent_id=?3, agent_team_id=?4,
                status=?5, attempt=?6, started_at=?7, finished_at=?8, error=?9, updated_at=?10
             WHERE id=?1",
            rusqlite::params![
                execution.id.as_str(),
                execution.workflow_run_id.as_ref().map(|id| id.as_str()),
                execution.agent_id.as_ref().map(|id| id.as_str()),
                execution.agent_team_id.as_ref().map(|id| id.as_str()),
                execution.status.to_string(),
                execution.attempt,
                execution.started_at,
                execution.finished_at,
                execution.error,
                execution.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Find any execution of a task that is not terminal (used to reject a
    /// second concurrent active execution).
    pub fn find_active_task_execution(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<TaskExecution>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, execution_id, subject_id, agent_name, parent_execution_id,
                    workflow_run_id, agent_id, agent_team_id, status, attempt, started_at,
                    finished_at, error, created_at, updated_at
                 FROM task_executions
                 WHERE task_id = ?1 AND status NOT IN
                    ('completed','failed','cancelled','interrupted')
                 ORDER BY created_at ASC LIMIT 1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![task_id.as_str()], map_task_execution_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    /// Mark every running execution of a task as interrupted (recovery scan).
    pub fn interrupt_running_executions(&self, now: i64) -> Result<usize, String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE task_executions SET status='interrupted', finished_at=?1, updated_at=?1
             WHERE status='running' OR status='created'",
            [now],
        )
        .map(|count| count as usize)
        .map_err(|error| error.to_string())
    }

    // ── Task plans ──

    pub fn create_task_plan(
        &self,
        id: &str,
        task_id: &TaskId,
        task_execution_id: &TaskExecutionId,
        plan: &TaskPlan,
        now: i64,
    ) -> Result<(), String> {
        let plan_json = encode_json(plan)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO task_plans (id, task_id, task_execution_id, schema_version, plan_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![
                id,
                task_id.as_str(),
                task_execution_id.as_str(),
                plan.schema_version,
                plan_json,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_latest_task_plan(
        &self,
        task_id: &TaskId,
        task_execution_id: &TaskExecutionId,
    ) -> Result<Option<TaskPlan>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT plan_json FROM task_plans
                 WHERE task_id = ?1 AND task_execution_id = ?2
                 ORDER BY created_at ASC, id ASC LIMIT 1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(
                rusqlite::params![task_id.as_str(), task_execution_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => {
                let plan_json = row.map_err(|error| error.to_string())?;
                Ok(Some(decode_json(&plan_json)?))
            }
            None => Ok(None),
        }
    }

    // ── Artifacts ──

    pub fn create_artifact(&self, artifact: &Artifact) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO artifacts (id, workspace_id, task_id, task_execution_id, name,
                artifact_type, path, mime_type, size, summary, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            rusqlite::params![
                artifact.id.as_str(),
                artifact.workspace_id.as_str(),
                artifact.task_id.as_str(),
                artifact.task_execution_id.as_str(),
                artifact.name,
                artifact.artifact_type.to_string(),
                artifact.path,
                artifact.mime_type,
                artifact.size,
                artifact.summary,
                artifact.created_at,
                artifact.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_artifact(&self, id: &ArtifactId) -> Result<Option<Artifact>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_id, task_id, task_execution_id, name, artifact_type,
                    path, mime_type, size, summary, created_at, updated_at
                 FROM artifacts WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_artifact_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_artifacts(&self, query: &ArtifactQuery) -> Result<Vec<Artifact>, String> {
        let conn = self.conn();
        let mut sql = String::from(
            "SELECT id, workspace_id, task_id, task_execution_id, name, artifact_type,
                    path, mime_type, size, summary, created_at, updated_at
             FROM artifacts WHERE 1 = 1",
        );
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(workspace_id) = &query.workspace_id {
            sql.push_str(&format!(" AND workspace_id = ?{}", values.len() + 1));
            values.push(Box::new(workspace_id.as_str()));
        }
        if let Some(task_id) = &query.task_id {
            sql.push_str(&format!(" AND task_id = ?{}", values.len() + 1));
            values.push(Box::new(task_id.as_str()));
        }
        if let Some(execution_id) = &query.task_execution_id {
            sql.push_str(&format!(" AND task_execution_id = ?{}", values.len() + 1));
            values.push(Box::new(execution_id.as_str()));
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC");
        let limit = query.limit.unwrap_or(50).clamp(1, 200) as i64;
        sql.push_str(&format!(" LIMIT ?{}", values.len() + 1));
        values.push(Box::new(limit));

        let refs = values.iter().map(|v| v.as_ref()).collect::<Vec<_>>();
        let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(refs.as_slice(), map_artifact_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    // ── Task events (timeline) ──

    pub fn create_task_event(&self, event: &TaskEvent) -> Result<(), String> {
        let metadata_json = encode_json(&event.metadata)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO task_events (id, workspace_id, task_id, task_execution_id, event_type,
                message, metadata_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                event.id,
                event.workspace_id.as_str(),
                event.task_id.as_str(),
                event.task_execution_id.as_ref().map(|id| id.as_str()),
                event.event_type.to_string(),
                event.message,
                metadata_json,
                event.created_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// List timeline events oldest → newest (for readable task progression).
    pub fn list_task_events(&self, task_id: &TaskId) -> Result<Vec<TaskEvent>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_id, task_id, task_execution_id, event_type, message,
                    metadata_json, created_at
                 FROM task_events WHERE task_id = ?1 ORDER BY created_at ASC, id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![task_id.as_str()], map_task_event_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    // ── Agent definitions ──

    pub fn create_agent_definition(&self, agent: &AgentDefinition) -> Result<(), String> {
        let allowed_tools = encode_json(&agent.allowed_tools)?;
        let capabilities = encode_json(&agent.capabilities)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO agent_definitions (id, name, description, instructions, allowed_tools,
                model, capabilities, max_iterations, enabled, source, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            rusqlite::params![
                agent.id.as_str(),
                agent.name,
                agent.description,
                agent.instructions,
                allowed_tools,
                agent.model,
                capabilities,
                agent.max_iterations,
                agent.enabled as i32,
                agent.source.to_string(),
                now_ms(),
                now_ms(),
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_agent_definition(&self, id: &AgentId) -> Result<Option<AgentDefinition>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, instructions, allowed_tools, model, capabilities,
                    max_iterations, enabled, source
                 FROM agent_definitions WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_agent_definition_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_agent_definitions(&self) -> Result<Vec<AgentDefinition>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, instructions, allowed_tools, model, capabilities,
                    max_iterations, enabled, source
                 FROM agent_definitions ORDER BY name ASC, id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], map_agent_definition_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    // ── Agent teams ──

    pub fn create_agent_team(&self, team: &AgentTeam) -> Result<(), String> {
        let member_ids = encode_json(&team.member_agent_ids)?;
        let policy_json = encode_json(&team.delegation_policy)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO agent_teams (id, name, description, coordinator_agent_id,
                member_agent_ids, delegation_policy_json, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                team.id.as_str(),
                team.name,
                team.description,
                team.coordinator_agent_id.as_str(),
                member_ids,
                policy_json,
                team.created_at,
                team.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_agent_team(&self, id: &AgentTeamId) -> Result<Option<AgentTeam>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, coordinator_agent_id, member_agent_ids,
                    delegation_policy_json, created_at, updated_at
                 FROM agent_teams WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_agent_team_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_agent_teams(&self) -> Result<Vec<AgentTeam>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, coordinator_agent_id, member_agent_ids,
                    delegation_policy_json, created_at, updated_at
                 FROM agent_teams ORDER BY name ASC, id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], map_agent_team_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    // ── Agent executions ──

    pub fn create_agent_execution(&self, execution: &AgentExecution) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO agent_executions (id, task_execution_id, agent_id,
                parent_agent_execution_id, depth, instruction, status, result_summary,
                agent_state_json, started_at, finished_at, error, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            rusqlite::params![
                execution.id.as_str(),
                execution.task_execution_id.as_str(),
                execution.agent_id.as_str(),
                execution
                    .parent_agent_execution_id
                    .as_ref()
                    .map(|id| id.as_str()),
                execution.depth,
                execution.instruction,
                execution.status.to_string(),
                execution.result_summary,
                execution.agent_state_json,
                execution.started_at,
                execution.finished_at,
                execution.error,
                execution.created_at,
                execution.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_agent_execution(
        &self,
        id: &AgentExecutionId,
    ) -> Result<Option<AgentExecution>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_execution_id, agent_id, parent_agent_execution_id, depth,
                    instruction, status, result_summary, agent_state_json, started_at,
                    finished_at, error, created_at, updated_at
                 FROM agent_executions WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_agent_execution_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_agent_executions(
        &self,
        task_execution_id: &TaskExecutionId,
    ) -> Result<Vec<AgentExecution>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_execution_id, agent_id, parent_agent_execution_id, depth,
                    instruction, status, result_summary, agent_state_json, started_at,
                    finished_at, error, created_at, updated_at
                 FROM agent_executions WHERE task_execution_id = ?1 ORDER BY created_at ASC, id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(
                rusqlite::params![task_execution_id.as_str()],
                map_agent_execution_row,
            )
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    pub fn count_agent_executions(
        &self,
        task_execution_id: &TaskExecutionId,
    ) -> Result<usize, String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT COUNT(*) FROM agent_executions WHERE task_execution_id = ?1",
            [task_execution_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count as usize)
        .map_err(|error| error.to_string())
    }

    pub fn update_agent_execution(&self, execution: &AgentExecution) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE agent_executions SET status=?2, result_summary=?3, agent_state_json=?4,
                started_at=?5, finished_at=?6, error=?7, updated_at=?8
             WHERE id=?1",
            rusqlite::params![
                execution.id.as_str(),
                execution.status.to_string(),
                execution.result_summary,
                execution.agent_state_json,
                execution.started_at,
                execution.finished_at,
                execution.error,
                execution.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Interrupt every running agent execution (recovery scan).
    pub fn interrupt_running_agent_executions(&self, now: i64) -> Result<usize, String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE agent_executions SET status='interrupted', finished_at=?1, updated_at=?1
             WHERE status='running' OR status='created'",
            [now],
        )
        .map(|count| count as usize)
        .map_err(|error| error.to_string())
    }

    // ── Task decisions ──

    pub fn create_task_decision(&self, decision: &TaskDecision) -> Result<(), String> {
        let options_json = encode_json(&decision.options)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO task_decisions (id, task_id, task_execution_id, prompt, options_json,
                status, created_at, resolved_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                decision.id.as_str(),
                decision.task_id.as_str(),
                decision.task_execution_id.as_str(),
                decision.prompt,
                options_json,
                decision.status.to_string(),
                decision.created_at,
                decision.resolved_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_task_decision(&self, id: &TaskDecisionId) -> Result<Option<TaskDecision>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, task_execution_id, prompt, options_json, status,
                    created_at, resolved_at
                 FROM task_decisions WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt
            .query_map(rusqlite::params![id.as_str()], map_task_decision_row)
            .map_err(|error| error.to_string())?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|error| error.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn list_pending_task_decisions(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<TaskDecision>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, task_execution_id, prompt, options_json, status,
                    created_at, resolved_at
                 FROM task_decisions WHERE task_id = ?1 AND status='pending'
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![task_id.as_str()], map_task_decision_row)
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    pub fn resolve_task_decision(
        &self,
        id: &TaskDecisionId,
        resolved_at: i64,
    ) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "UPDATE task_decisions SET status='resolved', resolved_at=?2 WHERE id=?1 AND status='pending'",
            rusqlite::params![id.as_str(), resolved_at],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let status: String = row.get(4)?;
    let priority: String = row.get(5)?;
    Ok(Task {
        id: TaskId::new(row.get::<_, String>(0)?).unwrap(),
        workspace_id: WorkspaceId::new(row.get::<_, String>(1)?).unwrap(),
        title: row.get(2)?,
        description: row.get(3)?,
        status: status_from_str(&status).map_err(conv_err(4))?,
        priority: status_from_str(&priority).map_err(conv_err(5))?,
        workflow_graph_id: row.get(6)?,
        agent_team_id: row
            .get::<_, Option<String>>(7)?
            .map(|s| AgentTeamId::new(s).unwrap()),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

fn map_task_execution_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskExecution> {
    let status: String = row.get(9)?;
    let execution_context = ExecutionContext::new(
        ExecutionId::new(row.get::<_, String>(2)?).unwrap(),
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, Option<String>>(5)?
            .map(|s| ExecutionId::new(s).unwrap()),
        row.get::<_, i64>(14)?,
    );
    Ok(TaskExecution {
        id: TaskExecutionId::new(row.get::<_, String>(0)?).unwrap(),
        task_id: TaskId::new(row.get::<_, String>(1)?).unwrap(),
        execution_context,
        workflow_run_id: row
            .get::<_, Option<String>>(6)?
            .map(|s| WorkflowRunId::new(s).unwrap()),
        agent_id: row
            .get::<_, Option<String>>(7)?
            .map(|s| AgentId::new(s).unwrap()),
        agent_team_id: row
            .get::<_, Option<String>>(8)?
            .map(|s| AgentTeamId::new(s).unwrap()),
        status: status_from_str(&status).map_err(conv_err(9))?,
        attempt: row.get(10)?,
        started_at: row.get(11)?,
        finished_at: row.get(12)?,
        error: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn map_artifact_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    let artifact_type: String = row.get(5)?;
    Ok(Artifact {
        id: ArtifactId::new(row.get::<_, String>(0)?).unwrap(),
        workspace_id: WorkspaceId::new(row.get::<_, String>(1)?).unwrap(),
        task_id: TaskId::new(row.get::<_, String>(2)?).unwrap(),
        task_execution_id: TaskExecutionId::new(row.get::<_, String>(3)?).unwrap(),
        name: row.get(4)?,
        artifact_type: status_from_str(&artifact_type).map_err(conv_err(5))?,
        path: row.get(6)?,
        mime_type: row.get(7)?,
        size: row.get(8)?,
        summary: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn map_task_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskEvent> {
    let event_type: String = row.get(4)?;
    Ok(TaskEvent {
        id: row.get(0)?,
        workspace_id: WorkspaceId::new(row.get::<_, String>(1)?).unwrap(),
        task_id: TaskId::new(row.get::<_, String>(2)?).unwrap(),
        task_execution_id: row
            .get::<_, Option<String>>(3)?
            .map(|s| TaskExecutionId::new(s).unwrap()),
        event_type: status_from_str(&event_type).map_err(conv_err(4))?,
        message: row.get(5)?,
        metadata: row
            .get::<_, Option<String>>(6)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({})),
        created_at: row.get(7)?,
    })
}

fn map_agent_definition_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDefinition> {
    let source: String = row.get(9)?;
    Ok(AgentDefinition {
        id: AgentId::new(row.get::<_, String>(0)?).unwrap(),
        name: row.get(1)?,
        description: row.get(2)?,
        instructions: row.get(3)?,
        allowed_tools: parse_opt_json(row.get(4)?),
        model: row.get(5)?,
        capabilities: parse_opt_json(row.get(6)?),
        max_iterations: row.get(7)?,
        enabled: row.get::<_, i32>(8)? != 0,
        source: status_from_str(&source).map_err(conv_err(9))?,
    })
}

fn map_agent_team_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTeam> {
    Ok(AgentTeam {
        id: AgentTeamId::new(row.get::<_, String>(0)?).unwrap(),
        name: row.get(1)?,
        description: row.get(2)?,
        coordinator_agent_id: AgentId::new(row.get::<_, String>(3)?).unwrap(),
        member_agent_ids: parse_opt_json(row.get(4)?),
        delegation_policy: row
            .get::<_, Option<String>>(5)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn map_agent_execution_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentExecution> {
    let status: String = row.get(6)?;
    Ok(AgentExecution {
        id: AgentExecutionId::new(row.get::<_, String>(0)?).unwrap(),
        task_execution_id: TaskExecutionId::new(row.get::<_, String>(1)?).unwrap(),
        agent_id: AgentId::new(row.get::<_, String>(2)?).unwrap(),
        parent_agent_execution_id: row
            .get::<_, Option<String>>(3)?
            .map(|s| AgentExecutionId::new(s).unwrap()),
        depth: row.get(4)?,
        instruction: row.get(5)?,
        status: status_from_str(&status).map_err(conv_err(6))?,
        result_summary: row.get(7)?,
        agent_state_json: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
        error: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn map_task_decision_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskDecision> {
    let status: String = row.get(5)?;
    Ok(TaskDecision {
        id: TaskDecisionId::new(row.get::<_, String>(0)?).unwrap(),
        task_id: TaskId::new(row.get::<_, String>(1)?).unwrap(),
        task_execution_id: TaskExecutionId::new(row.get::<_, String>(2)?).unwrap(),
        prompt: row.get(3)?,
        options: row
            .get::<_, Option<String>>(4)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        status: status_from_str(&status).map_err(conv_err(5))?,
        created_at: row.get(6)?,
        resolved_at: row.get(7)?,
    })
}

fn conv_err<E: std::fmt::Display>(column: usize) -> impl Fn(E) -> rusqlite::Error {
    move |error: E| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    }
}
