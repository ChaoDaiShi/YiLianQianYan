// ============================================================
// Task orchestrator — drives a TaskExecution to completion.
//
// Sequential multi-agent orchestration: plan steps execute one at a time
// (Agent / Workflow / Subagent). Every tool / MCP / subagent call is dispatched
// through the Security Execution Gateway using the task execution's subject —
// never a hard-coded local-user. Delegation is plan-driven (a fixed, validated
// DAG of steps), so there is no A→B→A recursion.
// ============================================================

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::artifact::ArtifactService;
use super::model::*;
use super::planner::TaskPlanner;
use super::timeline::TimelineService;
use crate::agent::verifier::DefaultVerifier;
use crate::config::types::AppConfig;
use crate::db::Database;
use crate::execution::ExecutionContext;
use crate::llm::client::LlmClient;
use crate::llm::types::ChatMessage;
use crate::safety::execution_gateway::{SecurityExecutionOutcome, SecurityGatewayError};
use crate::safety::{
    ApprovalStore, SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject,
};
use crate::server::AppServer;
use crate::workflow::{
    LlmWorkflowAgentExecutor, SecurityGatewayNodeExecutor, WorkflowAgentExecutor, WorkflowRun,
    WorkflowRunId, WorkflowRunner,
};

const MAX_AGENT_CONTEXT_CHARS: usize = 24000;
const MAX_AGENT_RESULT_SUMMARY_CHARS: usize = 8000;

/// Outcome of running a plan step.
enum StepOutcome {
    Done,
    PausedForApproval,
    Failed(String),
}

pub struct TaskOrchestrator {
    db: Database,
    timeline: TimelineService,
    artifacts: ArtifactService,
    planner: Arc<dyn TaskPlanner>,
    gateway: Arc<SecurityExecutionGateway>,
    approval_store: Arc<ApprovalStore>,
    config: Arc<parking_lot::RwLock<AppConfig>>,
    agent_executor: Arc<dyn WorkflowAgentExecutor>,
}

