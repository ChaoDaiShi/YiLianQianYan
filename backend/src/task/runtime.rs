use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::db::{Database, TaskExecutionPersistenceError, TaskWorldPersistenceError};
use crate::shared::command::{CommandRequest, CommandResult, CommandRouter, CommandStatus};
use crate::shared::context::{ContextRequest, TaskProjection, TaskProjectionProvider};
use crate::shared::event::{EventHub, YiEvent};

use super::validation::ValidationPolicy;
use super::voice_commands::{
    status_from_detail, TaskExecutionControl, TaskExecutionControlState, TaskStatusProjection,
};
use super::TaskProjectionProviderAdapter;
use super::{
    CanvasView, CanvasViewError, ExecutorResolver, GraphRevision, NodeExecution, NodeExecutionId,
    NodeExecutionStatus, ResolvedExecutionPlan, TaskCheckpoint, TaskCommandExecution,
    TaskCommandExecutionStatus, TaskEdge, TaskGraph, TaskGraphDetail, TaskGraphId,
    TaskGraphValidationError, TaskHarness, TaskHarnessError, TaskNode, TaskNodeExecutionSummary,
    TaskNodeId, TaskSupervisor, TaskSupervisorError,
};

const TASK_WORLD_EVENT_SOURCE: &str = "v1-task-world";

pub type TaskCommandDispatch = CommandResult;

