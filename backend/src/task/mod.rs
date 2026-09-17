// ============================================================
// Task domain — the product layer above WorkflowRun.
//
// A Task groups one-or-many TaskExecutions (each attempt preserves its own
// plan, timeline, artifacts), an optional TaskPlan, and an optional Agent
// Team. The TaskOrchestrator drives executions sequentially; every side-effect
// still flows through the Security Execution Gateway.
// ============================================================

pub mod adapters;
pub mod approval;
pub mod artifact;
pub mod canvas_view;
pub mod catalog;
pub mod context;
pub mod execution;
pub mod executor;
pub mod executor_ref;
pub mod graph_planner;
pub mod harness;
pub mod model;
pub mod orchestrator;
pub mod planner;
pub mod presence;
pub mod projection;
pub mod recovery;
pub mod runtime;
pub mod scheduler;
pub mod service;
pub mod task_graph;
pub mod task_supervisor;
pub mod timeline;
pub mod validation;
pub mod voice_commands;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod world_tests;

#[cfg(test)]
mod recovery_path_tests;

#[cfg(test)]
mod supervisor_tests;

pub use adapters::{
    AdapterError, AdapterRegistry, AgentExecutionProvider, AgentExecutor,
    CapabilityExecutionProvider, CapabilityExecutor, CommandExecutor, ExecutorDispatch,
    TaskEventPublisher, TaskExecutorAdapter, WorkflowExecutionProvider, WorkflowExecutor,
};
pub use approval::resolve_task_agent_approval;
pub use artifact::{ArtifactError, ArtifactService};
pub use canvas_view::*;
pub use catalog::*;
pub use execution::{
    ExecutionRetryPolicy, NodeContext, NodeExecution, NodeExecutionError, NodeExecutionId,
    NodeExecutionStatus, ValidationResult, ValidationStatus,
};
pub use executor::{
    ExecutorKind, ExecutorResolutionError, ExecutorResolver, ResolvedExecutionPlan,
};
pub use executor_ref::*;
pub use harness::{TaskHarness, TaskHarnessError};
pub use model::*;
pub use orchestrator::{build_task_orchestrator, TaskOrchestrator};
pub use planner::{
    build_planner_capabilities, validate_plan_references, validate_plan_structure, LlmTaskPlanner,
    PlannerCapability, TaskPlanner, TaskPlannerError, TaskPlanningInput, MAX_PLANNER_CAPABILITIES,
    MAX_PLANNER_CAPABILITY_CONTEXT_CHARS,
};
pub use presence::TaskPresenceAdapter;
pub use projection::*;
pub use recovery::{recover_interrupted, RecoveryReport};
pub use runtime::{TaskWorldRuntime, TaskWorldRuntimeError};
pub use service::{OrchestratorBuilder, TaskRunner};
pub use task_graph::*;
pub use task_supervisor::*;
pub use timeline::TimelineService;
pub use voice_commands::{
    register_task_commands, TaskCommandError, TaskCommandService, TaskCurrentProjection,
    TaskExecutionControl, TaskExecutionControlState, TaskStatusProjection, TaskVoiceAdapter,
};