impl TaskOrchestrator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Database,
        timeline: TimelineService,
        artifacts: ArtifactService,
        planner: Arc<dyn TaskPlanner>,
        gateway: Arc<SecurityExecutionGateway>,
        approval_store: Arc<ApprovalStore>,
        config: Arc<parking_lot::RwLock<AppConfig>>,
        agent_executor: Arc<dyn WorkflowAgentExecutor>,
    ) -> Self {
        Self {
            db,
            timeline,
            artifacts,
            planner,
            gateway,
            approval_store,
            config,
            agent_executor,
        }
    }

    /// Run a task execution to terminal state (or pause for approval / decision).
    pub async fn run(
        &self,
        task_id: &TaskId,
        execution_id: &TaskExecutionId,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        let mut task = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| "task not found".to_string())?;
        let mut execution = self
            .db
            .get_task_execution(execution_id)?
            .ok_or_else(|| "task execution not found".to_string())?;

        self.set_execution(&mut execution, TaskExecutionStatus::Running, None, None)?;
        self.set_task(&mut task, TaskStatus::Running, None)?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::TaskStarted,
            "任务已启动",
            serde_json::json!({ "execution_id": execution.id.as_str() }),
        )?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::ExecutionStarted,
            format!(
                "执行 #{}/attempt {}",
                execution.id.as_str(),
                execution.attempt
            ),
            serde_json::json!({}),
        )?;

        if cancel.is_cancelled() {
            return self.finish_cancelled(&task, &execution, cancel).await;
        }

        // Strategy A: a workflow-bound task runs the workflow directly.
        if let Some(workflow_graph_id) = &task.workflow_graph_id {
            let outcome = self
                .run_workflow_for_task(&task, &mut execution, workflow_graph_id, cancel)
                .await;
            return self
                .finish_from_outcome(&task, &execution, outcome, cancel)
                .await;
        }

        // Strategy B: plan-driven (Planner → sequential steps).
        let plan = self.plan_or_load(&task, &execution).await?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::PlanCreated,
            format!("计划已生成：{} 个步骤", plan.steps.len()),
            serde_json::json!({ "summary": plan.summary }),
        )?;

        for (index, step) in plan.steps.iter().enumerate() {
            if cancel.is_cancelled() {
                return self.finish_cancelled(&task, &execution, cancel).await;
            }
            let outcome = self
                .execute_step(&task, &mut execution, &plan, index, step, cancel)
                .await?;
            match outcome {
                StepOutcome::Done => {}
                StepOutcome::PausedForApproval => {
                    return self.finish_paused_approval(&task, &execution).await;
                }
                StepOutcome::Failed(message) => {
                    return self.finish_failed(&task, &execution, message, cancel).await;
                }
            }
        }

        // All steps done → task complete + a Task Summary artifact.
        self.mark_completed(&task, &execution).await
    }

    // ── Plan loading ──

    async fn plan_or_load(
        &self,
        task: &Task,
        execution: &TaskExecution,
    ) -> Result<TaskPlan, String> {
        if let Some(plan) = self.db.get_latest_task_plan(&task.id, &execution.id)? {
            return Ok(plan);
        }
        let workspace = self
            .db
            .get_workspace(&task.workspace_id)?
            .ok_or_else(|| "workspace not found".to_string())?;
        let input = super::planner::TaskPlanningInput {
            title: task.title.clone(),
            description: task.description.clone(),
            workspace_name: workspace.name,
            workspace_description: workspace.description,
        };
        let cancel = CancellationToken::new();
        let plan = self
            .planner
            .plan(input, &cancel)
            .await
            .map_err(|error| error.to_string())?;
        super::planner::validate_plan_structure(&plan)
            .map_err(|error| format!("invalid plan: {error}"))?;
        self.db.create_task_plan(
            &uuid::Uuid::new_v4().to_string(),
            &task.id,
            &execution.id,
            &plan,
            now(),
        )?;
        Ok(plan)
    }

    // ── Step execution ──

    async fn execute_step(
        &self,
        task: &Task,
        execution: &mut TaskExecution,
        _plan: &TaskPlan,
        index: usize,
        step: &TaskPlanStep,
        cancel: &CancellationToken,
    ) -> Result<StepOutcome, String> {
        match &step.executor {
            TaskPlanExecutor::Workflow { workflow_graph_id } => {
                self.timeline
                    .record(
                        &task.workspace_id,
                        &task.id,
                        Some(&execution.id),
                        TaskEventType::WorkflowStarted,
                        format!("步骤 {}：启动工作流", step.title),
                        serde_json::json!({ "workflow_graph_id": workflow_graph_id }),
                    )
                    .ok();
                match self
                    .run_workflow_for_task(task, execution, workflow_graph_id, cancel)
                    .await
                {
                    Ok(()) => Ok(StepOutcome::Done),
                    Err(message) => Ok(StepOutcome::Failed(message)),
                }
            }
            TaskPlanExecutor::Subagent { name } => {
                self.run_subagent_step(task, execution, index, step, name, cancel)
                    .await
            }
            TaskPlanExecutor::Agent { agent_id } => {
                self.run_agent_step(task, execution, index, step, agent_id, cancel)
                    .await
            }
        }
    }

    // ── Workflow execution ──

    async fn run_workflow_for_task(
        &self,
        task: &Task,
        execution: &mut TaskExecution,
        workflow_graph_id: &str,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        let graph = self
            .db
            .get_workflow_graph(workflow_graph_id)?
            .ok_or_else(|| format!("workflow graph not found: {workflow_graph_id}"))?;

        let now = now();
        let ctx = ExecutionContext::new(
            execution.execution_context.execution_id.clone(),
            execution.execution_context.subject_id.clone(),
            "task-workflow",
            Some(execution.execution_context.execution_id.clone()),
            now,
        );
        let mut run = WorkflowRun::new(
            WorkflowRunId::generate(),
            ctx,
            graph.definition.clone(),
            now,
        )
        .map_err(|error| error.to_string())?;
        let run_id = run.run_id.clone();
        execution.workflow_run_id = Some(run_id.clone());
        execution.updated_at = now;
        self.db.update_task_execution(execution)?;

        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::WorkflowStarted,
            format!("启动工作流 {}", graph.name),
            serde_json::json!({ "workflow_run_id": run_id.as_str() }),
        )?;

        let executor = SecurityGatewayNodeExecutor::new(
            Arc::clone(&self.gateway),
            Arc::clone(&self.approval_store),
        )
        .with_agent_executor(Arc::clone(&self.agent_executor));
        let runner = WorkflowRunner::new(executor);
        let db = self.db.clone_connection();
        let graph_id = workflow_graph_id.to_string();
        let result = runner
            .run(&mut run, cancel, move |r| {
                db.update_workflow_run(&graph_id, r)
            })
            .await;

        match (result, run.status) {
            (Ok(()), crate::workflow::WorkflowRunStatus::Completed) => {
                self.timeline.record(
                    &task.workspace_id,
                    &task.id,
                    Some(&execution.id),
                    TaskEventType::WorkflowCompleted,
                    format!("工作流 {} 完成", graph.name),
                    serde_json::json!({ "workflow_run_id": run_id.as_str() }),
                )?;
                // Workflow final Output result → a Text artifact.
                self.capture_workflow_output_artifact(task, execution, &run)
                    .await;
                Ok(())
            }
            (Ok(()), crate::workflow::WorkflowRunStatus::WaitingApproval) => {
                Err("approval_paused".to_string())
            }
            (Ok(()), crate::workflow::WorkflowRunStatus::Cancelled) => Err("cancelled".to_string()),
            _ => Err("workflow failed or ended unexpectedly".to_string()),
        }
    }

    async fn capture_workflow_output_artifact(
        &self,
        task: &Task,
        execution: &TaskExecution,
        run: &WorkflowRun,
    ) {
        let output = run
            .node_states
            .iter()
            .find(|s| {
                matches!(
                    run.definition.nodes.iter().find(|n| n.id == s.node_id),
                    Some(n) if n.kind == crate::workflow::WorkflowNodeKind::Output
                )
            })
            .and_then(|s| s.result.as_ref())
            .map(|r| r.summary.clone())
            .unwrap_or_else(|| "工作流执行完成".to_string());
        let _ = self
            .artifacts
            .register(
                &task.workspace_id,
                &task.id,
                &execution.id,
                "Workflow Output",
                ArtifactType::Text,
                None,
                None,
                None,
                output,
                None,
                now(),
            )
            .map(|artifact| {
                let _ = self.timeline.record(
                    &task.workspace_id,
                    &task.id,
                    Some(&execution.id),
                    TaskEventType::ArtifactCreated,
                    format!("产物已生成：{}", artifact.name),
                    serde_json::json!({ "artifact_id": artifact.id.as_str() }),
                );
            });
    }

    // ── Agent step (bounded LLM + tools via gateway) ──

    async fn run_agent_step(
        &self,
        task: &Task,
        execution: &mut TaskExecution,
        _index: usize,
        step: &TaskPlanStep,
        agent_id: &AgentId,
        cancel: &CancellationToken,
    ) -> Result<StepOutcome, String> {
        let agent = match self.agent_definition(agent_id) {
            Some(agent) => agent,
            None => return Ok(StepOutcome::Failed("agent not found".to_string())),
        };
        if !agent.enabled {
            return Ok(StepOutcome::Failed("agent is disabled".to_string()));
        }

        let count = self.db.count_agent_executions(&execution.id).unwrap_or(0);
        let policy = self.delegation_policy_for(execution).unwrap_or_default();
        if count as u32 >= policy.max_agent_executions {
            return Ok(StepOutcome::Failed(
                "max agent executions reached".to_string(),
            ));
        }

        let mut agent_execution = AgentExecution {
            id: AgentExecutionId::generate(),
            task_execution_id: execution.id.clone(),
            agent_id: agent_id.clone(),
            parent_agent_execution_id: None,
            depth: 0,
            instruction: step.instruction.clone(),
            status: AgentExecutionStatus::Created,
            result_summary: None,
            agent_state_json: None,
            started_at: None,
            finished_at: None,
            error: None,
            created_at: now(),
            updated_at: now(),
        };
        self.db.create_agent_execution(&agent_execution)?;
        self.timeline
            .record(
                &task.workspace_id,
                &task.id,
                Some(&execution.id),
                TaskEventType::AgentDelegated,
                format!("步骤 {}：委派给 {} ({})", step.title, agent.name, agent.id),
                serde_json::json!({ "agent_execution_id": agent_execution.id.as_str() }),
            )
            .ok();

        agent_execution.status = AgentExecutionStatus::Running;
        agent_execution.started_at = Some(now());
        self.db.update_agent_execution(&agent_execution)?;

        let context = build_task_context(task, step, &self.db, &execution.id);
        let outcome = self
            .agent_loop(
                task,
                execution,
                &mut agent_execution,
                &agent,
                &context,
                cancel,
            )
            .await;

        match outcome {
            AgentLoopOutcome::Completed(summary) => {
                agent_execution.status = AgentExecutionStatus::Completed;
                agent_execution.result_summary = Some(summary.clone());
                agent_execution.finished_at = Some(now());
                self.db.update_agent_execution(&agent_execution)?;
                self.timeline
                    .record(
                        &task.workspace_id,
                        &task.id,
                        Some(&execution.id),
                        TaskEventType::AgentCompleted,
                        format!("{} 完成", agent.name),
                        serde_json::json!({ "agent_execution_id": agent_execution.id.as_str() }),
                    )
                    .ok();
                Ok(StepOutcome::Done)
            }
            AgentLoopOutcome::PausedApproval => {
                agent_execution.status = AgentExecutionStatus::WaitingApproval;
                self.db.update_agent_execution(&agent_execution)?;
                Ok(StepOutcome::PausedForApproval)
            }
            AgentLoopOutcome::Failed(message) => {
                agent_execution.status = AgentExecutionStatus::Failed;
                agent_execution.error = Some(message.clone());
                agent_execution.finished_at = Some(now());
                self.db.update_agent_execution(&agent_execution)?;
                self.timeline
                    .record(
                        &task.workspace_id,
                        &task.id,
                        Some(&execution.id),
                        TaskEventType::AgentFailed,
                        format!("{} 失败：{}", agent.name, message),
                        serde_json::json!({ "agent_execution_id": agent_execution.id.as_str() }),
                    )
                    .ok();
                Ok(StepOutcome::Failed(message))
            }
        }
    }

    async fn run_subagent_step(
        &self,
        _task: &Task,
        execution: &mut TaskExecution,
        _index: usize,
        step: &TaskPlanStep,
        name: &str,
        _cancel: &CancellationToken,
    ) -> Result<StepOutcome, String> {
        let Some(tool_name) = crate::tools::subagent::subagent_tool_name(name) else {
            return Ok(StepOutcome::Failed(format!(
                "invalid subagent name: {name}"
            )));
        };
        let request = SecurityExecutionRequest {
            conversation_id: execution.execution_context.execution_id.to_string(),
            tool_call_id: format!("{}:{}", execution.id, step.id),
            tool_name: tool_name.clone(),
            arguments: serde_json::json!({ "task": step.instruction }),
            subject: SecuritySubject::from_subject_id(
                execution.execution_context.subject_id.clone(),
            ),
        };
        match self
            .gateway
            .execute(&request, crate::tools::RiskLevel::Low)
            .await
        {
            Ok(SecurityExecutionOutcome::Executed { tool_result, .. }) if tool_result.ok => {
                Ok(StepOutcome::Done)
            }
            Ok(SecurityExecutionOutcome::RequiresApproval { .. }) => {
                Ok(StepOutcome::PausedForApproval)
            }
            Ok(SecurityExecutionOutcome::Executed { tool_result, .. }) => Ok(StepOutcome::Failed(
                tool_result
                    .error
                    .unwrap_or_else(|| "subagent failed".to_string()),
            )),
            Ok(SecurityExecutionOutcome::Denied { reason }) => Ok(StepOutcome::Failed(reason)),
            Err(error) => Ok(StepOutcome::Failed(error.to_string())),
        }
    }

    // ── Bounded agent loop ──

    async fn agent_loop(
        &self,
        task: &Task,
        execution: &TaskExecution,
        agent_execution: &mut AgentExecution,
        agent: &AgentDefinition,
        context: &str,
        cancel: &CancellationToken,
    ) -> AgentLoopOutcome {
        let mut messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(format!(
                    "你是工作流任务中的 Agent「{}」。\n{}",
                    agent.name, agent.instructions
                )),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(format!(
                    "{context}\n\n任务指令：{}",
                    agent_execution.instruction
                )),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];

        let llm = self.llm_for(agent);
        let tools = self.tool_definitions(&agent.allowed_tools);
        let max_iterations = agent.max_iterations.max(1);

        for _iteration in 0..max_iterations {
            if cancel.is_cancelled() {
                return AgentLoopOutcome::Failed("cancelled".to_string());
            }
            let response = match llm.invoke(&messages, &tools).await {
                Ok(response) => response,
                Err(error) => return AgentLoopOutcome::Failed(error.to_string()),
            };
            let Some(choice) = response.choices.into_iter().next() else {
                return AgentLoopOutcome::Failed("empty LLM response".to_string());
            };
            let message = choice.message;

            let has_tool_calls = message.tool_calls.as_ref().is_some_and(|c| !c.is_empty());
            let content = message.content.unwrap_or_default();

            if has_tool_calls {
                let mut tool_results = Vec::new();
                let calls = message.tool_calls.as_ref().unwrap();
                for (call_index, call) in calls.iter().enumerate() {
                    let args: serde_json::Value = serde_json::from_str(&call.function.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let request = SecurityExecutionRequest {
                        conversation_id: execution.execution_context.execution_id.to_string(),
                        tool_call_id: format!("{}:{}", agent_execution.id, call_index),
                        tool_name: call.function.name.clone(),
                        arguments: args.clone(),
                        subject: SecuritySubject::from_subject_id(
                            execution.execution_context.subject_id.clone(),
                        ),
                    };
                    match self
                        .gateway
                        .execute(&request, crate::tools::RiskLevel::Low)
                        .await
                    {
                        Ok(SecurityExecutionOutcome::Executed { tool_result, .. }) => {
                            tool_results.push(serde_json::json!({
                                "tool_call_id": call.id,
                                "tool_name": call.function.name,
                                "ok": tool_result.ok,
                                "content": crate::workflow::safe_tool_result_summary(&tool_result.content),
                            }));
                        }
                        Ok(SecurityExecutionOutcome::RequiresApproval { .. }) => {
                            // Pause: persist a bounded state snapshot so resume can continue.
                            let snapshot = bounded_messages_snapshot(&messages);
                            agent_execution.agent_state_json = Some(snapshot);
                            agent_execution.status = AgentExecutionStatus::WaitingApproval;
                            self.db.update_agent_execution(agent_execution).ok();
                            self.timeline
                                .record(
                                    &task.workspace_id,
                                    &task.id,
                                    Some(&execution.id),
                                    TaskEventType::ApprovalRequired,
                                    format!("{} 需要审批：{}", agent.name, call.function.name),
                                    serde_json::json!({ "agent_execution_id": agent_execution.id.as_str() }),
                                )
                                .ok();
                            return AgentLoopOutcome::PausedApproval;
                        }
                        Ok(SecurityExecutionOutcome::Denied { reason }) => {
                            tool_results.push(serde_json::json!({
                                "tool_call_id": call.id,
                                "tool_name": call.function.name,
                                "ok": false,
                                "content": format!("安全拒绝：{reason}"),
                            }));
                        }
                        Err(SecurityGatewayError::ToolNotFound(name)) => {
                            tool_results.push(serde_json::json!({
                                "tool_call_id": call.id,
                                "tool_name": call.function.name,
                                "ok": false,
                                "content": format!("工具不存在：{name}"),
                            }));
                        }
                        Err(error) => {
                            tool_results.push(serde_json::json!({
                                "tool_call_id": call.id,
                                "tool_name": call.function.name,
                                "ok": false,
                                "content": format!("工具执行错误：{error}"),
                            }));
                        }
                    }
                }

                // Append the assistant tool-call message + results to continue.
                let assistant_message = ChatMessage {
                    role: "assistant".to_string(),
                    content: Some(content),
                    tool_calls: message.tool_calls.clone(),
                    tool_call_id: None,
                    name: None,
                };
                messages.push(assistant_message);
                for result in tool_results {
                    messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(result["content"].as_str().unwrap_or_default().to_string()),
                        tool_calls: None,
                        tool_call_id: Some(
                            result["tool_call_id"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                        ),
                        name: result["tool_name"].as_str().map(str::to_string),
                    });
                }
                continue;
            }

            // Text answer → done.
            let summary =
                crate::utils::text::truncate_chars(&content, MAX_AGENT_RESULT_SUMMARY_CHARS);
            return AgentLoopOutcome::Completed(summary);
        }
        AgentLoopOutcome::Failed("agent exceeded max iterations without a final answer".to_string())
    }

    // ── Finalization ──

    async fn finish_from_outcome(
        &self,
        task: &Task,
        execution: &TaskExecution,
        outcome: Result<(), String>,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        match outcome {
            Ok(()) => self.mark_completed(task, execution).await,
            Err(message) if message == "approval_paused" => {
                self.finish_paused_approval(task, execution).await
            }
            Err(message) if message == "cancelled" => {
                self.finish_cancelled(task, execution, cancel).await
            }
            Err(message) => self.finish_failed(task, execution, message, cancel).await,
        }
    }

    async fn mark_completed(&self, task: &Task, execution: &TaskExecution) -> Result<(), String> {
        let now = now();
        let mut execution = execution.clone();
        self.set_execution(&mut execution, TaskExecutionStatus::Completed, None, None)?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::ExecutionCompleted,
            "执行完成",
            serde_json::json!({}),
        )?;
        let mut task = task.clone();
        self.set_task(&mut task, TaskStatus::Completed, Some(now))?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            None,
            TaskEventType::TaskCompleted,
            "任务完成",
            serde_json::json!({}),
        )?;
        // Task Summary artifact.
        let _ = self
            .artifacts
            .register(
                &task.workspace_id,
                &task.id,
                &execution.id,
                "Task Summary",
                ArtifactType::Text,
                None,
                None,
                None,
                format!("任务「{}」已完成", task.title),
                None,
                now,
            )
            .map(|artifact| {
                let _ = self.timeline.record(
                    &task.workspace_id,
                    &task.id,
                    Some(&execution.id),
                    TaskEventType::ArtifactCreated,
                    format!("产物已生成：{}", artifact.name),
                    serde_json::json!({ "artifact_id": artifact.id.as_str() }),
                );
            });
        Ok(())
    }

    async fn finish_failed(
        &self,
        task: &Task,
        execution: &TaskExecution,
        message: String,
        _cancel: &CancellationToken,
    ) -> Result<(), String> {
        let mut execution = execution.clone();
        self.set_execution(
            &mut execution,
            TaskExecutionStatus::Failed,
            Some(message.clone()),
            None,
        )?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::ExecutionFailed,
            format!("执行失败：{message}"),
            serde_json::json!({}),
        )?;
        let mut task = task.clone();
        self.set_task(&mut task, TaskStatus::Failed, None)?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            None,
            TaskEventType::TaskFailed,
            format!("任务失败：{message}"),
            serde_json::json!({}),
        )?;
        Ok(())
    }

    async fn finish_paused_approval(
        &self,
        task: &Task,
        execution: &TaskExecution,
    ) -> Result<(), String> {
        let mut execution = execution.clone();
        self.set_execution(
            &mut execution,
            TaskExecutionStatus::WaitingApproval,
            None,
            None,
        )?;
        let mut task = task.clone();
        self.set_task(&mut task, TaskStatus::WaitingApproval, None)?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::ApprovalRequired,
            "等待审批",
            serde_json::json!({}),
        )?;
        Ok(())
    }

    async fn finish_cancelled(
        &self,
        task: &Task,
        execution: &TaskExecution,
        _cancel: &CancellationToken,
    ) -> Result<(), String> {
        let mut execution = execution.clone();
        self.set_execution(&mut execution, TaskExecutionStatus::Cancelled, None, None)?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            Some(&execution.id),
            TaskEventType::ExecutionCancelled,
            "执行已取消",
            serde_json::json!({}),
        )?;
        let mut task = task.clone();
        self.set_task(&mut task, TaskStatus::Cancelled, None)?;
        self.timeline.record(
            &task.workspace_id,
            &task.id,
            None,
            TaskEventType::TaskCancelled,
            "任务已取消",
            serde_json::json!({}),
        )?;
        Ok(())
    }

    // ── Helpers ──

    fn set_execution(
        &self,
        execution: &mut TaskExecution,
        status: TaskExecutionStatus,
        error: Option<String>,
        started_at: Option<i64>,
    ) -> Result<(), String> {
        if !execution_transition_allowed(execution.status, status) {
            return Err(format!(
                "invalid execution transition: {} -> {}",
                execution.status, status
            ));
        }
        execution.status = status;
        if status == TaskExecutionStatus::Running && execution.started_at.is_none() {
            execution.started_at = Some(started_at.unwrap_or_else(now));
        }
        if status.is_terminal() && execution.finished_at.is_none() {
            execution.finished_at = Some(now());
        }
        execution.error = error;
        execution.updated_at = now();
        self.db.update_task_execution(execution)
    }

    fn set_task(
        &self,
        task: &mut Task,
        status: TaskStatus,
        completed_at: Option<i64>,
    ) -> Result<(), String> {
        if !task_transition_allowed(task.status, status) {
            return Err(format!(
                "invalid task transition: {} -> {}",
                task.status, status
            ));
        }
        task.status = status;
        task.completed_at = completed_at;
        task.updated_at = now();
        self.db.update_task(task)
    }

    fn agent_definition(&self, agent_id: &AgentId) -> Option<AgentDefinition> {
        self.db.get_agent_definition(agent_id).ok().flatten()
    }

    fn delegation_policy_for(&self, execution: &TaskExecution) -> Option<DelegationPolicy> {
        let team_id = execution.agent_team_id.as_ref()?;
        self.db
            .get_agent_team(team_id)
            .ok()
            .flatten()
            .map(|t| t.delegation_policy)
    }

    fn llm_for(&self, agent: &AgentDefinition) -> LlmClient {
        let config = self.config.read().clone();
        let mut model = config.model.clone();
        if let Some(agent_model) = &agent.model {
            model.name = agent_model.clone();
        }
        LlmClient::new(&model)
    }

    fn tool_definitions(&self, allowed_tools: &[String]) -> Vec<serde_json::Value> {
        // The gateway's registry is authoritative for actual tool availability;
        // here we only build the LLM-visible definitions from the allowed set.
        let registry = Arc::new(crate::tools::ToolRegistry::with_defaults(""));
        registry
            .all()
            .iter()
            .filter(|tool| allowed_tools.iter().any(|name| name == tool.name()))
            .map(|tool| tool.to_openai_tool())
            .collect()
    }
}