/// Structured errors returned by the authoritative Task World registry.
#[derive(Debug, Error)]
pub enum TaskWorldRuntimeError {
    #[error("task world persistence failed: {0}")]
    Persistence(#[from] TaskWorldPersistenceError),
    #[error("task execution persistence failed: {0}")]
    ExecutionPersistence(#[from] TaskExecutionPersistenceError),
    #[error("task harness rejected execution: {0}")]
    Harness(#[from] TaskHarnessError),
    #[error("task graph validation failed: {0}")]
    Graph(#[from] TaskGraphValidationError),
    #[error("task supervisor rejected state: {0}")]
    Supervisor(#[from] TaskSupervisorError),
    #[error("task graph not found: {0}")]
    GraphNotFound(String),
    #[error("task graph already exists: {0}")]
    GraphAlreadyExists(String),
    #[error("stale task graph revision: expected {expected}, actual {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("task checkpoint not found: {0}")]
    CheckpointNotFound(String),
    #[error("task canvas view validation failed: {0}")]
    Canvas(#[from] CanvasViewError),
    #[error("stale task canvas view revision: expected {expected}, actual {actual}")]
    StaleCanvasViewRevision { expected: u64, actual: u64 },
    #[error("task canvas view revision overflow")]
    CanvasViewRevisionOverflow,
    #[error(
        "active command execution for task node {node_id} must be cancelled first: {request_id}"
    )]
    CommandExecutionActive { node_id: String, request_id: String },
    #[error("task node requires controlled command execution: {0}")]
    CommandRequiresExecution(String),
    #[error("task command is not active for node {node_id}: {request_id}")]
    CommandExecutionNotActive { node_id: String, request_id: String },
    #[error("task command result rejected: {0}")]
    InvalidCommandResult(String),
    #[error("task execution control is paused: {0}")]
    ExecutionPaused(String),
    #[error("task execution control is cancelled: {0}")]
    ExecutionCancelled(String),
    #[error("task execution control generation overflow")]
    ExecutionControlGenerationOverflow,
}

struct CommittedGraphMutation {
    graph: TaskGraph,
    newly_ready: Vec<TaskNodeId>,
}

struct ExecutionCancellation {
    graph_id: TaskGraphId,
    token: CancellationToken,
    in_flight: bool,
}

/// Owns one actual provider future, independently of its projected attempt
/// status. Dropping the future settles ownership and releases its signal.
pub struct TaskDispatchLease {
    execution_id: NodeExecutionId,
    token: CancellationToken,
    registry: Arc<Mutex<HashMap<NodeExecutionId, ExecutionCancellation>>>,
}

impl TaskDispatchLease {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Drop for TaskDispatchLease {
    fn drop(&mut self) {
        self.token.cancel();
        self.registry.lock().remove(&self.execution_id);
    }
}

/// The authoritative in-memory Task World registry.
///
/// Each graph has exactly one supervisor owned by this runtime.  Persistence
/// is committed before the candidate replaces the registry, and product facts
/// are published only after that replacement.  This layer owns Task state and
/// submits bounded command requests; it never performs native desktop work.
#[derive(Clone)]
pub struct TaskWorldRuntime {
    database: Database,
    supervisors: Arc<RwLock<HashMap<TaskGraphId, TaskSupervisor>>>,
    harnesses: Arc<RwLock<HashMap<TaskGraphId, TaskHarness>>>,
    execution_controls: Arc<RwLock<HashMap<TaskGraphId, TaskExecutionControl>>>,
    canvas_write_lock: Arc<Mutex<()>>,
    /// Shared ordering point for graph control changes and new dispatch
    /// reservations. A pause that acquires this guard first prevents a later
    /// dispatch from passing the running-state check.
    control_dispatch_lock: Arc<Mutex<()>>,
    execution_tokens: Arc<Mutex<HashMap<NodeExecutionId, ExecutionCancellation>>>,
    events: EventHub,
}

impl TaskWorldRuntime {
    /// The task-owned cancellation signal reserved before provider dispatch.
    pub fn execution_cancellation_token(
        &self,
        execution_id: &NodeExecutionId,
    ) -> Option<CancellationToken> {
        self.execution_tokens
            .lock()
            .get(execution_id)
            .map(|entry| entry.token.clone())
    }

    pub fn claim_execution_dispatch(
        &self,
        graph_id: &TaskGraphId,
        execution_id: &NodeExecutionId,
    ) -> Result<TaskDispatchLease, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        let harnesses = self.harnesses.read();
        let execution = harnesses
            .get(graph_id)
            .and_then(|harness| harness.execution(execution_id))
            .ok_or_else(|| TaskHarnessError::UnknownExecution(execution_id.clone()))?;
        if !execution.status.is_active() {
            return Err(TaskHarnessError::InvalidValidationStatus(execution.status).into());
        }
        let mut registry = self.execution_tokens.lock();
        let entry = registry
            .get_mut(execution_id)
            .ok_or_else(|| TaskHarnessError::UnknownExecution(execution_id.clone()))?;
        if entry.in_flight {
            return Err(TaskHarnessError::ActiveExecution(execution_id.clone()).into());
        }
        entry.in_flight = true;
        Ok(TaskDispatchLease {
            execution_id: execution_id.clone(),
            token: entry.token.clone(),
            registry: Arc::clone(&self.execution_tokens),
        })
    }

    fn ensure_no_unsettled_provider(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<(), TaskWorldRuntimeError> {
        if let Some((id, _)) = self
            .execution_tokens
            .lock()
            .iter()
            .find(|(_, entry)| entry.graph_id == *graph_id && entry.in_flight)
        {
            return Err(TaskHarnessError::ActiveExecution(id.clone()).into());
        }
        Ok(())
    }

    fn finish_execution_token(&self, execution_id: &NodeExecutionId, cancel: bool) {
        let mut registry = self.execution_tokens.lock();
        if let Some(entry) = registry.get(execution_id) {
            if cancel {
                entry.token.cancel();
            }
            if !entry.in_flight {
                registry.remove(execution_id);
            }
        }
    }

    /// Load all persisted v1 snapshots and explicitly install migration 1000.
    pub fn new(database: &Database, events: EventHub) -> Result<Self, TaskWorldRuntimeError> {
        database.initialize_task_world_schema()?;
        let persisted_controls = database.load_all_task_execution_controls()?;
        database.recover_node_executions(now())?;
        let persisted_executions = database.load_all_node_executions()?;
        let snapshots = database.load_all_task_supervisor_snapshots()?;
        let mut supervisors = HashMap::with_capacity(snapshots.len());
        for snapshot in snapshots {
            let mut supervisor = snapshot.supervisor;
            // A persisted in-flight request cannot be trusted after process
            // restart because the v2 provider may have lost its request
            // store. Fail it closed instead of leaving a permanent running
            // task that could later accept a stale result.
            let interrupted = supervisor.fail_active_command_executions_on_restart(now());
            if !interrupted.is_empty() {
                database.save_task_supervisor_snapshot(&supervisor, now())?;
            }
            database.ensure_task_revision_history(supervisor.graph(), snapshot.updated_at)?;
            let graph_id = supervisor.graph().id.clone();
            if supervisors.insert(graph_id.clone(), supervisor).is_some() {
                return Err(TaskWorldRuntimeError::GraphAlreadyExists(
                    graph_id.to_string(),
                ));
            }
        }
        let mut executions_by_graph: HashMap<TaskGraphId, Vec<NodeExecution>> = HashMap::new();
        for execution in persisted_executions {
            executions_by_graph
                .entry(execution.graph_id.clone())
                .or_default()
                .push(execution);
        }
        let mut harnesses = HashMap::with_capacity(supervisors.len());
        for (graph_id, supervisor) in &supervisors {
            let attempts = executions_by_graph.remove(graph_id).unwrap_or_default();
            let harness = TaskHarness::from_attempts(
                supervisor.graph().clone(),
                ExecutorResolver::new(),
                attempts,
            )?;
            harnesses.insert(graph_id.clone(), harness);
        }
        if let Some((graph_id, _)) = executions_by_graph.into_iter().next() {
            return Err(TaskWorldRuntimeError::Harness(TaskHarnessError::Graph(
                format!("execution history references unknown graph {graph_id}"),
            )));
        }
        let execution_controls = persisted_controls
            .into_iter()
            .map(|control| (control.graph_id.clone(), control))
            .collect();
        Ok(Self {
            database: database.clone(),
            supervisors: Arc::new(RwLock::new(supervisors)),
            harnesses: Arc::new(RwLock::new(harnesses)),
            execution_controls: Arc::new(RwLock::new(execution_controls)),
            canvas_write_lock: Arc::new(Mutex::new(())),
            control_dispatch_lock: Arc::new(Mutex::new(())),
            execution_tokens: Arc::new(Mutex::new(HashMap::new())),
            events,
        })
    }

    /// Return graph definitions in stable identity order for v1-owned APIs.
    pub fn list_graphs(&self) -> Vec<TaskGraph> {
        let mut graphs = self
            .supervisors
            .read()
            .values()
            .map(|supervisor| supervisor.graph().clone())
            .collect::<Vec<_>>();
        graphs.sort_unstable_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        graphs
    }

    pub fn get_graph(&self, graph_id: &TaskGraphId) -> Option<TaskGraph> {
        self.supervisors
            .read()
            .get(graph_id)
            .map(|supervisor| supervisor.graph().clone())
    }

    /// Return the persisted graph-level execution control.  Older snapshots
    /// without a control row are adopted as running exactly once.
    pub fn execution_control(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        self.ensure_graph(graph_id)?;
        if let Some(control) = self.database.load_task_execution_control(graph_id)? {
            self.execution_controls
                .write()
                .insert(graph_id.clone(), control.clone());
            return Ok(control);
        }

        let control = self
            .execution_controls
            .read()
            .get(graph_id)
            .cloned()
            .unwrap_or_else(|| TaskExecutionControl::initial(graph_id.clone(), now()));
        self.database.save_task_execution_control(&control)?;
        self.execution_controls
            .write()
            .insert(graph_id.clone(), control.clone());
        Ok(control)
    }

    /// Alias used by command/query adapters that treat the control as a state
    /// projection rather than an execution object.
    pub fn control_state(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        self.execution_control(graph_id)
    }

    fn ensure_execution_allowed(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<(), TaskWorldRuntimeError> {
        match self.execution_control(graph_id)?.state {
            TaskExecutionControlState::Running => Ok(()),
            TaskExecutionControlState::Paused => {
                Err(TaskWorldRuntimeError::ExecutionPaused(graph_id.to_string()))
            }
            TaskExecutionControlState::Cancelled => Err(TaskWorldRuntimeError::ExecutionCancelled(
                graph_id.to_string(),
            )),
        }
    }

    fn save_execution_control(
        &self,
        control: &TaskExecutionControl,
    ) -> Result<(), TaskWorldRuntimeError> {
        self.database.save_task_execution_control(control)?;
        self.execution_controls
            .write()
            .insert(control.graph_id.clone(), control.clone());
        Ok(())
    }

    /// Pause without erasing history. Local and registered Workflow attempts
    /// receive a cooperative stop; no external action rollback is claimed.
    pub fn pause_task(
        &self,
        graph_id: &TaskGraphId,
        updated_at: i64,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.pause_task_locked(graph_id, updated_at)
    }

    fn pause_task_locked(
        &self,
        graph_id: &TaskGraphId,
        updated_at: i64,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        self.ensure_graph(graph_id)?;
        let mut control = self.execution_control(graph_id)?;
        if control.state == TaskExecutionControlState::Cancelled {
            return Err(TaskWorldRuntimeError::ExecutionCancelled(
                graph_id.to_string(),
            ));
        }
        if control.state == TaskExecutionControlState::Paused {
            return Ok(control);
        }

        let mut paused_nodes = Vec::new();
        {
            let mut harnesses = self.harnesses.write();
            if let Some(harness) = harnesses.get_mut(graph_id) {
                let active_attempts = harness
                    .all_attempts()
                    .into_iter()
                    .filter(|execution| {
                        execution.status.is_active()
                            && (execution.executor_ref.is_none()
                                || (execution
                                    .executor_ref
                                    .as_ref()
                                    .is_some_and(|reference| reference.scheme() == "workflow")
                                    && self.execution_tokens.lock().contains_key(&execution.id)))
                    })
                    .collect::<Vec<_>>();
                for execution in active_attempts {
                    harness.cancel_execution_for_pause(&execution.id, updated_at)?;
                    self.finish_execution_token(&execution.id, true);
                    let updated = harness.execution(&execution.id).cloned().ok_or_else(|| {
                        TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(
                            execution.id.clone(),
                        ))
                    })?;
                    self.database.update_node_execution(&updated)?;
                    paused_nodes.push(updated.node_id);
                }
            }
        }
        paused_nodes.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        paused_nodes.dedup();
        control.state = TaskExecutionControlState::Paused;
        control.generation = control.next_generation()?;
        control.paused_nodes = paused_nodes;
        control.updated_at = updated_at;
        self.save_execution_control(&control)?;
        self.publish(
            "task.paused",
            json!({
                "graph_id": graph_id.as_str(),
                "generation": control.generation,
                "paused_nodes": control.paused_nodes.iter().map(TaskNodeId::as_str).collect::<Vec<_>>(),
            }),
        );
        Ok(control)
    }

    /// Resume from the persisted graph control state.  Existing succeeded
    /// attempts remain append-only history; pending work and pause-marked
    /// local attempts can be scheduled as fresh attempts.
    pub fn resume_task(
        &self,
        graph_id: &TaskGraphId,
        updated_at: i64,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.resume_task_locked(graph_id, updated_at)
    }

    fn resume_task_locked(
        &self,
        graph_id: &TaskGraphId,
        updated_at: i64,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        self.ensure_graph(graph_id)?;
        let mut control = self.execution_control(graph_id)?;
        match control.state {
            TaskExecutionControlState::Cancelled => {
                return Err(TaskWorldRuntimeError::ExecutionCancelled(
                    graph_id.to_string(),
                ))
            }
            TaskExecutionControlState::Running => {
                self.dispatch_ready_nodes_locked(graph_id, updated_at)?;
                return Ok(control);
            }
            TaskExecutionControlState::Paused => {}
        }
        control.state = TaskExecutionControlState::Running;
        control.generation = control.next_generation()?;
        control.paused_nodes.clear();
        control.updated_at = updated_at;
        self.save_execution_control(&control)?;
        self.publish(
            "task.resumed",
            json!({
                "graph_id": graph_id.as_str(),
                "generation": control.generation,
            }),
        );
        // Resume is a scheduling command, not only a control-state toggle.
        // Reserve every currently ready local node while the same ordering
        // guard is held; provider-backed nodes remain for their owning
        // integration adapter because v1 has no provider resolver here.
        self.dispatch_ready_nodes_locked(graph_id, updated_at)?;
        Ok(control)
    }

    /// Cancel the graph-level control.  Safe local work is treated like a
    /// pause before the terminal control is persisted; provider-backed work is
    /// left for its owning adapter to reconcile.
    pub fn cancel_task(
        &self,
        graph_id: &TaskGraphId,
        updated_at: i64,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.cancel_task_locked(graph_id, updated_at)
    }

    fn cancel_task_locked(
        &self,
        graph_id: &TaskGraphId,
        updated_at: i64,
    ) -> Result<TaskExecutionControl, TaskWorldRuntimeError> {
        self.ensure_graph(graph_id)?;
        let mut control = self.execution_control(graph_id)?;
        if control.state == TaskExecutionControlState::Cancelled {
            return Ok(control);
        }
        if control.state == TaskExecutionControlState::Running {
            control = self.pause_task_locked(graph_id, updated_at)?;
        }
        control.state = TaskExecutionControlState::Cancelled;
        control.generation = control.next_generation()?;
        control.updated_at = updated_at;
        self.save_execution_control(&control)?;
        self.publish(
            "task.cancelled",
            json!({
                "graph_id": graph_id.as_str(),
                "generation": control.generation,
            }),
        );
        Ok(control)
    }

    /// Reserve the bounded set of scheduler-ready local nodes. The method is
    /// intentionally private to the control transition paths plus the public
    /// explicit dispatch hook below, so all callers share the pause ordering
    /// boundary.
    fn dispatch_ready_nodes_locked(
        &self,
        graph_id: &TaskGraphId,
        now: i64,
    ) -> Result<Vec<NodeExecution>, TaskWorldRuntimeError> {
        self.ensure_execution_allowed(graph_id)?;
        if self.ensure_no_unsettled_provider(graph_id).is_err() {
            return Ok(Vec::new());
        }
        let graph = self.ensure_graph(graph_id)?;
        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .get_mut(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let ready = harness.schedule().ready;
        let mut started = Vec::new();
        for node_id in ready {
            let Some(node) = graph.node(&node_id) else {
                continue;
            };
            // v1 can reserve plain local work. Provider-backed nodes need the
            // integration resolver and must not be represented as dispatched
            // by a deterministic fallback.
            if node.input.get("executor_ref").is_some() {
                continue;
            }
            let execution_id = harness.start_node(&node_id, now)?;
            let execution = harness.execution(&execution_id).cloned().ok_or_else(|| {
                TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(
                    execution_id.clone(),
                ))
            })?;
            self.database.save_node_execution(&execution)?;
            started.push(execution);
        }
        drop(harnesses);

        for execution in &started {
            self.publish(
                "task.execution.created",
                json!({
                    "graph_id": graph_id.as_str(),
                    "node_id": execution.node_id.as_str(),
                    "execution_id": execution.id.as_str(),
                    "attempt": execution.attempt,
                    "status": execution.status,
                    "reason": "control transition re-drove scheduler-ready local work",
                }),
            );
        }
        Ok(started)
    }

    /// Explicitly reserve ready local work through the same control/dispatch
    /// ordering point used by pause, resume, retry and rerun.
    pub fn dispatch_ready_nodes(
        &self,
        graph_id: &TaskGraphId,
        now: i64,
    ) -> Result<Vec<NodeExecution>, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.dispatch_ready_nodes_locked(graph_id, now)
    }

    /// Build a bounded natural-language-ready projection from the graph detail
    /// and persisted execution control, never from raw event logs.
    pub fn status_projection(
        &self,
        graph_id: &TaskGraphId,
        updated_at: i64,
    ) -> Result<TaskStatusProjection, TaskWorldRuntimeError> {
        let detail = self.get_graph_detail(graph_id)?;
        let control = self.execution_control(graph_id)?;
        Ok(status_from_detail(&detail, &control, updated_at))
    }

    /// Return a safe detail projection assembled from the authoritative
    /// supervisor and persisted semantic summaries. Raw runtime structures,
    /// database rows, and synchronization primitives never cross this API.
    pub fn get_graph_detail(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<TaskGraphDetail, TaskWorldRuntimeError> {
        let supervisor = self
            .supervisors
            .read()
            .get(graph_id)
            .cloned()
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let revisions = self.database.load_task_revision_history(graph_id)?;
        let checkpoints = self.database.load_task_checkpoint_summaries(graph_id)?;
        let executions = self
            .harnesses
            .read()
            .get(graph_id)
            .map(TaskHarness::execution_histories)
            .unwrap_or_default();
        Ok(TaskGraphDetail::from_supervisor_with_executions(
            &supervisor,
            revisions,
            checkpoints,
            &executions,
        ))
    }

    /// Load and reconcile presentation state for the current graph. A missing
    /// row receives deterministic initial positions but is not a semantic
    /// graph write.
    pub fn get_canvas_view(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<CanvasView, TaskWorldRuntimeError> {
        let graph = self
            .get_graph(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let mut view = self
            .database
            .load_task_canvas_view(&graph)?
            .unwrap_or(CanvasView::initial(&graph, now())?);
        view.reconcile(&graph, now())?;
        Ok(view)
    }

    /// Persist presentation-only state using an independent optimistic view
    /// revision. The graph revision is read for validation/reconciliation but
    /// is never incremented by this method.
    pub fn save_canvas_view(
        &self,
        graph_id: &TaskGraphId,
        mut view: CanvasView,
        expected_view_revision: u64,
        updated_at: i64,
    ) -> Result<CanvasView, TaskWorldRuntimeError> {
        // Serialize the read/validate/increment/write sequence across runtime
        // clones so two editors cannot both publish the same next revision.
        let _canvas_write_guard = self.canvas_write_lock.lock();
        let graph = self
            .get_graph(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let current = self
            .database
            .load_task_canvas_view(&graph)?
            .map(|mut view| {
                view.reconcile(&graph, updated_at)?;
                Ok::<_, CanvasViewError>(view)
            })
            .transpose()?
            .unwrap_or(CanvasView::initial(&graph, updated_at)?);
        if expected_view_revision == 0 || expected_view_revision != current.view_revision {
            return Err(TaskWorldRuntimeError::StaleCanvasViewRevision {
                expected: expected_view_revision,
                actual: current.view_revision,
            });
        }
        if view.view_revision != expected_view_revision {
            return Err(TaskWorldRuntimeError::StaleCanvasViewRevision {
                expected: expected_view_revision,
                actual: view.view_revision,
            });
        }
        if view.graph_id != graph.id {
            return Err(TaskWorldRuntimeError::Canvas(
                CanvasViewError::GraphIdentityMismatch,
            ));
        }
        view.reconcile(&graph, updated_at)?;
        view.view_revision = current
            .view_revision
            .checked_add(1)
            .ok_or(TaskWorldRuntimeError::CanvasViewRevisionOverflow)?;
        view.updated_at = updated_at;
        self.database.save_task_canvas_view(&view, &graph)?;
        self.publish(
            "task.canvas.updated",
            json!({
                "graph_id": graph.id.as_str(),
                "view_revision": view.view_revision,
                "graph_revision": graph.revision.value(),
            }),
        );
        Ok(view)
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<YiEvent> {
        self.events.subscribe()
    }

    /// Return every persisted attempt for one graph/node in attempt order.
    pub fn list_node_executions(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
    ) -> Result<Vec<NodeExecution>, TaskWorldRuntimeError> {
        self.ensure_graph(graph_id)?;
        Ok(self.database.load_node_executions(graph_id, node_id)?)
    }

    pub fn list_node_execution_summaries(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
    ) -> Result<Vec<TaskNodeExecutionSummary>, TaskWorldRuntimeError> {
        Ok(self
            .list_node_executions(graph_id, node_id)?
            .iter()
            .map(super::projection::execution_summary)
            .collect())
    }

    /// Reserve and persist one independent execution attempt. The resolver
    /// remains explicit in the in-process harness; an unregistered provider is
    /// rejected before any adapter can be invoked.
    pub fn start_execution(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        self.start_execution_with_resolver(
            graph_id,
            node_id,
            expected_revision,
            ExecutorResolver::new(),
            now,
        )
    }

    /// Reserve an attempt using an availability snapshot built by the
    /// integration boundary. Provider discovery stays outside TaskGraph while
    /// the resolved reference is persisted on the attempt before dispatch.
    pub fn start_execution_with_resolver(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        resolver: ExecutorResolver,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.ensure_execution_allowed(graph_id)?;
        self.ensure_no_unsettled_provider(graph_id)?;
        let expected_revision = GraphRevision::new(expected_revision)?;
        let graph = self.ensure_graph(graph_id)?;
        let supervisors = self.supervisors.read();
        let supervisor = supervisors
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        ensure_revision(&supervisor, expected_revision)?;
        drop(supervisors);
        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .entry(graph_id.clone())
            .or_insert(TaskHarness::new(graph, ExecutorResolver::new())?);
        harness.set_resolver(resolver);
        let execution_id = if harness
            .latest_execution(node_id)
            .is_some_and(|execution| execution.status == NodeExecutionStatus::Failed)
        {
            harness.retry_node(node_id, now)?
        } else {
            harness.start_node(node_id, now)?
        };
        let execution = harness.execution(&execution_id).cloned().ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
        self.database.save_node_execution(&execution)?;
        self.execution_tokens.lock().insert(
            execution_id.clone(),
            ExecutionCancellation {
                graph_id: graph_id.clone(),
                token: CancellationToken::new(),
                in_flight: false,
            },
        );
        drop(harnesses);
        self.publish(
            "task.execution.created",
            json!({
                "graph_id": graph_id.as_str(),
                "node_id": node_id.as_str(),
                "execution_id": execution.id.as_str(),
                "attempt": execution.attempt,
                "status": execution.status,
            }),
        );
        Ok(execution)
    }

    pub fn execution_plan(
        &self,
        graph_id: &TaskGraphId,
        execution_id: &NodeExecutionId,
    ) -> Result<ResolvedExecutionPlan, TaskWorldRuntimeError> {
        self.ensure_graph(graph_id)?;
        self.harnesses
            .read()
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?
            .resolve_execution_plan(execution_id)
            .map_err(Into::into)
    }

    pub fn mark_execution_waiting_approval(
        &self,
        graph_id: &TaskGraphId,
        execution_id: &NodeExecutionId,
        approval_ref: &str,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .get_mut(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        harness.mark_waiting_approval(execution_id, approval_ref, now)?;
        let execution = harness.execution(execution_id).cloned().ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
        self.database.update_node_execution(&execution)?;
        drop(harnesses);
        self.publish(
            "task.execution.waiting_approval",
            json!({
                "graph_id": graph_id.as_str(),
                "node_id": execution.node_id.as_str(),
                "execution_id": execution.id.as_str(),
                "attempt": execution.attempt,
                "status": execution.status,
                "approval_ref": approval_ref,
            }),
        );
        Ok(execution)
    }

    pub fn complete_execution(
        &self,
        graph_id: &TaskGraphId,
        execution_id: &NodeExecutionId,
        output: Value,
        policy: ValidationPolicy,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .get_mut(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        if harness
            .execution(execution_id)
            .is_some_and(|execution| execution.status == NodeExecutionStatus::WaitingApproval)
        {
            harness.mark_running(execution_id, now)?;
        }
        harness.record_output(execution_id, output)?;
        harness.validate_execution(execution_id, policy, now)?;
        let execution = harness.execution(execution_id).cloned().ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
        self.database.update_node_execution(&execution)?;
        self.finish_execution_token(execution_id, false);
        drop(harnesses);
        self.publish(
            if execution.status == NodeExecutionStatus::Succeeded {
                "task.execution.succeeded"
            } else {
                "task.execution.failed"
            },
            json!({
                "graph_id": graph_id.as_str(),
                "node_id": execution.node_id.as_str(),
                "execution_id": execution.id.as_str(),
                "attempt": execution.attempt,
                "status": execution.status,
            }),
        );
        Ok(execution)
    }

    pub fn fail_execution(
        &self,
        graph_id: &TaskGraphId,
        execution_id: &NodeExecutionId,
        code: &str,
        error: &str,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .get_mut(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        harness.fail_execution(execution_id, code, error, now)?;
        let execution = harness.execution(execution_id).cloned().ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
        self.database.update_node_execution(&execution)?;
        self.finish_execution_token(execution_id, true);
        drop(harnesses);
        self.publish(
            "task.execution.failed",
            json!({
                "graph_id": graph_id.as_str(),
                "node_id": execution.node_id.as_str(),
                "execution_id": execution.id.as_str(),
                "attempt": execution.attempt,
                "status": execution.status,
                "failure_code": execution.failure_code(),
            }),
        );
        Ok(execution)
    }

    pub fn find_execution(
        &self,
        execution_id: &NodeExecutionId,
    ) -> Option<(TaskGraphId, NodeExecution)> {
        self.harnesses
            .read()
            .iter()
            .find_map(|(graph_id, harness)| {
                harness
                    .execution(execution_id)
                    .cloned()
                    .map(|execution| (graph_id.clone(), execution))
            })
    }

    pub fn update_execution(
        &self,
        graph_id: &TaskGraphId,
        execution_id: &NodeExecutionId,
        status: NodeExecutionStatus,
        output: Option<Value>,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        self.ensure_graph(graph_id)?;
        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .get_mut(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        match status {
            NodeExecutionStatus::Running => harness.mark_running(execution_id, now)?,
            NodeExecutionStatus::WaitingApproval => {
                harness.mark_waiting_approval(execution_id, "approval-required", now)?
            }
            NodeExecutionStatus::Validating => {
                return Err(TaskWorldRuntimeError::Harness(
                    TaskHarnessError::InvalidValidationStatus(status),
                ))
            }
            NodeExecutionStatus::Cancelled => {
                harness.cancel_execution(execution_id, now)?;
                self.finish_execution_token(execution_id, true);
            }
            _ => {}
        }
        if let Some(output) = output {
            harness.record_output(execution_id, output)?;
        }
        let execution = harness.execution(execution_id).cloned().ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
        self.database.update_node_execution(&execution)?;
        drop(harnesses);
        self.publish(
            "task.execution.updated",
            json!({
                "graph_id": graph_id.as_str(),
                "node_id": execution.node_id.as_str(),
                "execution_id": execution.id.as_str(),
                "status": execution.status,
            }),
        );
        Ok(execution)
    }

    pub fn cancel_execution(
        &self,
        graph_id: &TaskGraphId,
        execution_id: &NodeExecutionId,
        expected_revision: u64,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        let expected_revision = GraphRevision::new(expected_revision)?;
        let supervisor = self
            .supervisors
            .read()
            .get(graph_id)
            .cloned()
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        ensure_revision(&supervisor, expected_revision)?;
        self.update_execution(
            graph_id,
            execution_id,
            NodeExecutionStatus::Cancelled,
            None,
            now,
        )
    }

    pub fn rerun_from_node(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        now: i64,
    ) -> Result<Vec<TaskNodeId>, TaskWorldRuntimeError> {
        self.rerun_from_node_with_dispatch(graph_id, node_id, expected_revision, now, true)
    }

    /// User recovery validates and prepares the branch. A later explicit start
    /// selects a real provider; unrelated editable nodes are never dispatched.
    pub fn prepare_rerun_from_node(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        now: i64,
    ) -> Result<Vec<TaskNodeId>, TaskWorldRuntimeError> {
        self.rerun_from_node_with_dispatch(graph_id, node_id, expected_revision, now, false)
    }

    fn rerun_from_node_with_dispatch(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        now: i64,
        dispatch: bool,
    ) -> Result<Vec<TaskNodeId>, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.ensure_execution_allowed(graph_id)?;
        self.ensure_no_unsettled_provider(graph_id)?;
        let expected_revision = GraphRevision::new(expected_revision)?;
        let supervisor = self
            .supervisors
            .read()
            .get(graph_id)
            .cloned()
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        ensure_revision(&supervisor, expected_revision)?;
        supervisor.graph().validate()?;
        let mut harnesses = self.harnesses.write();
        let mut candidate = harnesses
            .get(graph_id)
            .cloned()
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let affected = candidate.rerun_from_node(node_id, now)?;
        let changed_attempts = candidate
            .all_attempts()
            .into_iter()
            .filter(|execution| affected.contains(&execution.node_id))
            .collect::<Vec<_>>();
        self.database
            .update_node_executions_atomically(&changed_attempts)?;
        harnesses.insert(graph_id.clone(), candidate);
        drop(harnesses);
        self.publish(
            "task.rerun.started",
            json!({
                "graph_id": graph_id.as_str(),
                "node_id": node_id.as_str(),
            }),
        );
        if dispatch {
            self.dispatch_ready_nodes_locked(graph_id, now)?;
        }
        for affected_node in &affected {
            self.publish(
                "task.node.stale",
                json!({
                    "graph_id": graph_id.as_str(),
                    "node_id": affected_node.as_str(),
                    "reason": "partial rerun",
                }),
            );
        }
        self.publish(
            "task.rerun.completed",
            json!({
                "graph_id": graph_id.as_str(),
                "node_id": node_id.as_str(),
                "affected_nodes": affected.iter().map(TaskNodeId::as_str).collect::<Vec<_>>(),
            }),
        );
        Ok(affected)
    }

    /// Reserve a fresh append-only attempt for a failed or pause-marked node.
    /// The harness decides whether retry is allowed; this runtime only owns
    /// persistence and the graph-level execution-control guard.
    pub fn retry_node(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        now: i64,
    ) -> Result<NodeExecution, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.ensure_execution_allowed(graph_id)?;
        self.ensure_no_unsettled_provider(graph_id)?;
        let graph = self.ensure_graph(graph_id)?;
        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .get_mut(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let execution_id = harness.retry_node(node_id, now)?;
        let execution = harness.execution(&execution_id).cloned().ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::UnknownExecution(execution_id.clone()))
        })?;
        self.database.save_node_execution(&execution)?;
        drop(harnesses);
        self.publish(
            "task.retry.started",
            json!({
                "graph_id": graph.id.as_str(),
                "node_id": node_id.as_str(),
                "execution_id": execution.id.as_str(),
                "attempt": execution.attempt,
            }),
        );
        self.dispatch_ready_nodes_locked(graph_id, now)?;
        Ok(execution)
    }

    fn ensure_graph(&self, graph_id: &TaskGraphId) -> Result<TaskGraph, TaskWorldRuntimeError> {
        self.get_graph(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))
    }

    pub fn create_graph(
        &self,
        graph_id: TaskGraphId,
        nodes: Vec<TaskNode>,
        edges: Vec<TaskEdge>,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let graph = TaskGraph::new(graph_id.clone(), GraphRevision::initial(), nodes, edges)?;
        let harness = TaskHarness::new(graph.clone(), ExecutorResolver::new())?;
        let supervisor = TaskSupervisor::new(graph, now)?;

        let mut supervisors = self.supervisors.write();
        if supervisors.contains_key(&graph_id) {
            return Err(TaskWorldRuntimeError::GraphAlreadyExists(
                graph_id.to_string(),
            ));
        }
        self.database
            .save_task_supervisor_snapshot_with_revision(&supervisor, now, "created")?;
        let graph = supervisor.graph().clone();
        let ready = supervisor.runnable_nodes();
        supervisors.insert(graph_id.clone(), supervisor);
        drop(supervisors);
        self.harnesses.write().insert(graph_id, harness);
        let control = TaskExecutionControl::initial(graph.id.clone(), now);
        self.execution_controls
            .write()
            .insert(graph.id.clone(), control);

        self.publish(
            "task.created",
            json!({
                "graph_id": graph.id.as_str(),
                "graph_revision": graph.revision.value(),
                "node_count": graph.nodes.len(),
            }),
        );
        self.publish_ready(&graph, ready);
        Ok(graph)
    }

    pub fn add_node(
        &self,
        graph_id: &TaskGraphId,
        node: TaskNode,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let mutation = self.mutate_graph(
            graph_id,
            expected_revision,
            now,
            "node_added",
            |supervisor| supervisor.add_node(node, now).map(|_| ()),
        )?;
        self.publish_graph_updated(&mutation.graph, "node_added");
        self.publish_ready(&mutation.graph, mutation.newly_ready);
        Ok(mutation.graph)
    }

    pub fn update_node(
        &self,
        graph_id: &TaskGraphId,
        node: TaskNode,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let mutation = self.mutate_graph(
            graph_id,
            expected_revision,
            now,
            "node_updated",
            |supervisor| supervisor.update_node(node, now).map(|_| ()),
        )?;
        self.publish_graph_updated(&mutation.graph, "node_updated");
        self.publish_ready(&mutation.graph, mutation.newly_ready);
        Ok(mutation.graph)
    }

    pub fn delete_node(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let node_id = node_id.clone();
        let mutation = self.mutate_graph(
            graph_id,
            expected_revision,
            now,
            "node_deleted",
            |supervisor| supervisor.remove_node(&node_id, now).map(|_| ()),
        )?;
        self.publish_graph_updated(&mutation.graph, "node_deleted");
        self.publish_ready(&mutation.graph, mutation.newly_ready);
        Ok(mutation.graph)
    }

    pub fn remove_node(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        self.delete_node(graph_id, node_id, expected_revision, now)
    }

    pub fn add_edge(
        &self,
        graph_id: &TaskGraphId,
        edge: TaskEdge,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let mutation = self.mutate_graph(
            graph_id,
            expected_revision,
            now,
            "edge_added",
            |supervisor| supervisor.add_edge(edge, now).map(|_| ()),
        )?;
        self.publish_graph_updated(&mutation.graph, "edge_added");
        self.publish_ready(&mutation.graph, mutation.newly_ready);
        Ok(mutation.graph)
    }

    pub fn delete_edge(
        &self,
        graph_id: &TaskGraphId,
        edge: &TaskEdge,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let edge = edge.clone();
        let mutation = self.mutate_graph(
            graph_id,
            expected_revision,
            now,
            "edge_deleted",
            |supervisor| supervisor.remove_edge(&edge, now).map(|_| ()),
        )?;
        self.publish_graph_updated(&mutation.graph, "edge_deleted");
        self.publish_ready(&mutation.graph, mutation.newly_ready);
        Ok(mutation.graph)
    }

    pub fn remove_edge(
        &self,
        graph_id: &TaskGraphId,
        edge: &TaskEdge,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        self.delete_edge(graph_id, edge, expected_revision, now)
    }

    /// Transition one ready node to `running`.  This records only product
    /// state; no executor is selected or invoked here.
    pub fn start_node(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.ensure_execution_allowed(graph_id)?;
        let node_id = node_id.clone();
        {
            let supervisors = self.supervisors.read();
            let supervisor = supervisors
                .get(graph_id)
                .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
            ensure_revision(supervisor, GraphRevision::new(expected_revision)?)?;
            let node = supervisor.graph().node(&node_id).ok_or_else(|| {
                TaskWorldRuntimeError::Supervisor(TaskSupervisorError::UnknownNode(
                    node_id.to_string(),
                ))
            })?;
            if node.command_binding()?.is_some_and(|binding| {
                binding.command == crate::task::executor_ref::DESKTOP_APP_FOCUS_COMMAND
            }) {
                return Err(TaskWorldRuntimeError::CommandRequiresExecution(
                    node_id.to_string(),
                ));
            }
        }
        let mutation = self.mutate_graph_locked(
            graph_id,
            expected_revision,
            now,
            "node_running",
            |supervisor| supervisor.start_node(&node_id, now),
        )?;
        self.publish(
            "task.node.running",
            json!({
                "graph_id": mutation.graph.id.as_str(),
                "graph_revision": mutation.graph.revision.value(),
                "node_id": node_id.as_str(),
            }),
        );
        self.publish_ready(&mutation.graph, mutation.newly_ready);
        Ok(mutation.graph)
    }

    /// Reserve one ordinary TaskNode attempt and submit the bounded focus
    /// request to the shared CommandRouter.  The v1 runtime never performs a
    /// native action; a provider handler registered by v2 owns policy,
    /// approval, dispatch, and verification.
    pub fn execute_node(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        command_router: &CommandRouter,
        now: i64,
    ) -> Result<TaskCommandDispatch, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.ensure_execution_allowed(graph_id)?;
        let expected_revision = GraphRevision::new(expected_revision)?;
        let (request_id, app_id, graph_revision) = {
            let mut supervisors = self.supervisors.write();
            let current = supervisors
                .get(graph_id)
                .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
            ensure_revision(current, expected_revision)?;
            if let Some((active_node_id, execution)) = active_command(current) {
                return Err(TaskWorldRuntimeError::CommandExecutionActive {
                    node_id: active_node_id.to_string(),
                    request_id: execution.request_id.clone(),
                });
            }
            let node = current.graph().node(node_id).ok_or_else(|| {
                TaskWorldRuntimeError::Supervisor(TaskSupervisorError::UnknownNode(
                    node_id.to_string(),
                ))
            })?;
            let binding = node.command_binding()?.ok_or_else(|| {
                TaskWorldRuntimeError::CommandRequiresExecution(node_id.to_string())
            })?;
            if binding.command != crate::task::executor_ref::DESKTOP_APP_FOCUS_COMMAND {
                return Err(TaskWorldRuntimeError::CommandRequiresExecution(
                    node_id.to_string(),
                ));
            }

            let mut candidate = current.clone();
            candidate.start_node(node_id, now)?;
            let request_id = uuid::Uuid::new_v4().to_string();
            let graph_revision = candidate.graph_revision();
            let attempt = candidate
                .node_state(node_id)
                .map(|state| state.attempts)
                .ok_or_else(|| {
                    TaskWorldRuntimeError::Supervisor(TaskSupervisorError::UnknownNode(
                        node_id.to_string(),
                    ))
                })?;
            candidate.attach_command_execution(
                node_id,
                TaskCommandExecution {
                    request_id: request_id.clone(),
                    command: binding.command,
                    app_id: binding.args.app_id.clone(),
                    attempt,
                    graph_revision,
                    status: TaskCommandExecutionStatus::Dispatching,
                    approval_id: None,
                },
            )?;
            self.database
                .save_task_supervisor_snapshot(&candidate, now)?;
            let graph = candidate.graph().clone();
            supervisors.insert(graph.id.clone(), candidate);
            (request_id, binding.args.app_id, graph_revision)
        };

        self.publish(
            "task.node.command.requested",
            json!({
                "graph_id": graph_id.as_str(),
                "graph_revision": graph_revision.value(),
                "node_id": node_id.as_str(),
                "request_id": request_id,
                "command": crate::task::executor_ref::DESKTOP_APP_FOCUS_COMMAND,
                "app_id": app_id,
            }),
        );

        let result = command_router.execute(CommandRequest::new(
            crate::task::executor_ref::DESKTOP_APP_FOCUS_COMMAND,
            request_id.clone(),
            "task-world",
            json!({"app_id": app_id}),
        ));
        self.reconcile_command_result(graph_id, node_id, &request_id, &result, now)?;
        Ok(result)
    }

    /// Query a provider-owned request using a new outer request id. The
    /// original focus request remains the only node correlation key.
    pub fn query_command_status(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        original_request_id: &str,
        command_router: &CommandRouter,
        now: i64,
    ) -> Result<TaskCommandDispatch, TaskWorldRuntimeError> {
        let expected_revision = GraphRevision::new(expected_revision)?;
        self.ensure_command_request(
            graph_id,
            node_id,
            expected_revision,
            original_request_id,
            false,
        )?;
        let outer_request_id = uuid::Uuid::new_v4().to_string();
        let result = command_router.execute(CommandRequest::new(
            "desktop.app.focus.status",
            outer_request_id,
            "task-world",
            json!({"request_id": original_request_id}),
        ));
        self.reconcile_command_result(graph_id, node_id, original_request_id, &result, now)?;
        Ok(result)
    }

    /// Ask the provider to cancel the original request. Local TaskNode state
    /// is cancelled only after an authoritative `cancelled` result (or is
    /// failed when the provider cannot be found), so a UI click cannot claim
    /// cancellation while a native action may still be pending.
    pub fn cancel_command(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: u64,
        original_request_id: &str,
        command_router: &CommandRouter,
        now: i64,
    ) -> Result<TaskCommandDispatch, TaskWorldRuntimeError> {
        let expected_revision = GraphRevision::new(expected_revision)?;
        self.ensure_command_request(
            graph_id,
            node_id,
            expected_revision,
            original_request_id,
            true,
        )?;
        let outer_request_id = uuid::Uuid::new_v4().to_string();
        let result = command_router.execute(CommandRequest::new(
            "desktop.app.focus.cancel",
            outer_request_id,
            "task-world",
            json!({"request_id": original_request_id}),
        ));
        self.reconcile_command_result(graph_id, node_id, original_request_id, &result, now)?;
        Ok(result)
    }

    /// Refresh every active command correlation before a detail projection is
    /// served. This repairs missed SSE events and lets a restarted UI observe
    /// the provider's authoritative status without treating an event payload
    /// as completion authority.
    pub fn reconcile_active_command_requests(
        &self,
        graph_id: &TaskGraphId,
        command_router: &CommandRouter,
        now: i64,
    ) -> Result<(), TaskWorldRuntimeError> {
        let requests = {
            let supervisors = self.supervisors.read();
            let supervisor = supervisors
                .get(graph_id)
                .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
            supervisor
                .node_states()
                .iter()
                .filter_map(|state| {
                    let execution = state.command_execution.as_ref()?;
                    if !execution.status.is_active() {
                        return None;
                    }
                    Some((
                        graph_id.clone(),
                        state.node_id.clone(),
                        supervisor.graph_revision().value(),
                        execution.request_id.clone(),
                    ))
                })
                .collect::<Vec<_>>()
        };
        for (graph_id, node_id, revision, request_id) in requests {
            self.query_command_status(
                &graph_id,
                &node_id,
                revision,
                &request_id,
                command_router,
                now,
            )?;
        }
        Ok(())
    }

    fn ensure_command_request(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        expected_revision: GraphRevision,
        request_id: &str,
        active_only: bool,
    ) -> Result<String, TaskWorldRuntimeError> {
        let supervisors = self.supervisors.read();
        let supervisor = supervisors
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        ensure_revision(supervisor, expected_revision)?;
        let state = supervisor.node_state(node_id).ok_or_else(|| {
            TaskWorldRuntimeError::Supervisor(TaskSupervisorError::UnknownNode(node_id.to_string()))
        })?;
        let Some(execution) = state.command_execution.as_ref() else {
            return Err(TaskWorldRuntimeError::CommandExecutionNotActive {
                node_id: node_id.to_string(),
                request_id: request_id.to_string(),
            });
        };
        if execution.request_id != request_id {
            return Err(TaskWorldRuntimeError::InvalidCommandResult(
                "request id does not match the node's command correlation".to_string(),
            ));
        }
        if active_only && !execution.status.is_active() {
            return Err(TaskWorldRuntimeError::CommandExecutionNotActive {
                node_id: node_id.to_string(),
                request_id: request_id.to_string(),
            });
        }
        Ok(execution.app_id.clone())
    }

    fn reconcile_command_result(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        original_request_id: &str,
        result: &CommandResult,
        now: i64,
    ) -> Result<(), TaskWorldRuntimeError> {
        if result.status != CommandStatus::Succeeded {
            let message = result
                .error
                .as_ref()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "command provider did not return a result".to_string());
            return self.fail_command_execution(
                graph_id,
                node_id,
                original_request_id,
                &message,
                now,
            );
        }

        let app_id = self.command_app_id(graph_id, node_id, original_request_id)?;
        let value = match result.result.as_ref() {
            Some(value) => value,
            None => {
                let error = TaskWorldRuntimeError::InvalidCommandResult(
                    "successful command routing returned no result object".to_string(),
                );
                self.fail_command_execution(
                    graph_id,
                    node_id,
                    original_request_id,
                    &error.to_string(),
                    now,
                )?;
                return Err(error);
            }
        };
        let command_result = match parse_focus_result(value, original_request_id, &app_id) {
            Ok(command_result) => command_result,
            Err(error) => {
                self.fail_command_execution(
                    graph_id,
                    node_id,
                    original_request_id,
                    &error.to_string(),
                    now,
                )?;
                return Err(error);
            }
        };
        match command_result.status.as_str() {
            "waiting_approval" => self.mutate_command_state(graph_id, node_id, now, |supervisor| {
                supervisor.update_command_execution(
                    node_id,
                    original_request_id,
                    &app_id,
                    TaskCommandExecutionStatus::WaitingApproval,
                    command_result.approval_id.clone(),
                    now,
                )
            }),
            "running" => self.mutate_command_state(graph_id, node_id, now, |supervisor| {
                supervisor.update_command_execution(
                    node_id,
                    original_request_id,
                    &app_id,
                    TaskCommandExecutionStatus::Running,
                    command_result.approval_id.clone(),
                    now,
                )
            }),
            "verified" => {
                if !verified_for_target(&command_result, &app_id) {
                    self.fail_command_execution(
                        graph_id,
                        node_id,
                        original_request_id,
                        "verified result did not contain matching independent verification",
                        now,
                    )?;
                    return Err(TaskWorldRuntimeError::InvalidCommandResult(
                        "verified result failed the independent verification gate".to_string(),
                    ));
                }
                self.mutate_command_state(graph_id, node_id, now, |supervisor| {
                    supervisor.mark_command_verified(node_id, original_request_id, &app_id, now)?;
                    supervisor.succeed_node(
                        node_id,
                        json!({
                            "summary": "desktop.app.focus verified by independent observation",
                            "command": crate::task::executor_ref::DESKTOP_APP_FOCUS_COMMAND,
                            "request_id": original_request_id,
                            "status": "verified",
                        }),
                        now,
                    )
                })
            }
            "failed" => self.fail_command_execution(
                graph_id,
                node_id,
                original_request_id,
                &command_result.reason,
                now,
            ),
            "cancelled" => self.mutate_command_state(graph_id, node_id, now, |supervisor| {
                supervisor.update_command_execution(
                    node_id,
                    original_request_id,
                    &app_id,
                    TaskCommandExecutionStatus::Cancelled,
                    command_result.approval_id.clone(),
                    now,
                )?;
                supervisor.cancel_node(node_id, now)
            }),
            status => Err(TaskWorldRuntimeError::InvalidCommandResult(format!(
                "unsupported desktop.app.focus status: {status}"
            ))),
        }
    }

    fn command_app_id(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        request_id: &str,
    ) -> Result<String, TaskWorldRuntimeError> {
        let supervisors = self.supervisors.read();
        let supervisor = supervisors
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let state = supervisor.node_state(node_id).ok_or_else(|| {
            TaskWorldRuntimeError::Supervisor(TaskSupervisorError::UnknownNode(node_id.to_string()))
        })?;
        let execution = state
            .command_execution
            .as_ref()
            .filter(|execution| execution.request_id == request_id)
            .ok_or_else(|| {
                TaskWorldRuntimeError::InvalidCommandResult(
                    "command result does not match the active node request".to_string(),
                )
            })?;
        Ok(execution.app_id.clone())
    }

    fn fail_command_execution(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        request_id: &str,
        reason: &str,
        now: i64,
    ) -> Result<(), TaskWorldRuntimeError> {
        let app_id = self.command_app_id(graph_id, node_id, request_id)?;
        self.mutate_command_state(graph_id, node_id, now, |supervisor| {
            supervisor.update_command_execution(
                node_id,
                request_id,
                &app_id,
                TaskCommandExecutionStatus::Failed,
                None,
                now,
            )?;
            supervisor.fail_node(node_id, reason, now)
        })
    }

    /// Reconcile a terminal desktop command result back into the matching
    /// Task Harness attempt. Command request ids are the persisted execution
    /// ids, so approval handlers never need client-supplied graph metadata.
    pub fn reconcile_harness_command_result(
        &self,
        execution_id: &str,
        output: Value,
        now: i64,
    ) -> Result<Option<NodeExecution>, TaskWorldRuntimeError> {
        let execution_id = match NodeExecutionId::new(execution_id) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let Some((graph_id, execution)) = self.find_execution(&execution_id) else {
            return Ok(None);
        };
        let plan = self.execution_plan(&graph_id, &execution_id)?;
        let binding = plan.command_binding.ok_or_else(|| {
            TaskWorldRuntimeError::Harness(TaskHarnessError::Graph(
                "persisted command execution is missing its validated binding".to_string(),
            ))
        })?;
        let status = output
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if status == "verified" {
            return self
                .complete_execution(
                    &graph_id,
                    &execution_id,
                    output,
                    ValidationPolicy::CommandVerification {
                        command: binding.command,
                        app_id: binding.args.app_id,
                    },
                    now,
                )
                .map(Some);
        }
        if status == "cancelled" {
            return self
                .update_execution(
                    &graph_id,
                    &execution_id,
                    NodeExecutionStatus::Cancelled,
                    None,
                    now,
                )
                .map(Some);
        }
        if status == "failed" {
            let reason = output
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("desktop command did not verify");
            return self
                .fail_execution(&graph_id, &execution_id, "provider_error", reason, now)
                .map(Some);
        }
        Ok(Some(execution))
    }

    fn mutate_command_state<F>(
        &self,
        graph_id: &TaskGraphId,
        node_id: &TaskNodeId,
        now: i64,
        mutate: F,
    ) -> Result<(), TaskWorldRuntimeError>
    where
        F: FnOnce(&mut TaskSupervisor) -> Result<(), TaskSupervisorError>,
    {
        let mut supervisors = self.supervisors.write();
        let current = supervisors
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        let mut candidate = current.clone();
        mutate(&mut candidate)?;
        self.database
            .save_task_supervisor_snapshot(&candidate, now)?;
        let graph = candidate.graph().clone();
        let projection = candidate
            .node_state(node_id)
            .and_then(|state| state.command_execution.as_ref())
            .map(|execution| {
                json!({
                    "request_id": execution.request_id,
                    "command": execution.command,
                    "app_id": execution.app_id,
                    "status": execution.status,
                    "approval_id": execution.approval_id,
                })
            });
        supervisors.insert(graph.id.clone(), candidate);
        drop(supervisors);
        self.publish(
            "task.node.command.updated",
            json!({
                "graph_id": graph.id.as_str(),
                "graph_revision": graph.revision.value(),
                "node_id": node_id.as_str(),
                "command_execution": projection,
            }),
        );
        Ok(())
    }

    pub fn checkpoint(
        &self,
        graph_id: &TaskGraphId,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskCheckpoint, TaskWorldRuntimeError> {
        let expected_revision = GraphRevision::new(expected_revision)?;
        let supervisors = self.supervisors.read();
        let supervisor = supervisors
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        ensure_revision(supervisor, expected_revision)?;
        if let Some((node_id, execution)) = active_command(supervisor) {
            return Err(TaskWorldRuntimeError::CommandExecutionActive {
                node_id: node_id.to_string(),
                request_id: execution.request_id.clone(),
            });
        }
        let checkpoint = supervisor.checkpoint(now);
        drop(supervisors);

        self.database.save_task_checkpoint(&checkpoint)?;
        self.publish(
            "task.checkpoint.created",
            json!({
                "checkpoint_id": checkpoint.id.as_str(),
                "graph_id": checkpoint.graph_id.as_str(),
                "graph_revision": checkpoint.graph_revision.value(),
            }),
        );
        Ok(checkpoint)
    }

    pub fn restore(
        &self,
        graph_id: &TaskGraphId,
        checkpoint_id: &str,
        expected_revision: u64,
        now: i64,
    ) -> Result<TaskGraph, TaskWorldRuntimeError> {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        let checkpoint = self
            .database
            .load_task_checkpoint(checkpoint_id)?
            .ok_or_else(|| TaskWorldRuntimeError::CheckpointNotFound(checkpoint_id.to_string()))?;
        let expected_revision = GraphRevision::new(expected_revision)?;

        let mut supervisors = self.supervisors.write();
        let current = supervisors
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        ensure_revision(current, expected_revision)?;
        if let Some((node_id, execution)) = active_command(current) {
            return Err(TaskWorldRuntimeError::CommandExecutionActive {
                node_id: node_id.to_string(),
                request_id: execution.request_id.clone(),
            });
        }
        let mut candidate = current.clone();
        self.ensure_no_active_harness_execution(graph_id)?;
        let _restored_revision = candidate.restore(&checkpoint)?;
        candidate.graph().validate()?;
        let graph = candidate.graph().clone();
        let mut harnesses = self.harnesses.write();
        let mut restored_harness = harnesses
            .get(graph_id)
            .cloned()
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        restored_harness.update_graph(graph.clone())?;
        // Restoring definitions never claims prior external effects were undone.
        // Existing attempts for restored nodes become stale, while removed-node
        // attempts remain queryable archival evidence.
        let restored_nodes = graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        restored_harness.mark_stale_nodes(&restored_nodes, now)?;
        let changed_attempts = restored_harness
            .all_attempts()
            .into_iter()
            .filter(|execution| restored_nodes.contains(&execution.node_id))
            .collect::<Vec<_>>();
        self.database.save_task_snapshot_with_executions(
            &candidate,
            now,
            "restored",
            &changed_attempts,
        )?;
        let ready = candidate.runnable_nodes();
        supervisors.insert(graph_id.clone(), candidate);
        harnesses.insert(graph_id.clone(), restored_harness);
        drop(harnesses);
        drop(supervisors);

        self.publish(
            "task.restored",
            json!({
                "checkpoint_id": checkpoint.id.as_str(),
                "graph_id": graph.id.as_str(),
                "graph_revision": graph.revision.value(),
            }),
        );
        self.publish_ready(&graph, ready);
        Ok(graph)
    }

    fn mutate_graph<F>(
        &self,
        graph_id: &TaskGraphId,
        expected_revision: u64,
        now: i64,
        change: &str,
        edit: F,
    ) -> Result<CommittedGraphMutation, TaskWorldRuntimeError>
    where
        F: FnOnce(&mut TaskSupervisor) -> Result<(), TaskSupervisorError>,
    {
        let _control_dispatch_guard = self.control_dispatch_lock.lock();
        self.mutate_graph_locked(graph_id, expected_revision, now, change, edit)
    }

    fn ensure_no_active_harness_execution(
        &self,
        graph_id: &TaskGraphId,
    ) -> Result<(), TaskWorldRuntimeError> {
        self.ensure_no_unsettled_provider(graph_id)?;
        if let Some(execution) = self.harnesses.read().get(graph_id).and_then(|harness| {
            harness
                .all_attempts()
                .into_iter()
                .find(|execution| execution.status.is_active())
        }) {
            return Err(TaskWorldRuntimeError::Harness(
                TaskHarnessError::ActiveExecution(execution.id),
            ));
        }
        Ok(())
    }

    fn mutate_graph_locked<F>(
        &self,
        graph_id: &TaskGraphId,
        expected_revision: u64,
        now: i64,
        change: &str,
        edit: F,
    ) -> Result<CommittedGraphMutation, TaskWorldRuntimeError>
    where
        F: FnOnce(&mut TaskSupervisor) -> Result<(), TaskSupervisorError>,
    {
        let expected_revision = GraphRevision::new(expected_revision)?;
        let mut supervisors = self.supervisors.write();
        let current = supervisors
            .get(graph_id)
            .ok_or_else(|| TaskWorldRuntimeError::GraphNotFound(graph_id.to_string()))?;
        ensure_revision(current, expected_revision)?;
        if let Some((node_id, execution)) = active_command(current) {
            return Err(TaskWorldRuntimeError::CommandExecutionActive {
                node_id: node_id.to_string(),
                request_id: execution.request_id.clone(),
            });
        }
        let before = current.clone();
        self.ensure_no_active_harness_execution(graph_id)?;
        let mut candidate = current.clone();

        // All edits operate on a candidate.  Supervisor graph edits validate
        // before their revision changes; this final validation protects the
        // runtime boundary if another edit path is added later.
        edit(&mut candidate)?;
        candidate.graph().validate()?;
        let newly_ready = newly_ready(&before, &candidate);
        if candidate.graph_revision() != before.graph_revision() {
            self.database
                .save_task_supervisor_snapshot_with_revision(&candidate, now, change)?;
        } else {
            self.database
                .save_task_supervisor_snapshot(&candidate, now)?;
        }
        let graph = candidate.graph().clone();
        let invalidated = candidate
            .node_states()
            .iter()
            .filter(|state| state.status == super::TaskNodeStatus::Invalidated)
            .map(|state| state.node_id.clone())
            .collect::<Vec<_>>();
        supervisors.insert(graph.id.clone(), candidate);
        drop(supervisors);

        let mut harnesses = self.harnesses.write();
        let harness = harnesses
            .entry(graph.id.clone())
            .or_insert(TaskHarness::new(graph.clone(), ExecutorResolver::new())?);
        harness.update_graph(graph.clone())?;
        harness.mark_stale_nodes(&invalidated, now)?;
        for execution in harness.all_attempts() {
            if invalidated
                .iter()
                .any(|node_id| node_id == &execution.node_id)
                && self.database.load_node_execution(&execution.id)?.is_some()
            {
                self.database.update_node_execution(&execution)?;
            }
        }
        drop(harnesses);

        Ok(CommittedGraphMutation { graph, newly_ready })
    }

    fn publish_graph_updated(&self, graph: &TaskGraph, change: &str) {
        self.publish(
            "task.graph.updated",
            json!({
                "graph_id": graph.id.as_str(),
                "graph_revision": graph.revision.value(),
                "change": change,
                "node_count": graph.nodes.len(),
                "edge_count": graph.edges.len(),
            }),
        );
    }

    fn publish_ready(&self, graph: &TaskGraph, node_ids: Vec<TaskNodeId>) {
        for node_id in node_ids {
            self.publish(
                "task.node.ready",
                json!({
                    "graph_id": graph.id.as_str(),
                    "graph_revision": graph.revision.value(),
                    "node_id": node_id.as_str(),
                }),
            );
        }
    }

    fn publish(&self, event_type: &str, payload: Value) {
        // A product event is best-effort at the broadcast boundary: no
        // subscribers is normal, and state has already been durably committed
        // before this point.  The event payload remains a stable fact rather
        // than serialization or scheduler telemetry.
        let _ = self
            .events
            .publish(YiEvent::new(event_type, TASK_WORLD_EVENT_SOURCE, payload));
    }
}

impl TaskProjectionProvider for TaskWorldRuntime {
    fn list(&self, request: &ContextRequest) -> Vec<TaskProjection> {
        if request.max_items == 0 {
            return Vec::new();
        }

        let supervisors = self.supervisors.read();
        let mut projections = supervisors
            .values()
            .map(|supervisor| TaskProjectionProviderAdapter::new(supervisor).project())
            .collect::<Vec<_>>();
        projections.sort_unstable_by(|left, right| left.id.cmp(&right.id));
        let query = request.query.as_deref().map(str::to_lowercase);
        projections
            .into_iter()
            .filter(|projection| {
                query.as_deref().map_or(true, |query| {
                    projection.id.to_lowercase().contains(query)
                        || projection.title.to_lowercase().contains(query)
                })
            })
            .take(request.max_items)
            .collect()
    }
}

fn ensure_revision(
    supervisor: &TaskSupervisor,
    expected: GraphRevision,
) -> Result<(), TaskWorldRuntimeError> {
    let actual = supervisor.graph_revision();
    if actual != expected {
        return Err(TaskWorldRuntimeError::StaleRevision {
            expected: expected.value(),
            actual: actual.value(),
        });
    }
    Ok(())
}

fn active_command(supervisor: &TaskSupervisor) -> Option<(&TaskNodeId, &TaskCommandExecution)> {
    supervisor.node_states().iter().find_map(|state| {
        state
            .command_execution
            .as_ref()
            .filter(|execution| execution.status.is_active())
            .map(|execution| (&state.node_id, execution))
    })
}

struct FocusCommandResult {
    status: String,
    approval_id: Option<String>,
    executed: bool,
    reason: String,
    verification: Option<Value>,
}

fn parse_focus_result(
    value: &Value,
    request_id: &str,
    app_id: &str,
) -> Result<FocusCommandResult, TaskWorldRuntimeError> {
    let object = value.as_object().ok_or_else(|| {
        TaskWorldRuntimeError::InvalidCommandResult(
            "desktop.app.focus result must be a JSON object".to_string(),
        )
    })?;
    let actual_request_id = object
        .get("request_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus result is missing request_id".to_string(),
            )
        })?;
    if actual_request_id != request_id {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(
            "desktop.app.focus result request_id does not match the node request".to_string(),
        ));
    }
    if object.get("command").and_then(Value::as_str)
        != Some(crate::task::executor_ref::DESKTOP_APP_FOCUS_COMMAND)
    {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(
            "desktop.app.focus result command does not match the requested command".to_string(),
        ));
    }
    if object.get("app_id").and_then(Value::as_str) != Some(app_id) {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(
            "desktop.app.focus result app_id does not match the node binding".to_string(),
        ));
    }
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus result is missing status".to_string(),
            )
        })?
        .to_string();
    if !matches!(
        status.as_str(),
        "waiting_approval" | "running" | "verified" | "failed" | "cancelled"
    ) {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(format!(
            "unsupported desktop.app.focus status: {status}"
        )));
    }
    let executed = object
        .get("executed")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus result is missing executed".to_string(),
            )
        })?;
    let simulated = object
        .get("simulated")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus result is missing simulated".to_string(),
            )
        })?;
    if simulated {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(
            "desktop.app.focus cannot complete from a simulated result".to_string(),
        ));
    }
    let display_name = object
        .get("display_name")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus result is missing display_name".to_string(),
            )
        })?;
    if display_name.chars().count() > 500 {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(
            "desktop.app.focus display_name is too long".to_string(),
        ));
    }
    let provider = object
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus result is missing provider".to_string(),
            )
        })?;
    if provider.trim().is_empty() || provider.chars().count() > 128 {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(
            "desktop.app.focus provider is invalid".to_string(),
        ));
    }
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus result is missing reason".to_string(),
            )
        })?
        .to_string();
    let approval_id = match object.get("approval_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if !value.trim().is_empty() && value.len() <= 128 => {
            Some(value.clone())
        }
        _ => {
            return Err(TaskWorldRuntimeError::InvalidCommandResult(
                "desktop.app.focus approval_id is invalid".to_string(),
            ))
        }
    };
    let verification = object.get("verification").cloned();
    if status == "verified" && verification.is_none() {
        return Err(TaskWorldRuntimeError::InvalidCommandResult(
            "verified desktop.app.focus result is missing verification".to_string(),
        ));
    }
    Ok(FocusCommandResult {
        status,
        approval_id,
        executed,
        reason,
        verification,
    })
}

