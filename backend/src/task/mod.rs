// ============================================================
// Task domain — the product layer above WorkflowRun.
//
// A Task groups one-or-many TaskExecutions (each attempt preserves its own
// plan, timeline, artifacts), an optional TaskPlan, and an optional Agent
// Team. The TaskOrchestrator drives executions sequentially; every side-effect
// still flows through the Security Execution Gateway.
// ============================================================

pub mod approval;
pub mod artifact;
pub mod model;
pub mod orchestrator;
pub mod planner;
pub mod recovery;
pub mod service;
pub mod timeline;

#[cfg(test)]
mod tests;

pub use approval::resolve_task_agent_approval;
pub use artifact::{ArtifactError, ArtifactService};
pub use model::*;
pub use orchestrator::{build_task_orchestrator, TaskOrchestrator};
pub use planner::{
    build_planner_capabilities, validate_plan_references, validate_plan_structure, LlmTaskPlanner,
    PlannerCapability, TaskPlanner, TaskPlannerError, TaskPlanningInput, MAX_PLANNER_CAPABILITIES,
    MAX_PLANNER_CAPABILITY_CONTEXT_CHARS,
};
pub use recovery::{recover_interrupted, RecoveryReport};
pub use service::{OrchestratorBuilder, TaskRunner};
pub use timeline::TimelineService;