enum AgentLoopOutcome {
    Completed(String),
    PausedApproval,
    Failed(String),
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Build a bounded task context for an agent. Truncates to a safe char bound.
fn build_task_context(
    task: &Task,
    step: &TaskPlanStep,
    db: &Database,
    execution_id: &TaskExecutionId,
) -> String {
    let mut parts = Vec::new();
    parts.push(format!("任务标题：{}", task.title));
    if !task.description.is_empty() {
        parts.push(format!("任务描述：{}", task.description));
    }
    parts.push(format!("当前步骤：{}", step.title));
    parts.push(format!("步骤指令：{}", step.instruction));

    // Recent timeline summaries (bounded).
    if let Ok(events) = db.list_task_events(&task.id) {
        let recent: Vec<String> = events
            .iter()
            .rev()
            .take(5)
            .map(|event| {
                format!(
                    "[{}] {}",
                    event.event_type.to_string().replace('_', " "),
                    event.message
                )
            })
            .collect();
        if !recent.is_empty() {
            parts.push(format!("最近时间线：{}", recent.join("；")));
        }
    }
    // Artifact summaries (bounded).
    if let Ok(artifacts) = db.list_artifacts(&crate::db::ArtifactQuery {
        task_execution_id: Some(execution_id.clone()),
        ..Default::default()
    }) {
        let summaries: Vec<String> = artifacts
            .iter()
            .map(|artifact| format!("{}：{}", artifact.name, artifact.summary))
            .collect();
        if !summaries.is_empty() {
            parts.push(format!("已有产物：{}", summaries.join("；")));
        }
    }

    let joined = parts.join("\n");
    crate::utils::text::truncate_chars(&joined, MAX_AGENT_CONTEXT_CHARS)
}

/// Persist a bounded snapshot of the conversation for approval-resume.
fn bounded_messages_snapshot(messages: &[ChatMessage]) -> String {
    let value: Vec<serde_json::Value> = messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "role": message.role,
                "content": message.content,
                "name": message.name,
            })
        })
        .collect();
    let text = serde_json::to_string(&value).unwrap_or_default();
    crate::utils::text::truncate_chars(&text, 40000)
}

/// Build a production orchestrator from an AppServer (API layer convenience).
pub async fn build_task_orchestrator(server: &AppServer) -> TaskOrchestrator {
    let tool_registry = server.build_agent_tool_registry().await;
    let config = server.config.read().clone();
    let gateway = Arc::new(
        SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            config.sandbox.clone(),
            server.workspace_root.clone(),
            Arc::clone(&tool_registry),
            Arc::new(DefaultVerifier::new(&server.workspace_root)),
            Arc::new(server.audit_recorder.clone()),
        )
        .with_db(Arc::new(server.db.clone_connection())),
    );
    let planner = Arc::new(super::planner::LlmTaskPlanner::new(&config.model));
    let agent_executor = Arc::new(LlmWorkflowAgentExecutor::new(&config.model));
    TaskOrchestrator::new(
        server.db.clone_connection(),
        TimelineService::new(server.db.clone_connection()),
        ArtifactService::new(server.db.clone_connection()),
        planner,
        gateway,
        Arc::clone(&server.approval_store),
        Arc::clone(&server.config),
        agent_executor,
    )
}