fn verified_for_target(result: &FocusCommandResult, app_id: &str) -> bool {
    if !result.executed {
        return false;
    }
    let Some(Value::Object(verification)) = result.verification.as_ref() else {
        return false;
    };
    verification.get("requested_target").and_then(Value::as_str) == Some(app_id)
        && verification.get("resolved_target").and_then(Value::as_str) == Some(app_id)
        && verification.get("verified").and_then(Value::as_bool) == Some(true)
        && verification.get("action_issued").and_then(Value::as_bool) == Some(true)
        && verification
            .get("fresh_observation_received")
            .and_then(Value::as_bool)
            == Some(true)
        && verification
            .get("foreground_app_matches")
            .and_then(Value::as_bool)
            == Some(true)
        && verification
            .get("foreground_window_matches")
            .and_then(Value::as_bool)
            == Some(true)
        && verification
            .get("target_minimized_after")
            .and_then(Value::as_bool)
            == Some(false)
        && verification
            .get("visibly_presented")
            .and_then(Value::as_bool)
            == Some(true)
        && verification
            .get("observation_provider")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        && verification
            .get("observed_foreground")
            .and_then(Value::as_str)
            == Some(app_id)
        && verification
            .get("reason")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        && verification
            .get("timestamp")
            .and_then(Value::as_i64)
            .is_some()
}

