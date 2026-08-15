// ============================================================
// Task domain model — the product layer above WorkflowRun.
//
// A Task has one-or-many TaskExecutions (each attempt/history preserved), a
// TaskPlan produced by a Planner, Artifacts, a user-facing Timeline, and
// optional Agent teams. Trusted Execution (SecurityExecutionGateway) remains
// the only boundary that authorizes tool / MCP / subagent side effects.
// ============================================================

use serde::{Deserialize, Serialize};

use crate::execution::ExecutionContext;
use crate::workflow::WorkflowRunId;
use crate::workspace::WorkspaceId;

// ── Identifiers ──

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(raw: impl Into<String>) -> Result<Self, TaskError> {
                let raw = raw.into();
                if raw.is_empty() || raw.chars().any(char::is_control) {
                    return Err(TaskError::InvalidId(stringify!($name)));
                }
                Ok(Self(raw))
            }

            pub fn generate() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_newtype!(TaskId);
id_newtype!(TaskExecutionId);
id_newtype!(ArtifactId);
id_newtype!(AgentId);
id_newtype!(AgentTeamId);
id_newtype!(AgentExecutionId);
id_newtype!(TaskDecisionId);

// ── Task ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    Ready,
    Running,
    WaitingApproval,
    WaitingUser,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::WaitingUser => "waiting_user",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        })
    }
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Normal,
    High,
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        })
    }
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub workspace_id: WorkspaceId,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub workflow_graph_id: Option<String>,
    pub agent_team_id: Option<AgentTeamId>,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

impl Task {
    pub fn new(
        id: TaskId,
        workspace_id: WorkspaceId,
        title: String,
        description: String,
        priority: TaskPriority,
        workflow_graph_id: Option<String>,
        agent_team_id: Option<AgentTeamId>,
        now: i64,
    ) -> Result<Self, TaskError> {
        validate_task_fields(&title, &description)?;
        Ok(Self {
            id,
            workspace_id,
            title: title.trim().to_string(),
            description: description.trim().to_string(),
            status: TaskStatus::Draft,
            priority,
            workflow_graph_id,
            agent_team_id,
            created_at: now,
            updated_at: now,
            completed_at: None,
        })
    }
}

// ── Task execution ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionStatus {
    Created,
    Running,
    WaitingApproval,
    WaitingUser,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl std::fmt::Display for TaskExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::WaitingUser => "waiting_user",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        })
    }
}

impl TaskExecutionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: TaskExecutionId,
    pub task_id: TaskId,
    pub execution_context: ExecutionContext,
    pub workflow_run_id: Option<WorkflowRunId>,
    pub agent_id: Option<AgentId>,
    pub agent_team_id: Option<AgentTeamId>,
    pub status: TaskExecutionStatus,
    pub attempt: u32,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TaskExecution {
    pub fn new(
        id: TaskExecutionId,
        task_id: TaskId,
        execution_context: ExecutionContext,
        attempt: u32,
        now: i64,
    ) -> Self {
        Self {
            id,
            task_id,
            execution_context,
            workflow_run_id: None,
            agent_id: None,
            agent_team_id: None,
            status: TaskExecutionStatus::Created,
            attempt,
            started_at: None,
            finished_at: None,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }
}

// ── Task plan ──

pub const MAX_TASK_PLAN_STEPS: usize = 20;
pub const MAX_TASK_PLAN_SUMMARY_CHARS: usize = 4000;
pub const MAX_TASK_PLAN_STEP_TITLE_CHARS: usize = 200;
pub const MAX_TASK_PLAN_STEP_INSTRUCTION_CHARS: usize = 4000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPlan {
    pub schema_version: u32,
    pub summary: String,
    pub steps: Vec<TaskPlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPlanStep {
    pub id: String,
    pub title: String,
    pub instruction: String,
    pub executor: TaskPlanExecutor,
}

/// The executor for a plan step. Planner output is deliberately restricted: no
/// raw tool / script / shell steps. Executors are Agent, Workflow, or Subagent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskPlanExecutor {
    Agent { agent_id: AgentId },
    Workflow { workflow_graph_id: String },
    Subagent { name: String },
}

// ── Task event (timeline) ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEventType {
    TaskCreated,
    TaskUpdated,
    TaskStarted,
    PlanCreated,
    ExecutionStarted,
    ExecutionCompleted,
    ExecutionFailed,
    ExecutionCancelled,
    ExecutionInterrupted,
    AgentDelegated,
    AgentCompleted,
    AgentFailed,
    WorkflowStarted,
    WorkflowCompleted,
    ApprovalRequired,
    ApprovalResolved,
    UserDecisionRequired,
    UserDecisionResolved,
    ArtifactCreated,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
}

impl std::fmt::Display for TaskEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_default();
        f.write_str(s.trim_matches('"'))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEvent {
    pub id: String,
    pub workspace_id: WorkspaceId,
    pub task_id: TaskId,
    pub task_execution_id: Option<TaskExecutionId>,
    pub event_type: TaskEventType,
    pub message: String,
    pub metadata: serde_json::Value,
    pub created_at: i64,
}

// ── Artifact ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    File,
    Document,
    Code,
    Report,
    Image,
    Data,
    Text,
    Other,
}

impl std::fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::File => "file",
            Self::Document => "document",
            Self::Code => "code",
            Self::Report => "report",
            Self::Image => "image",
            Self::Data => "data",
            Self::Text => "text",
            Self::Other => "other",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub workspace_id: WorkspaceId,
    pub task_id: TaskId,
    pub task_execution_id: TaskExecutionId,
    pub name: String,
    pub artifact_type: ArtifactType,
    pub path: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    pub summary: String,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Agent definition / team ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    Builtin,
    LocalFile,
    Database,
}