fn newly_ready(before: &TaskSupervisor, after: &TaskSupervisor) -> Vec<TaskNodeId> {
    let before_ready = before.runnable_nodes().into_iter().collect::<HashSet<_>>();
    after
        .runnable_nodes()
        .into_iter()
        .filter(|node_id| !before_ready.contains(node_id))
        .collect()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::shared::context::{ContextRequest, TaskProjectionProvider};
    use crate::shared::event::EventHub;
    use crate::task::{
        GraphRevision, TaskCommandExecutionStatus, TaskGraph, TaskGraphId, TaskNode, TaskNodeId,
        TaskNodeKind, TaskNodeStatus,
    };
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn active_harness_semantic_edits_are_rejected_before_any_persistence() {
        let mut violations = Vec::new();
        for operation in [
            "update",
            "delete",
            "add-node",
            "add-edge",
            "delete-edge",
            "restore",
        ] {
            let database = Database::new(Path::new(":memory:")).unwrap();
            let runtime = TaskWorldRuntime::new(&database, EventHub::new(16)).unwrap();
            let id = TaskGraphId::new("active-edit").unwrap();
            let active = TaskNodeId::new("a").unwrap();
            let dependent = TaskNodeId::new("b").unwrap();
            let unrelated = TaskNodeId::new("c").unwrap();
            let nodes = [&active, &dependent, &unrelated]
                .into_iter()
                .map(|node_id| {
                    TaskNode::new(
                        node_id.clone(),
                        TaskNodeKind::Work,
                        node_id.to_string(),
                        json!({"executor_ref":"workflow://local"}),
                    )
                    .unwrap()
                })
                .collect();
            let original = runtime
                .create_graph(
                    id.clone(),
                    nodes,
                    vec![TaskEdge::new(active.clone(), dependent.clone())],
                    1,
                )
                .unwrap();
            let checkpoint = runtime.checkpoint(&id, 1, 2).unwrap();
            let attempt = runtime
                .start_execution_with_resolver(
                    &id,
                    &active,
                    1,
                    ExecutorResolver::new().with_workflow("local"),
                    3,
                )
                .unwrap();
            let revisions = database.load_task_revision_history(&id).unwrap();
            let history =
                serde_json::to_value(runtime.list_node_executions(&id, &active).unwrap()).unwrap();
            let result = match operation {
                "update" => runtime.update_node(
                    &id,
                    TaskNode::new(active.clone(), TaskNodeKind::Work, "Changed", json!({}))
                        .unwrap(),
                    1,
                    4,
                ),
                "delete" => runtime.delete_node(&id, &active, 1, 4),
                "add-node" => runtime.add_node(
                    &id,
                    TaskNode::new(
                        TaskNodeId::new("new").unwrap(),
                        TaskNodeKind::Work,
                        "New",
                        json!({}),
                    )
                    .unwrap(),
                    1,
                    4,
                ),
                "add-edge" => runtime.add_edge(&id, TaskEdge::new(active.clone(), unrelated), 1, 4),
                "delete-edge" => {
                    runtime.delete_edge(&id, &TaskEdge::new(active.clone(), dependent), 1, 4)
                }
                "restore" => runtime.restore(&id, checkpoint.id.as_str(), 1, 4),
                _ => unreachable!(),
            };
            let unchanged = result.is_err()
                && runtime.get_graph(&id).as_ref() == Some(&original)
                && database
                    .load_task_supervisor_snapshot(&id)
                    .unwrap()
                    .unwrap()
                    .supervisor
                    .graph()
                    == &original
                && database.load_task_revision_history(&id).unwrap() == revisions
                && serde_json::to_value(runtime.list_node_executions(&id, &active).unwrap())
                    .unwrap()
                    == history
                && runtime.find_execution(&attempt.id).unwrap().1.status
                    == NodeExecutionStatus::Dispatching;
            if !unchanged {
                violations.push(operation);
            }
        }
        assert!(
            violations.is_empty(),
            "active edits changed state before rejecting: {violations:?}"
        );
    }

    #[test]
    fn active_harness_keeps_canvas_layout_writes_independent() {
        let database = Database::new(Path::new(":memory:")).unwrap();
        let runtime = TaskWorldRuntime::new(&database, EventHub::new(16)).unwrap();
        let id = TaskGraphId::new("active-layout").unwrap();
        let node_id = TaskNodeId::new("work").unwrap();
        runtime
            .create_graph(
                id.clone(),
                vec![
                    TaskNode::new(node_id.clone(), TaskNodeKind::Work, "Work", json!({})).unwrap(),
                ],
                vec![],
                1,
            )
            .unwrap();
        runtime.start_execution(&id, &node_id, 1, 2).unwrap();
        let mut view = runtime.get_canvas_view(&id).unwrap();
        let revision = view.view_revision;
        view.viewport.x = 42.0;
        let saved = runtime.save_canvas_view(&id, view, revision, 3).unwrap();
        assert_eq!(saved.view_revision, revision + 1);
        assert_eq!(runtime.get_graph(&id).unwrap().revision.value(), 1);
        assert_eq!(
            runtime.list_node_executions(&id, &node_id).unwrap().len(),
            1
        );
    }

    #[test]
    fn execution_cancellation_tokens_validate_target_and_clean_up_terminal_attempts() {
        let database = Database::new(Path::new(":memory:")).unwrap();
        let runtime = TaskWorldRuntime::new(&database, EventHub::new(16)).unwrap();
        let graph_id = TaskGraphId::new("token-owner").unwrap();
        let other_id = TaskGraphId::new("other-graph").unwrap();
        let node_id = TaskNodeId::new("work").unwrap();
        runtime
            .create_graph(
                graph_id.clone(),
                vec![TaskNode::new(
                    node_id.clone(),
                    TaskNodeKind::Work,
                    "Work",
                    json!({"executor_ref":"workflow://local"}),
                )
                .unwrap()],
                vec![],
                1,
            )
            .unwrap();
        runtime
            .create_graph(other_id.clone(), vec![], vec![], 1)
            .unwrap();
        let first = runtime
            .start_execution_with_resolver(
                &graph_id,
                &node_id,
                1,
                ExecutorResolver::new().with_workflow("local"),
                2,
            )
            .unwrap();
        let token = runtime
            .execution_cancellation_token(&first.id)
            .expect("registered before dispatch");
        assert!(runtime
            .cancel_execution(&graph_id, &first.id, 9, 3)
            .is_err());
        assert!(runtime
            .cancel_execution(&other_id, &first.id, 1, 3)
            .is_err());
        assert!(!token.is_cancelled());
        runtime
            .cancel_execution(&graph_id, &first.id, 1, 4)
            .unwrap();
        assert!(token.is_cancelled());
        assert!(runtime.execution_cancellation_token(&first.id).is_none());
        runtime
            .prepare_rerun_from_node(&graph_id, &node_id, 1, 5)
            .unwrap();
        let second = runtime
            .start_execution_with_resolver(
                &graph_id,
                &node_id,
                1,
                ExecutorResolver::new().with_workflow("local"),
                6,
            )
            .unwrap();
        runtime
            .complete_execution(
                &graph_id,
                &second.id,
                json!({"ok":true}),
                ValidationPolicy::StructuredResult,
                7,
            )
            .unwrap();
        assert!(runtime.execution_cancellation_token(&second.id).is_none());
        runtime
            .prepare_rerun_from_node(&graph_id, &node_id, 1, 8)
            .unwrap();
        let third = runtime
            .start_execution_with_resolver(
                &graph_id,
                &node_id,
                1,
                ExecutorResolver::new().with_workflow("local"),
                9,
            )
            .unwrap();
        runtime
            .fail_execution(&graph_id, &third.id, "provider_error", "local failure", 10)
            .unwrap();
        assert!(runtime.execution_cancellation_token(&third.id).is_none());
        assert_eq!(
            runtime
                .list_node_executions(&graph_id, &node_id)
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn graph_creation_rolls_back_when_execution_control_persistence_fails() {
        let database = Database::new(Path::new(":memory:")).unwrap();
        let runtime = TaskWorldRuntime::new(&database, EventHub::new(16)).unwrap();
        database.conn().execute_batch("CREATE TRIGGER reject_control BEFORE INSERT ON task_world_execution_controls BEGIN SELECT RAISE(ABORT, 'injected persistence failure'); END;").unwrap();
        let id = TaskGraphId::new("atomic-create").unwrap();
        assert!(runtime.create_graph(id.clone(), vec![], vec![], 1).is_err());
        assert!(runtime.get_graph(&id).is_none());
        assert!(database
            .load_task_supervisor_snapshot(&id)
            .unwrap()
            .is_none());
        assert!(database.load_task_revision_history(&id).unwrap().is_empty());
    }

    #[test]
    fn second_graph_and_duplicate_concurrent_creation_are_persisted_once() {
        let database = Database::new(Path::new(":memory:")).unwrap();
        let runtime = TaskWorldRuntime::new(&database, EventHub::new(16)).unwrap();
        runtime
            .create_graph(TaskGraphId::new("first").unwrap(), vec![], vec![], 1)
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                barrier.wait();
                runtime.create_graph(TaskGraphId::new("second").unwrap(), vec![], vec![], 2)
            });
            let second = scope.spawn(|| {
                barrier.wait();
                runtime.create_graph(TaskGraphId::new("second").unwrap(), vec![], vec![], 2)
            });
            vec![first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let reloaded = TaskWorldRuntime::new(&database, EventHub::new(16)).unwrap();
        assert_eq!(reloaded.list_graphs().len(), 2);
    }

    #[test]
    fn runtime_create_start_reload_and_publish_facts() {
        let database = Database::new(Path::new(":memory:")).expect("database opens");
        let events = EventHub::new(16);
        let mut subscriber = events.subscribe();
        let runtime = TaskWorldRuntime::new(&database, events.clone()).expect("runtime loads");
        let graph = TaskGraph::new(
            TaskGraphId::new("runtime-graph").unwrap(),
            GraphRevision::initial(),
            vec![TaskNode::new(
                TaskNodeId::new("root").unwrap(),
                TaskNodeKind::Work,
                "Root",
                json!({"source": "runtime-test"}),
            )
            .unwrap()],
            vec![],
        )
        .unwrap();

        runtime
            .create_graph(
                graph.id.clone(),
                graph.nodes.clone(),
                graph.edges.clone(),
                100,
            )
            .unwrap();
        assert_eq!(
            runtime
                .list(&ContextRequest::new("task-world"))
                .first()
                .unwrap()
                .status,
            "runnable"
        );
        assert_eq!(subscriber.try_recv().unwrap().event_type, "task.created");
        assert_eq!(subscriber.try_recv().unwrap().event_type, "task.node.ready");

        runtime
            .start_node(&graph.id, &TaskNodeId::new("root").unwrap(), 1, 101)
            .unwrap();
        assert_eq!(
            runtime
                .list(&ContextRequest::new("task-world"))
                .first()
                .unwrap()
                .status,
            "working"
        );
        assert_eq!(
            subscriber.try_recv().unwrap().event_type,
            "task.node.running"
        );

        let reloaded = TaskWorldRuntime::new(&database, events).expect("runtime reloads");
        assert_eq!(
            reloaded
                .list(&ContextRequest::new("task-world"))
                .first()
                .unwrap()
                .status,
            "working"
        );
    }

    #[test]
    fn runtime_creates_persists_and_reloads_empty_task_graph() {
        let database = Database::new(Path::new(":memory:")).expect("database opens");
        let events = EventHub::new(16);
        let mut subscriber = events.subscribe();
        let runtime = TaskWorldRuntime::new(&database, events.clone()).expect("runtime loads");
        let graph_id = TaskGraphId::new("empty-runtime-canvas").unwrap();

        let created = runtime
            .create_graph(graph_id.clone(), Vec::new(), Vec::new(), 100)
            .expect("empty graph creates");

        assert!(created.nodes.is_empty());
        assert!(created.edges.is_empty());
        assert_eq!(subscriber.try_recv().unwrap().event_type, "task.created");
        assert!(subscriber.try_recv().is_err());
        assert_eq!(
            runtime
                .list(&ContextRequest::new("task-world"))
                .first()
                .unwrap()
                .status,
            "pending"
        );
        let canvas = runtime
            .get_canvas_view(&graph_id)
            .expect("empty graph has an editable canvas view");
        assert!(canvas.node_layouts.is_empty());

        let reloaded = TaskWorldRuntime::new(&database, events).expect("runtime reloads");
        let persisted = reloaded.get_graph(&graph_id).expect("empty graph persists");
        assert!(persisted.nodes.is_empty());
        assert!(persisted.edges.is_empty());
    }

    #[test]
    fn focus_command_completes_only_from_correlated_verified_result() {
        let database = Database::new(Path::new(":memory:")).expect("database opens");
        let events = EventHub::new(16);
        let runtime = TaskWorldRuntime::new(&database, events).expect("runtime loads");
        let graph = TaskGraph::new(
            TaskGraphId::new("focus-runtime").unwrap(),
            GraphRevision::initial(),
            vec![TaskNode::new(
                TaskNodeId::new("focus").unwrap(),
                TaskNodeKind::Work,
                "Focus app",
                json!({
                    "executor_ref": "command://desktop.app.focus",
                    "command_binding": {
                        "command": "desktop.app.focus",
                        "args": {"app_id": "app:code.exe"}
                    }
                }),
            )
            .unwrap()],
            vec![],
        )
        .unwrap();
        runtime
            .create_graph(
                graph.id.clone(),
                graph.nodes.clone(),
                graph.edges.clone(),
                100,
            )
            .unwrap();

        let router = crate::shared::command::CommandRouter::new();
        router
            .register("desktop.app.focus", |request| {
                Ok(json!({
                    "request_id": request.request_id,
                    "command": "desktop.app.focus",
                    "app_id": "app:code.exe",
                    "display_name": "Code",
                    "status": "verified",
                    "approval_id": null,
                    "executed": true,
                    "simulated": false,
                    "provider": "fixture",
                    "reason": "verified fixture",
                    "verification": {
                        "requested_target": "app:code.exe",
                        "resolved_target": "app:code.exe",
                        "action_issued": true,
                        "fresh_observation_received": true,
                        "foreground_app_matches": true,
                        "foreground_window_matches": true,
                        "target_minimized_after": false,
                        "visibly_presented": true,
                        "observation_provider": "fixture",
                        "observed_foreground": "app:code.exe",
                        "verified": true,
                        "reason": "foreground matched",
                        "timestamp": 101
                    }
                }))
            })
            .unwrap();

        let result = runtime
            .execute_node(
                &graph.id,
                &TaskNodeId::new("focus").unwrap(),
                1,
                &router,
                101,
            )
            .expect("verified fixture result is accepted");
        assert_eq!(
            result.status,
            crate::shared::command::CommandStatus::Succeeded
        );
        let detail = runtime
            .get_graph_detail(&graph.id)
            .expect("detail is available");
        assert_eq!(detail.nodes[0].status, TaskNodeStatus::Succeeded);
        assert_eq!(
            detail.nodes[0].result_summary.as_deref(),
            Some("desktop.app.focus verified by independent observation")
        );
        assert_eq!(
            detail.nodes[0]
                .state
                .command_execution
                .as_ref()
                .unwrap()
                .status,
            TaskCommandExecutionStatus::Verified
        );
    }

    #[test]
    fn app_level_match_cannot_complete_a_focus_task_without_visible_window_evidence() {
        let result = FocusCommandResult {
            status: "verified".to_string(),
            approval_id: None,
            executed: true,
            reason: "incorrect app-only verification".to_string(),
            verification: Some(json!({
                "requested_target": "app:code.exe",
                "resolved_target": "app:code.exe",
                "action_issued": true,
                "fresh_observation_received": true,
                "foreground_app_matches": true,
                "foreground_window_matches": true,
                "target_minimized_after": true,
                "visibly_presented": false,
                "observation_provider": "fixture",
                "observed_foreground": "app:code.exe",
                "verified": true,
                "reason": "app matched while minimized",
                "timestamp": 101
            })),
        };

        assert!(!verified_for_target(&result, "app:code.exe"));
    }
}