impl std::fmt::Display for AgentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Builtin => "builtin",
            Self::LocalFile => "local_file",
            Self::Database => "database",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: AgentId,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub capabilities: Vec<String>,
    pub max_iterations: u32,
    pub enabled: bool,
    pub source: AgentSource,
}

pub const MAX_DELEGATION_DEPTH: u32 = 3;
pub const MAX_AGENT_EXECUTIONS_PER_TASK: u32 = 12;
pub const MAX_TOTAL_AGENT_ITERATIONS: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationPolicy {
    pub max_depth: u32,
    pub max_agent_executions: u32,
    pub max_total_iterations: u32,
}

impl Default for DelegationPolicy {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_agent_executions: MAX_AGENT_EXECUTIONS_PER_TASK,
            max_total_iterations: MAX_TOTAL_AGENT_ITERATIONS,
        }
    }
}

impl DelegationPolicy {
    /// Clamp any user-supplied policy to the hard limits.
    pub fn clamped(mut self) -> Self {
        self.max_depth = self.max_depth.min(MAX_DELEGATION_DEPTH);
        self.max_agent_executions = self.max_agent_executions.min(MAX_AGENT_EXECUTIONS_PER_TASK);
        self.max_total_iterations = self.max_total_iterations.min(MAX_TOTAL_AGENT_ITERATIONS);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTeam {
    pub id: AgentTeamId,
    pub name: String,
    pub description: String,
    pub coordinator_agent_id: AgentId,
    pub member_agent_ids: Vec<AgentId>,
    pub delegation_policy: DelegationPolicy,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Agent execution ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentExecutionStatus {
    Created,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl std::fmt::Display for AgentExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        })
    }
}

impl AgentExecutionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExecution {
    pub id: AgentExecutionId,
    pub task_execution_id: TaskExecutionId,
    pub agent_id: AgentId,
    pub parent_agent_execution_id: Option<AgentExecutionId>,
    pub depth: u32,
    pub instruction: String,
    pub status: AgentExecutionStatus,
    pub result_summary: Option<String>,
    pub agent_state_json: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ── Task decision ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDecisionStatus {
    Pending,
    Resolved,
}

impl std::fmt::Display for TaskDecisionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDecisionOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDecision {
    pub id: TaskDecisionId,
    pub task_id: TaskId,
    pub task_execution_id: TaskExecutionId,
    pub prompt: String,
    pub options: Vec<TaskDecisionOption>,
    pub status: TaskDecisionStatus,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

// ── Errors + field validation ──

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaskError {
    #[error("invalid {0} id")]
    InvalidId(&'static str),
    #[error("task title must not be empty")]
    EmptyTitle,
    #[error("task title exceeds 200 characters")]
    TitleTooLong,
    #[error("task description exceeds 4000 characters")]
    DescriptionTooLong,
    #[error("invalid state transition for task {task_id}: {from} -> {to}")]
    InvalidTaskTransition {
        task_id: TaskId,
        from: TaskStatus,
        to: TaskStatus,
    },
    #[error("invalid state transition for execution {execution_id}: {from} -> {to}")]
    InvalidExecutionTransition {
        execution_id: TaskExecutionId,
        from: TaskExecutionStatus,
        to: TaskExecutionStatus,
    },
    #[error("unknown task: {0}")]
    UnknownTask(String),
    #[error("unknown task execution: {0}")]
    UnknownExecution(String),
    #[error("task already has an active execution")]
    AlreadyActive,
    #[error("plan validation failed: {0}")]
    InvalidPlan(String),
}

pub const MAX_TASK_TITLE_CHARS: usize = 200;
pub const MAX_TASK_DESCRIPTION_CHARS: usize = 4000;

pub fn validate_task_fields(title: &str, description: &str) -> Result<(), TaskError> {
    if title.trim().is_empty() {
        return Err(TaskError::EmptyTitle);
    }
    if title.trim().chars().count() > MAX_TASK_TITLE_CHARS {
        return Err(TaskError::TitleTooLong);
    }
    if description.trim().chars().count() > MAX_TASK_DESCRIPTION_CHARS {
        return Err(TaskError::DescriptionTooLong);
    }
    Ok(())
}

/// Whether a task status transition is legal.
pub fn task_transition_allowed(from: TaskStatus, to: TaskStatus) -> bool {
    use TaskStatus::*;
    if to == Cancelled {
        return matches!(
            from,
            Draft | Ready | Running | WaitingApproval | WaitingUser | Blocked
        );
    }
    match from {
        Draft | Blocked => matches!(to, Ready | Running),
        Ready => matches!(to, Running),
        Running => matches!(
            to,
            WaitingApproval | WaitingUser | Blocked | Completed | Failed
        ),
        WaitingApproval => matches!(to, Running | Blocked | Failed | Cancelled),
        WaitingUser => matches!(to, Running | Blocked | Failed | Cancelled),
        Completed | Failed | Cancelled => false,
    }
}

/// Whether a task execution status transition is legal.
pub fn execution_transition_allowed(from: TaskExecutionStatus, to: TaskExecutionStatus) -> bool {
    use TaskExecutionStatus::*;
    if to == Cancelled {
        return matches!(from, Created | Running | WaitingApproval | WaitingUser);
    }
    match from {
        Created => matches!(to, Running),
        Running => matches!(
            to,
            WaitingApproval | WaitingUser | Completed | Failed | Interrupted
        ),
        WaitingApproval => matches!(to, Running | Interrupted),
        WaitingUser => matches!(to, Running | Interrupted),
        Completed | Failed | Cancelled | Interrupted => false,
    }
}
