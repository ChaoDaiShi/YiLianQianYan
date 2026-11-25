// ============================================================
// Chat API — POST /api/chat (SSE streaming)
// ============================================================

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
#[path = "chat_cli.rs"]
pub(crate) mod cli;
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[path = "chat_grounding.rs"]
pub(crate) mod grounding;
#[path = "chat_lanes.rs"]
pub(crate) mod lanes;
#[path = "chat_lifecycle.rs"]
pub(crate) mod lifecycle;
#[path = "chat_options.rs"]
pub(crate) mod options;
#[path = "chat_recall.rs"]
pub(crate) mod recall;
#[path = "chat_runtime.rs"]
pub(crate) mod runtime;
#[path = "chat_steering.rs"]
pub(crate) mod steering;
#[path = "chat_task_commands.rs"]
pub(crate) mod task_commands;
#[path = "chat_understanding.rs"]
mod understanding;

pub use crate::agent::delegation::GoalRef as ChatGoalRef;
use crate::agent::engine::{self, AgentEvent};
use crate::agent::state::AgentState;
use crate::app::state::AppServer;
use crate::db::{Database, MessageRow};
use crate::modules::task::proofloop::budget::GoalModelReservation;
use crate::utils::text::truncate_chars;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    #[serde(flatten)]
    pub options: options::ChatRequestOptions,
    pub conversation_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub resource_ids: Vec<String>,
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub goal_ref: Option<ChatGoalRef>,
}

#[derive(serde::Deserialize)]
pub struct ChatReadinessQuery {
    pub conversation_id: Option<String>,
}
pub async fn readiness_handler(
    State(server): State<Arc<AppServer>>,
    axum::extract::Query(query): axum::extract::Query<ChatReadinessQuery>,
) -> Json<runtime::ChatReadiness> {
    Json(runtime::readiness(&server, query.conversation_id.as_deref()).await)
}

struct GoalBoundChatContext {
    goal_id: String,
    goal_version: u32,
    goal_summary: String,
    evidence_ids: Vec<String>,
    deadline: tokio::time::Instant,
    budget: Arc<crate::modules::task::resource_budget::SharedExecutionBudget>,
    reservation: GoalModelReservation,
    activity_lease: crate::modules::task::proofloop::budget::BudgetLease,
    task_lease: crate::modules::task::runtime::TaskGoalChatLease,
}

pub(crate) struct GoalChatRun {
    pub(crate) scope: crate::agent::delegation::ExecutionScope,
    pub(crate) goal_ref: ChatGoalRef,
    pub(crate) goal_summary: String,
    reservation: GoalModelReservation,
    activity_lease: crate::modules::task::proofloop::budget::BudgetLease,
    _task_lease: crate::modules::task::runtime::TaskGoalChatLease,
}

impl GoalChatRun {
    pub(crate) fn finish(self) -> Result<(), String> {
        let Self {
            mut reservation,
            activity_lease,
            ..
        } = self;
        let usage_finish = reservation.finish();
        let activity_finish = activity_lease.finish().map_err(|error| error.to_string());
        if let Err(error) = usage_finish {
            Err(format!(
                "ProofLoop model usage could not be settled: {error}"
            ))
        } else if let Err(error) = activity_finish {
            Err(format!(
                "ProofLoop active time could not be settled: {error}"
            ))
        } else {
            Ok(())
        }
    }
}

pub(crate) type ChatApiError = (StatusCode, Json<serde_json::Value>);

pub(crate) fn chat_error_message(error: ChatApiError) -> String {
    let body = error.1 .0;
    let code = body
        .get("code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("proofloop_goal_context_unavailable");
    let message = body
        .get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("目标聊天上下文无法验证");
    format!("{code}: {message}")
}

fn chat_error(status: StatusCode, code: &str, message: impl Into<String>) -> ChatApiError {
    (
        status,
        Json(serde_json::json!({ "code": code, "error": message.into() })),
    )
}

fn preparation_cancelled() -> ChatApiError {
    chat_error(
        StatusCode::CONFLICT,
        "chat_cancelled",
        "已取消本次消息准备，请重新发送",
    )
}

pub(crate) fn load_conversation_goal_ref(
    db: &Database,
    conversation_id: &str,
) -> Result<Option<ChatGoalRef>, ChatApiError> {
    db.load_conversation_goal_binding(conversation_id)
        .map(|binding| {
            binding.map(|binding| ChatGoalRef {
                goal_id: binding.goal_id,
                goal_version: binding.goal_version,
            })
        })
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_goal_binding_read_failed",
                error,
            )
        })
}

fn resolve_conversation_goal_ref(
    requested: Option<&ChatGoalRef>,
    persisted: Option<ChatGoalRef>,
) -> Result<Option<ChatGoalRef>, ChatApiError> {
    if requested
        .zip(persisted.as_ref())
        .is_some_and(|(requested, persisted)| requested != persisted)
    {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_goal_rebind_rejected",
            "此对话已绑定到其他目标版本，不能重新绑定",
        ));
    }
    Ok(requested.cloned().or(persisted))
}

fn persist_conversation_goal_ref(
    db: &Database,
    conversation_id: &str,
    reference: &ChatGoalRef,
) -> Result<(), ChatApiError> {
    let binding = crate::db::ConversationGoalBinding {
        conversation_id: conversation_id.to_string(),
        goal_id: reference.goal_id.clone(),
        goal_version: reference.goal_version,
    };
    let contract = db
        .load_goal_version(&reference.goal_id, reference.goal_version)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_goal_read_failed",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_goal_not_confirmed",
                "目标记录不可用",
            )
        })?;
    let bound = db
        .bind_conversation_goal_with_workspace(&binding, Some(&contract.workspace_id))
        .map_err(|error| {
            if error == "conversation_goal_workspace_conflict" {
                chat_error(
                    StatusCode::CONFLICT,
                    "conversation_goal_workspace_conflict",
                    "对话与目标属于不同工作空间",
                )
            } else {
                chat_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "proofloop_goal_binding_persist_failed",
                    error,
                )
            }
        })?;
    if !bound {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_goal_rebind_rejected",
            "此对话已绑定到其他目标版本，不能重新绑定",
        ));
    }
    Ok(())
}

fn proof_error_response(error: crate::modules::task::proofloop::ProofError) -> ChatApiError {
    let status = if error.code.starts_with("proofloop_") {
        StatusCode::CONFLICT
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    chat_error(status, &error.code, error.message)
}

fn current_goal_evidence(
    db: &Database,
    goal_id: &str,
    goal_version: u32,
    graph_id: &str,
    graph_revision: u64,
) -> Result<(Option<String>, Vec<String>), ChatApiError> {
    let reports = db.list_acceptance_reports(goal_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "proofloop_report_read_failed",
            error.to_string(),
        )
    })?;
    let current = reports
        .into_iter()
        .filter(|report| {
            report.goal_version == goal_version
                && report.graph_id == graph_id
                && report.graph_revision == graph_revision
        })
        .max_by_key(|report| report.checked_at_ms);
    let Some(current) = current else {
        return Ok((None, Vec::new()));
    };
    let (persisted_report, snapshot) = db
        .get_acceptance_report(&current.report_id)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_report_read_failed",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_report_stale",
                "当前验收报告已不存在",
            )
        })?;
    if persisted_report.goal_id != goal_id
        || persisted_report.goal_version != goal_version
        || persisted_report.graph_id != graph_id
        || persisted_report.graph_revision != graph_revision
        || snapshot.goal_id != goal_id
        || snapshot.goal_version != goal_version
        || snapshot.graph_id != graph_id
        || snapshot.graph_revision != graph_revision
    {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_report_stale",
            "当前验收报告与已确认目标或任务图版本不一致",
        ));
    }

    let available = snapshot
        .evidence
        .iter()
        .map(|evidence| (evidence.evidence_id.as_str(), evidence))
        .collect::<std::collections::HashMap<_, _>>();
    let cited = persisted_report
        .results
        .iter()
        .flat_map(|result| result.evidence_ids.iter())
        .chain(
            persisted_report
                .deliverable_results
                .iter()
                .flat_map(|result| result.evidence_ids.iter()),
        );
    let mut seen = std::collections::HashSet::new();
    let mut evidence_ids = Vec::new();
    let mut stale_reference = false;
    for (index, evidence_id) in cited.enumerate() {
        if index >= 128 {
            stale_reference = true;
            break;
        }
        if !seen.insert(evidence_id.as_str()) {
            continue;
        }
        let Some(evidence) = available.get(evidence_id.as_str()) else {
            stale_reference = true;
            continue;
        };
        if evidence.origin.graph_revision != graph_revision {
            stale_reference = true;
            continue;
        }
        if is_current_successful_evidence(db, evidence)? {
            if evidence_ids.len() < 32 {
                evidence_ids.push(evidence_id.clone());
            }
        } else {
            stale_reference = true;
        }
    }
    let status = if stale_reference {
        "stale_evidence".to_string()
    } else {
        format!("{:?}", persisted_report.status)
    };
    Ok((Some(status), evidence_ids))
}

fn is_current_successful_evidence(
    db: &Database,
    evidence: &crate::modules::task::proofloop::EvidenceRef,
) -> Result<bool, ChatApiError> {
    let execution_ref = &evidence.origin;
    let graph_id =
        crate::modules::task::TaskGraphId::new(execution_ref.graph_id.clone()).map_err(|_| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_evidence_stale",
                "证据图标识无效",
            )
        })?;
    let node_id =
        crate::modules::task::TaskNodeId::new(execution_ref.node_id.clone()).map_err(|_| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_evidence_stale",
                "证据节点标识无效",
            )
        })?;
    let execution_id = crate::modules::task::NodeExecutionId::new(
        execution_ref.execution_id.clone(),
    )
    .map_err(|_| {
        chat_error(
            StatusCode::CONFLICT,
            "proofloop_evidence_stale",
            "证据执行标识无效",
        )
    })?;
    let Some(execution) = db.load_node_execution(&execution_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "proofloop_evidence_read_failed",
            error.to_string(),
        )
    })?
    else {
        return Ok(false);
    };
    let latest = db
        .load_node_executions(&graph_id, &node_id)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_evidence_read_failed",
                error.to_string(),
            )
        })?
        .into_iter()
        .max_by_key(|item| item.attempt);
    Ok(execution.graph_id == graph_id
        && execution.node_id == node_id
        && execution_ref.graph_revision > 0
        && execution.status == crate::modules::task::NodeExecutionStatus::Succeeded
        && execution.validation.as_ref().is_some_and(|validation| {
            validation.status == crate::modules::task::ValidationStatus::Accepted
        })
        && latest.is_some_and(|item| item.id == execution_id))
}

fn resolve_goal_chat_context(
    server: &AppServer,
    db: &Database,
    reference: &ChatGoalRef,
    cancel: CancellationToken,
) -> Result<GoalBoundChatContext, ChatApiError> {
    if reference.goal_id.trim().is_empty()
        || reference.goal_id.as_str() != reference.goal_id.trim()
        || reference.goal_id.len() > 200
        || reference.goal_id.chars().any(char::is_control)
        || reference.goal_version == 0
    {
        return Err(chat_error(
            StatusCode::BAD_REQUEST,
            "proofloop_goal_ref_invalid",
            "goal_ref must contain a non-empty goal id and a positive version",
        ));
    }
    let contract = db
        .load_goal_version(&reference.goal_id, reference.goal_version)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_goal_read_failed",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_goal_not_confirmed",
                "指定目标版本不存在或尚未确认",
            )
        })?;
    let draft = db
        .confirmed_proof_draft_for_goal(&reference.goal_id, reference.goal_version)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_goal_read_failed",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_goal_not_confirmed",
                "指定目标没有已确认的计划",
            )
        })?;
    let Some(confirmed_revision) = draft.confirmed_graph_revision else {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_goal_not_confirmed",
            "指定目标没有已确认的任务图版本",
        ));
    };
    if draft.status != crate::modules::task::proofloop::DraftStatus::Confirmed
        || draft.contract != contract
        || contract.goal_id != reference.goal_id
        || contract.version != reference.goal_version
    {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_goal_stale",
            "目标草稿与已确认的目标版本不一致",
        ));
    }
    let graph_id =
        crate::modules::task::TaskGraphId::new(draft.graph_id.clone()).map_err(|error| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_goal_graph_invalid",
                error.to_string(),
            )
        })?;
    let snapshot = db
        .load_task_supervisor_snapshot(&graph_id)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_goal_graph_read_failed",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_goal_graph_stale",
                "已确认的任务图不存在",
            )
        })?;
    let current_revision = snapshot.supervisor.graph_revision().value();
    if current_revision != confirmed_revision {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_goal_graph_stale",
            "任务图已变化，必须先重新确认目标",
        ));
    }
    let current_draft = db
        .confirmed_proof_draft_for_graph(&draft.graph_id)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "proofloop_goal_read_failed",
                error.to_string(),
            )
        })?
        .ok_or_else(|| {
            chat_error(
                StatusCode::CONFLICT,
                "proofloop_goal_not_current",
                "任务图没有当前已确认目标",
            )
        })?;
    if current_draft.contract.goal_id != reference.goal_id
        || current_draft.contract.version != reference.goal_version
        || current_draft.confirmed_graph_revision != Some(current_revision)
    {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_goal_not_current",
            "指定目标版本不是当前任务图已确认的目标",
        ));
    }

    let task_lease = server
        .task_world
        .register_goal_chat(&graph_id, cancel)
        .map_err(|error| {
            let code = match &error {
                crate::modules::task::TaskWorldRuntimeError::ExecutionCancelled(_) => {
                    "task_cancelled"
                }
                crate::modules::task::TaskWorldRuntimeError::ExecutionPaused(_) => "task_paused",
                _ => "proofloop_goal_control_unavailable",
            };
            chat_error(StatusCode::CONFLICT, code, error.to_string())
        })?;
    let (report_status, evidence_ids) = current_goal_evidence(
        db,
        &reference.goal_id,
        reference.goal_version,
        &draft.graph_id,
        confirmed_revision,
    )?;

    let now = chrono::Utc::now().timestamp_millis();
    let remaining_time = server
        .proofloop_budget
        .check(&contract, now)
        .map_err(proof_error_response)?;
    let mut reservation =
        GoalModelReservation::reserve(db, &draft, "goal-chat").map_err(proof_error_response)?;
    let budget = reservation.budget();
    let activity_lease = match server.proofloop_budget.begin_model_activity(&draft) {
        Ok(lease) => lease,
        Err(error) => {
            let finish = reservation.finish();
            let message = match finish {
                Ok(()) => error.to_string(),
                Err(finish_error) => format!(
                    "{}; goal reservation cleanup failed: {}",
                    error, finish_error
                ),
            };
            return Err(chat_error(
                StatusCode::CONFLICT,
                "proofloop_usage_unavailable",
                message,
            ));
        }
    };

    let deliverables = contract
        .deliverables
        .iter()
        .take(16)
        .map(|item| format!("- {} ({})", item.title, item.mime_type))
        .collect::<Vec<_>>()
        .join("\n");
    let criteria = contract
        .criteria
        .iter()
        .take(24)
        .map(|item| {
            format!(
                "- {}{}: {}",
                if item.required { "[required] " } else { "" },
                item.id,
                item.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let evidence = if evidence_ids.is_empty() {
        "No evidence ids are available from a persisted current acceptance report.".to_string()
    } else {
        format!(
            "Evidence ids available from the persisted current report: {}",
            evidence_ids.join(", ")
        )
    };
    let report = report_status.unwrap_or_else(|| "unavailable".into());
    let goal_summary = truncate_chars(
        &format!(
            "Validated ProofLoop goal reference: {} version {}.\nObjective: {}\nDeliverables:\n{}\nAcceptance criteria:\n{}\nCurrent persisted report status: {}.\n{}\nUse only the listed evidence ids for claims tied to this goal; do not claim acceptance beyond the persisted report.",
            contract.goal_id, contract.version, contract.user_goal, deliverables, criteria, report, evidence
        ),
        6000,
    );
    Ok(GoalBoundChatContext {
        goal_id: contract.goal_id.clone(),
        goal_version: contract.version,
        goal_summary,
        evidence_ids,
        deadline: tokio::time::Instant::now() + remaining_time,
        budget,
        reservation,
        activity_lease,
        task_lease,
    })
}

pub(crate) fn resolve_goal_chat_run(
    server: &AppServer,
    db: &Database,
    reference: &ChatGoalRef,
    cancel: CancellationToken,
) -> Result<GoalChatRun, ChatApiError> {
    let context = resolve_goal_chat_context(server, db, reference, cancel.clone())?;
    Ok(GoalChatRun {
        scope: crate::agent::delegation::ExecutionScope {
            owner: None,
            cancel,
            budget: Some(Arc::clone(&context.budget)),
            handoff_id: None,
            goal_id: Some(context.goal_id),
            goal_version: Some(context.goal_version),
            evidence_ids: context.evidence_ids,
            deadline: Some(context.deadline),
        },
        goal_ref: reference.clone(),
        goal_summary: context.goal_summary,
        reservation: context.reservation,
        activity_lease: context.activity_lease,
        _task_lease: context.task_lease,
    })
}

/// POST /api/chat — SSE streaming agent response
pub async fn chat_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<serde_json::Value>)>
{
    let request_received_at = chrono::Utc::now().timestamp_millis();
    let message = req.message.trim().to_string();
    let cli_provider = cli::Provider::from_profile(req.options.model_profile_id.as_deref());
    if let Some(provider) = cli_provider {
        if !crate::modules::onboarding::allowed(&server.db, "external_model") {
            return Err(chat_error(
                StatusCode::FORBIDDEN,
                "external_model_disabled",
                "当前资料档案已关闭外部模型调用",
            ));
        }
        if !provider.installed() {
            return Err(chat_error(
                StatusCode::BAD_REQUEST,
                "cli_provider_not_installed",
                "所选 CLI 模型尚未安装或配置",
            ));
        }
        if !req.resource_ids.is_empty()
            || req.workflow_id.is_some()
            || req.goal_ref.is_some()
            || req.options.agent_id.is_some()
        {
            return Err(chat_error(
                StatusCode::BAD_REQUEST,
                "cli_text_only",
                "CLI 对话仅接受文本，请移除附件、目标、工作流和指定智能体",
            ));
        }
    }
    if message.is_empty() && req.resource_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "消息内容不能为空"})),
        ));
    }

    let db = server.db.clone_connection();
    let requested_conversation_id = req
        .conversation_id
        .as_ref()
        .filter(|id| !id.is_empty())
        .cloned();
    // Resolve the immutable goal scope while holding the same turn gate used
    // by explicit binding, before any asynchronous runtime preparation.
    let cancel_token = CancellationToken::new();
    let requested_turn_guard = match requested_conversation_id.as_deref() {
        Some(id) => {
            let guard = lifecycle::acquire_registered_turn(&server, id, &cancel_token).await;
            if db
                .conversation_run_status(id)
                .map_err(|error| {
                    chat_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "conversation_read_failed",
                        error,
                    )
                })?
                .is_none()
            {
                return Err(chat_error(
                    StatusCode::NOT_FOUND,
                    "conversation_not_found",
                    "会话已删除或不存在",
                ));
            }
            Some(guard)
        }
        None => None,
    };
    let persisted_goal_ref = match requested_conversation_id.as_deref() {
        Some(conversation_id) => load_conversation_goal_ref(&db, conversation_id)?,
        None => None,
    };
    let conversation_goal_ref =
        resolve_conversation_goal_ref(req.goal_ref.as_ref(), persisted_goal_ref)?;
    if let Some(id) = requested_conversation_id.as_deref() {
        if crate::modules::automation::personal::is_delegated(&server, id).map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "personal_scope_failed",
                error,
            )
        })? && (conversation_goal_ref.is_some() || req.workflow_id.is_some())
        {
            return Err(chat_error(
                StatusCode::CONFLICT,
                "personal_delegation_scope",
                "个人委托不能使用其他目标或工作流替换范围与预算，请从委托详情管理",
            ));
        }
    }
    let channel_context = requested_conversation_id
        .as_deref()
        .map(|id| db.channel_for_conversation(id))
        .transpose()
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "channel_context_failed",
                error,
            )
        })?
        .flatten()
        .is_some();
    if channel_context && (conversation_goal_ref.is_some() || req.workflow_id.is_some()) {
        return Err(chat_error(
            StatusCode::BAD_REQUEST,
            "channel_scope_invalid",
            "渠道会话仅处理当前私聊文本",
        ));
    }
    let goal_run = conversation_goal_ref
        .as_ref()
        .map(|goal_ref| resolve_goal_chat_run(&server, &db, goal_ref, cancel_token.clone()))
        .transpose()?;
    let goal_bound = goal_run.is_some();

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(256);
    let (handoff, mut stream_rx) =
        lifecycle::ChatTurnEvents::start(db.clone_connection(), event_rx);

    let conv_id = match requested_conversation_id.clone() {
        Some(id) => id,
        None => {
            db.create_conversation_with_workspace_default("新对话", conversation_goal_ref.is_none())
                .map_err(|error| {
                    chat_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "conversation_create_failed",
                        error,
                    )
                })?
                .id
        }
    };
    let turn_guard = match requested_turn_guard {
        Some(guard) => guard,
        None => lifecycle::acquire_registered_turn(&server, &conv_id, &cancel_token).await,
    };
    if server.cloud_revocation_retry_cancel.is_cancelled() {
        return Err(chat_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "app_shutting_down",
            "应用正在退出",
        ));
    }
    if !server
        .approval_store
        .list_pending_for_conversation(&conv_id)
        .is_empty()
    {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "approval_pending",
            "请先处理当前审批，或回退当前用户消息",
        ));
    }
    if !db
        .pending_conversation_inputs(&conv_id)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "question_read_failed",
                error,
            )
        })?
        .is_empty()
    {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "user_input_pending",
            "请先回答当前问题，或停止当前轮次",
        ));
    }

    if let Some(issue) = db.conversation_protocol_issue(&conv_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "chat_history_read_failed",
            error,
        )
    })? {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code":"chat_history_protocol_invalid","error":issue.error,
                "affected_user_message_id":issue.affected_user_message_id
            })),
        ));
    }
    let lane = db.conversation_lane(&conv_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "conversation_lane_read_failed",
            error.to_string(),
        )
    })?;
    let mode = if lane.as_ref().is_some_and(|(_, kind)| kind == "chat") {
        crate::db::conversation_modes::WorkspaceMode::Chat
    } else {
        req.options
            .workspace_mode
            .unwrap_or(db.conversation_mode(&conv_id).map_err(|error| {
                chat_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "conversation_mode_read_failed",
                    error,
                )
            })?)
    };
    let previous_user = db.last_user_message(&conv_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "chat_history_invalid",
            error,
        )
    })?;
    let ordinary = runtime::ordinary_conversation(&server, &conv_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "conversation_scope_failed",
            error,
        )
    })?;
    if cli_provider.is_some() && (!ordinary || goal_bound) {
        return Err(chat_error(
            StatusCode::BAD_REQUEST,
            "cli_scope_invalid",
            "请在普通聊天会话中使用 CLI 模型",
        ));
    }
    let mut route = ordinary.then(|| {
        let names: Vec<String> = server
            .software_runtime
            .chat_sessions(&conv_id)
            .unwrap_or_default()
            .into_iter()
            .flat_map(|(r, _)| {
                let stem = std::path::Path::new(&r.config.executable)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_owned);
                std::iter::once(r.config.name).chain(stem)
            })
            .collect();
        crate::interaction::chat_understanding::understand_with_software(
            &message,
            previous_user.as_ref().map(|(_, text)| text.as_str()),
            !req.resource_ids.is_empty(),
            goal_bound,
            mode,
            &names,
        )
    });
    let selected_agent = ordinary
        .then(|| {
            req.options
                .agent_id
                .clone()
                .or_else(|| route.as_ref().map(|route| route.role(mode).agent_id()))
        })
        .flatten();
    let mut runtime = tokio::select! {
        biased;
        _ = cancel_token.cancelled() => return Err(preparation_cancelled()),
        runtime = runtime::prepare_for_turn(
            &server,
            Some(&conv_id),
            Some(&event_tx),
            req.options.model_profile_id.as_deref().filter(|_| cli_provider.is_none()),
            selected_agent.as_deref(),
        ) => runtime,
    }
    .map_err(|error| chat_error(StatusCode::BAD_REQUEST, "connector_runtime_invalid", error))?;
    if cli_provider.is_some() {
        runtime.model_pool_id = None;
        runtime.model_row = None;
    } else {
        req.options
            .apply(&server, &mut runtime)
            .map_err(|error| chat_error(StatusCode::BAD_REQUEST, "chat_options_invalid", error))?;
    }
    // Finish fallible model/pool setup before accepting the user message.
    let llm_client = runtime::client(&server, &runtime, &conv_id, &event_tx)
        .map_err(|error| chat_error(StatusCode::BAD_REQUEST, "connector_model_invalid", error))?;
    let llm_client = req.options.client(llm_client);
    let mut turn_options = req.options.clone();
    turn_options.workspace_mode = Some(mode);
    turn_options.agent_id = selected_agent;
    db.save_chat_options(&conv_id, &turn_options)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chat_options_save_failed",
                error,
            )
        })?;
    if req.options.workspace_mode.is_some() {
        db.set_conversation_mode(&conv_id, mode).map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "conversation_mode_save_failed",
                error,
            )
        })?;
    }
    if let Some(route) = route.as_mut() {
        use crate::interaction::chat_understanding::{Action, Source};
        for id in &req.resource_ids {
            if runtime
                .context_manifest
                .entries
                .iter()
                .any(|entry| entry.kind == "resource" && entry.source_id == *id)
            {
                route.sources.push(Source {
                    kind: "attachment",
                    id: id.clone(),
                });
            }
        }
        if route.actions.contains(&Action::Continue) || route.actions.contains(&Action::Correct) {
            if let Some((id, _)) = previous_user {
                route.sources.push(Source {
                    kind: "previous_user_message",
                    id,
                });
            }
        }
        if let Some(reference) = &conversation_goal_ref {
            route.sources.push(Source {
                kind: "goal",
                id: reference.goal_id.clone(),
            });
        }
        understanding::apply(&mut runtime, route);
    }
    let config = runtime.config.clone();
    let tool_registry = runtime.tools.clone();
    tokio::select! {
        biased;
        _ = cancel_token.cancelled() => return Err(preparation_cancelled()),
        _ = event_tx.send(crate::api::conversation_modes::event(
            &conv_id,
            mode,
            false,
            "当前会话模式",
        )) => {},
    }
    let mut system_prompt = format!("{}\n\n{}", runtime.prompt, mode.prompt());

    // Merge workflow context if workflow_id is provided
    let workflow_prompt = if channel_context {
        None
    } else if let Some(ref wf_id) = req.workflow_id {
        if let Ok(Some(wf)) = db.get_workflow(wf_id) {
            let mut extra = String::new();
            extra.push_str("\n\n## 当前工作流\n\n");
            extra.push_str(&format!("**名称**: {}\n", wf.name));
            if !wf.description.is_empty() {
                extra.push_str(&format!("**描述**: {}\n", wf.description));
            }
            if !wf.nodes.is_empty() {
                extra.push_str(&format!("**流程**: {}\n", wf.nodes.join(" → ")));
            }
            if !wf.system_prompt_extra.is_empty() {
                extra.push_str(&format!("\n{}\n", wf.system_prompt_extra));
            }
            extra.push_str("\n请按照以上工作流结构执行任务。");
            Some(extra)
        } else {
            None
        }
    } else {
        // Use active workflow if no explicit workflow_id
        let active_wf = db
            .get_active_workflow_id()
            .ok()
            .flatten()
            .and_then(|aid| db.get_workflow(&aid).ok().flatten());
        active_wf.map(|wf| {
            let mut extra = String::new();
            extra.push_str("\n\n## 当前激活工作流\n\n");
            extra.push_str(&format!("**名称**: {}\n", wf.name));
            if !wf.description.is_empty() {
                extra.push_str(&format!("**描述**: {}\n", wf.description));
            }
            if !wf.nodes.is_empty() {
                extra.push_str(&format!("**流程**: {}\n", wf.nodes.join(" → ")));
            }
            if !wf.system_prompt_extra.is_empty() {
                extra.push_str(&format!("\n{}\n", wf.system_prompt_extra));
            }
            extra.push_str("\n请按照以上工作流结构执行任务。");
            extra
        })
    };

    if let Some(extra) = workflow_prompt {
        system_prompt.push_str(&extra);
    }

    use crate::agent::memory::context::chat_memory_query;
    let history = db.load_chat_messages(&conv_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "chat_history_invalid",
            error,
        )
    })?;
    let mut context_manifest = runtime.context_manifest.clone();
    if let Some(provider) = cli_provider {
        context_manifest.record(
            "external_model",
            provider.profile(),
            None,
            crate::execution::context_manifest::ContextStatus::Resolved,
            0,
            Some("Text-only provider using the local conversation history"),
        );
    }
    lanes::append_parent_context(&db, &conv_id, &mut system_prompt, &mut context_manifest)?;
    let recall_reply = if !goal_bound
        && !channel_context
        && recall::enabled(&runtime)
        && crate::modules::onboarding::allowed(&db, "memory")
    {
        recall::prepare(
            &db,
            &conv_id,
            &message,
            &history,
            &mut system_prompt,
            &mut context_manifest,
        )
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not recall an earlier chat");
            Some("暂时无法读取旧对话，请稍后再试。".into())
        })
    } else {
        None
    };
    if let Some(route) = route.as_mut() {
        if cli_provider.is_none()
            && recall_reply.is_none()
            && !goal_bound
            && req.workflow_id.is_none()
            && req.resource_ids.is_empty()
        {
            understanding::refine(route, mode, &llm_client, &runtime, &message, &cancel_token)
                .await?;
        }
        let _ = event_tx
            .send(understanding::event(
                &conv_id,
                route,
                runtime.agent_id.as_deref(),
            ))
            .await;
    }
    let memory_resolved_at = if recall_reply.is_none() {
        let memory_manifest = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => return Err(preparation_cancelled()),
            manifest = grounding::append_memory(
                &server,
                &runtime,
                &conv_id,
                &mut system_prompt,
                chat_memory_query(&message, &history),
                !goal_bound && !channel_context,
            ) => manifest,
        };
        context_manifest.extend(memory_manifest);
        let memory_resolved_at = chrono::Utc::now().timestamp_millis();
        let knowledge_manifest = grounding::append_with_manifest(
            &db,
            &mut system_prompt,
            &message,
            runtime.bindings.restrict_knowledge,
        )
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "knowledge_retrieval_failed",
                error,
            )
        })?;
        context_manifest.extend(knowledge_manifest);
        memory_resolved_at
    } else {
        chrono::Utc::now().timestamp_millis()
    };

    if let Some(goal_ref) = conversation_goal_ref.as_ref() {
        persist_conversation_goal_ref(&db, &conv_id, goal_ref)?;
    }

    // Build agent state from history before persisting the current user message.
    let mut agent_state = AgentState::new(system_prompt);
    agent_state.goal_ref = conversation_goal_ref;
    agent_state.load_history(history);
    agent_state.compaction_checkpoint =
        db.load_conversation_compaction(&conv_id).map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "context_checkpoint_read_failed",
                error,
            )
        })?;
    let prev_msg_count = agent_state.messages.len();

    // Persist the current user message after taking the history snapshot.
    if cancel_token.is_cancelled() {
        return Err(preparation_cancelled());
    }
    let now = chrono::Utc::now().timestamp_millis();
    let user_message_id = uuid::Uuid::new_v4().to_string();
    let accepted_turn_id = db
        .accept_conversation_turn_with_resources(
            &MessageRow {
                id: user_message_id.clone(),
                conversation_id: conv_id.clone(),
                role: "user".to_string(),
                content: message.clone(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_result: None,
                created_at: now,
            },
            &req.resource_ids,
        )
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "conversation_turn_write_failed",
                error,
            )
        })?;

    if let Err(error) = crate::execution::context_manifest::save_conversation_manifest(
        &db,
        &conv_id,
        &accepted_turn_id,
        &context_manifest,
        Some(
            crate::execution::context_manifest::ConversationContextTimeline::new(
                request_received_at,
                memory_resolved_at,
                &context_manifest,
            ),
        ),
    ) {
        tracing::warn!(%error, "accepted turn context metadata could not be persisted");
    }

    // Auto-title: use first user message (trim to 40 chars)
    let title = if message.is_empty() {
        "附件对话".to_string()
    } else {
        truncate_chars(&message, 40)
    };
    let _ = db.update_conversation_title(&conv_id, &title);

    if let Some(run) = goal_run.as_ref() {
        agent_state.add_user_message(run.goal_summary.clone());
    }

    // Add the current user message to the LLM context exactly once.
    agent_state.add_user_message(if message.is_empty() {
        "请查看本次附加的文件。图片请使用图像分析工具读取内容。".to_string()
    } else {
        message.clone()
    });
    agent_state.set_turn_user_start(agent_state.messages.len() - 1);
    use crate::execution::chat_context_cache::{ChatContextObservation, CURRENT_CHAT_CONTEXT};
    let model_id = cli_provider.map_or_else(
        || runtime.context_model_id(),
        |provider| provider.profile().into(),
    );
    let context_observation = ChatContextObservation::new(
        &server,
        &conv_id,
        &accepted_turn_id,
        &user_message_id,
        model_id,
        &config,
        &context_manifest,
    )
    .map_err(|error| tracing::warn!(%error, "chat context observation unavailable"))
    .ok();

    // Spawn agent loop
    let conv_clone = conv_id.clone();
    let config_clone = config.clone();
    let log_buffer = server.log_buffer.clone();

    let db_clone = db.clone();
    let bound_agent = runtime.agent_id.clone();
    let chat_turn_model = crate::execution::context::ChatTurnModel {
        conversation_id: conv_id.clone(),
        turn_id: accepted_turn_id.clone(),
        profile_id: turn_options.model_profile_id.clone(),
    };
    let steering = (ordinary && recall_reply.is_none() && cli_provider.is_none())
        .then(|| steering::register(&server, &conv_id, &accepted_turn_id, cancel_token.clone()));
    let accepts_steering = steering.is_some();
    let run_turn_id = accepted_turn_id.clone();
    tokio::spawn(CURRENT_CHAT_CONTEXT.scope(context_observation, async move {
        let _turn_guard = turn_guard;
        let mut result = if let Some(reply) = recall_reply {
            if cancel_token.is_cancelled() {
                Err("已取消".into())
            } else {
                agent_state.add_assistant_message(Some(reply.clone()), None);
                Ok(engine::publish_reply_done(&conv_clone, &event_tx, reply).await)
            }
        } else if let Some(provider) = cli_provider {
            cli::run(
                provider,
                &server,
                &conv_clone,
                &run_turn_id,
                &mut agent_state,
                &cancel_token,
                &event_tx,
            )
            .await
        } else {
            log_buffer.push("info", "agent", "Agent 循环开始");
            let security_gateway = crate::safety::production_gateway::build(
                &server,
                &server.workspace_root,
                Arc::clone(&tool_registry),
                bound_agent,
            );
            let execution = engine::run_react_loop_with_channel(
                &mut agent_state,
                &llm_client,
                &tool_registry,
                &server.approval_store,
                &security_gateway,
                &config_clone,
                &conv_clone,
                &cancel_token,
                &event_tx,
                &log_buffer,
            );
            let execution = crate::execution::owner::CURRENT_EXECUTION_OWNER.scope(
                crate::execution::ExecutionOwner::Conversation {
                    conversation_id: conv_clone.clone(),
                },
                execution,
            );
            let execution = crate::execution::context::CURRENT_CHAT_TURN_MODEL
                .scope(Some(chat_turn_model), execution);
            let goal_scope = goal_run
                .as_ref()
                .map(|run| run.scope.clone())
                .or_else(|| runtime.execution_scope.clone());
            let run = async {
                match goal_scope {
                    Some(scope) => {
                        crate::agent::delegation::EXECUTION_SCOPE
                            .scope(scope, execution)
                            .await
                    }
                    None => execution.await,
                }
            };
            match &steering {
                Some(steering) => steering.run(run).await,
                None => run.await,
            }
        };
        if let Some(run) = goal_run {
            if let Err(error) = run.finish() {
                result = Err(error);
            }
        }

        if let Err(error) =
            db_clone.save_conversation_compaction(&conv_clone, &agent_state.compaction_checkpoint)
        {
            result = Err(error);
        }
        // Save new messages (only assistant + tool produced after the loaded history).
        let total_msgs = agent_state.messages.len();
        let new_start = prev_msg_count; // current user starts here and is filtered below
        let now = chrono::Utc::now().timestamp_millis();
        let new_start = new_start.min(total_msgs);
        if let Some(steering) = &steering {
            if let Err(error) =
                steering.persist(&db_clone, &conv_clone, &agent_state.messages, new_start)
            {
                result = Err(error);
            }
        } else {
            for msg in agent_state.messages[new_start..]
                .iter()
                .filter(|msg| msg.role != "user" && msg.role != "system")
            {
                if let Err(error) = db_clone.persist_chat_message(&conv_clone, msg, now) {
                    result = Err(error);
                    break;
                }
            }
        }

        match result {
            Err(ref e) => {
                let status = if cancel_token.is_cancelled() {
                    crate::db::ConversationRunStatus::Cancelled
                } else {
                    crate::db::ConversationRunStatus::Failed
                };
                let _ = handoff.set_status(&db_clone, &conv_clone, status, Some(e));
                log_buffer.push("error", "agent", &format!("Agent 错误: {}", e));
                let _ = event_tx
                    .send(AgentEvent {
                        event_type: "error".into(),
                        conversation_id: conv_clone.clone(),
                        error: Some(e.clone()),
                        token: None,
                        tool_call_id: None,
                        tool_name: None,
                        args: None,
                        result: None,
                        status: None,
                        message_id: None,
                        risk_level: None,
                        reason: None,
                        approval_id: None,
                        verification_success: None,
                        verification_reason: None,
                        configuration_source: None,
                        should_replan: None,
                    })
                    .await;
            }
            Ok(engine::RunOutcome::Done { .. }) => {
                let _ = handoff.set_status(
                    &db_clone,
                    &conv_clone,
                    crate::db::ConversationRunStatus::Completed,
                    None,
                );
                log_buffer.push("info", "agent", "Agent 完成");
            }
            Ok(engine::RunOutcome::WaitingInput {
                tool_call_id,
                request,
            }) => {
                crate::api::conversation_inputs::persist_waiting_input(
                    &db_clone,
                    &conv_clone,
                    &tool_call_id,
                    &request,
                    &event_tx,
                    Some(&handoff),
                )
                .await;
            }
            Ok(engine::RunOutcome::Paused { approval_id }) => {
                let _ = handoff.set_status(
                    &db_clone,
                    &conv_clone,
                    crate::db::ConversationRunStatus::WaitingApproval,
                    None,
                );
                log_buffer.push(
                    "warn",
                    "agent",
                    &format!("Agent 暂停，等待审批 {}", approval_id),
                );
                // The handoff publishes approval_required only after history and lifecycle events commit.
            }
        }
        handoff.finish(&server, &conv_clone).await;
    }));

    // Return SSE stream
    let conv_stream = conv_id.clone();
    let stream = async_stream::stream! {
        yield Ok(Event::default()
            .event("connected")
            .data(serde_json::json!({"conversation_id": conv_stream,"user_message_id":user_message_id,"turn_id":accepted_turn_id,"accepts_steering":accepts_steering}).to_string()));

        while let Some(event) = stream_rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().event("agent-event").data(json));
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
}

/// POST /api/chat/stop — cancel a running generation
pub async fn stop_handler(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let conv_id = body["conversation_id"].as_str().unwrap_or("");
    if conv_id.is_empty() {
        return Json(serde_json::json!({"status":"not_found"}));
    }
    match super::chat_stop::stop(server, conv_id).await {
        Ok(true) => Json(serde_json::json!({"status":"cancelled"})),
        Ok(false) => Json(serde_json::json!({"status":"not_found"})),
        Err(error) => Json(serde_json::json!({"status":"error","error":error})),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_conversation_goal_ref, ChatRequest};
    use std::{path::PathBuf, sync::Arc};

    use axum::{
        body::{to_bytes, Body},
        extract::State,
        http::{header::CONTENT_TYPE, Request, StatusCode},
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use serde_json::{json, Value};
    use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
    use tower::ServiceExt;

    use crate::{
        agent::engine::AgentEvent,
        api::build_router,
        app::state::AppServer,
        db::MessageRow,
        safety::{ControlSession, CONTROL_SESSION_HEADER},
    };

    type CapturedRequests = Arc<Mutex<Vec<Value>>>;

    struct TempDatabase(PathBuf);

    impl TempDatabase {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "yilian-chat-user-message-{}.db",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for TempDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    async fn capture_llm_request(
        State(requests): State<CapturedRequests>,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        requests.lock().await.push(body);
        (
            [(CONTENT_TYPE, "text/event-stream")],
            concat!(
                "data: {\"id\":\"test\",\"choices\":[{\"index\":0,",
                "\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            ),
        )
    }

    async fn start_mock_llm() -> (String, CapturedRequests, JoinHandle<()>) {
        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(capture_llm_request))
            .with_state(requests.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), requests, task)
    }

    fn test_server(database: &TempDatabase, base_url: &str) -> (Arc<AppServer>, String) {
        let token = "a".repeat(64);
        let server = Arc::new(
            AppServer::new_with_control_session(
                &database.0,
                ".",
                ControlSession::new(token.clone()).unwrap(),
            )
            .unwrap(),
        );
        {
            let mut config = server.config.write();
            config.model.base_url = base_url.to_string();
            config.model.api_key = "test-key".to_string();
            config.model.api_key_env.clear();
            config.model.invoke_timeout_ms = 5_000;
        }
        (server, token)
    }

    async fn send_chat(server: Arc<AppServer>, token: &str, body: Value) {
        let response = build_router(server)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header(CONTROL_SESSION_HEADER, token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    }

    #[tokio::test]
    async fn chat_context_receipt_is_scoped_and_rebuilt_from_canonical_history() {
        let (base_url, requests, provider) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let id = server.db.create_conversation("上下文验收").unwrap().id;
        let send = |conversation: String, message: &'static str| {
            send_chat(
                server.clone(),
                &token,
                json!({"conversation_id":conversation,"workspace_mode":"chat","message":message}),
            )
        };
        let snapshot = |conversation: &str| {
            crate::execution::context_manifest::load_conversation_manifest(&server.db, conversation)
                .unwrap()
                .unwrap()
        };
        send(id.clone(), "receipt-first-marker").await;
        let first = snapshot(&id).projection.unwrap();
        assert_eq!(first.conversation_id, id);
        assert!(first.branch_id.is_none());
        assert!(!first.static_budget_cache_hit);
        assert_eq!(first.content_digest.len(), 64);
        let turns = server.db.conversation_turns(&id).unwrap();
        assert_eq!(first.event_cursor, turns[0].user_message_id);

        send(id.clone(), "receipt-second-marker").await;
        let second = snapshot(&id).projection.unwrap();
        assert!(second.static_budget_cache_hit);
        assert_ne!(first.event_cursor, second.event_cursor);
        assert_ne!(first.content_digest, second.content_digest);

        let profile = build_router(server.clone())
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/profile")
                    .header(CONTROL_SESSION_HEADER, &token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"expected_revision":0,"permissions":{"memory":false}}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(profile.status(), StatusCode::OK);
        send(id.clone(), "receipt-after-permission-change").await;
        let updated = snapshot(&id).projection.unwrap();
        assert!(!updated.static_budget_cache_hit);
        assert_ne!(updated.source_revision, second.source_revision);

        server.config.write().model.name = "receipt-second-model".into();
        send(id.clone(), "receipt-after-model-change").await;
        let changed_model = snapshot(&id).projection.unwrap();
        assert!(!changed_model.static_budget_cache_hit);
        assert_ne!(changed_model.model_id, updated.model_id);
        assert_ne!(changed_model.source_revision, updated.source_revision);

        let old_reply = server.db.get_conversation(&id).unwrap().messages[1]
            .id
            .clone();
        let branch = server
            .db
            .fork_message_canvas_conversation(&id, &old_reply)
            .unwrap();
        send(branch.clone(), "receipt-branch-marker").await;
        let child = snapshot(&branch).projection.unwrap();
        assert_eq!(child.branch_id.as_deref(), Some(branch.as_str()));
        assert!(!child.static_budget_cache_hit);
        assert_ne!(child.source_revision, changed_model.source_revision);

        server
            .db
            .mutate_conversation_message(&id, &old_reply, false)
            .unwrap();
        assert!(snapshot(&id).projection.is_none());
        assert_eq!(
            snapshot(&branch).projection.unwrap().content_digest,
            child.content_digest
        );
        drop(server);
        let (restarted, restarted_token) = test_server(&database, &base_url);
        send_chat(
            restarted.clone(),
            &restarted_token,
            json!({"conversation_id":id,"workspace_mode":"chat","message":"receipt-after-restart-marker"}),
        ).await;
        let rebuilt =
            crate::execution::context_manifest::load_conversation_manifest(&restarted.db, &id)
                .unwrap()
                .unwrap()
                .projection
                .unwrap();
        assert!(!rebuilt.static_budget_cache_hit);
        let requests = requests.lock().await;
        let sent = requests.last().unwrap()["messages"].to_string();
        assert!(sent.contains("receipt-first-marker"));
        assert!(sent.contains("receipt-second-marker"));
        assert!(!sent.contains("receipt-branch-marker"));
        let receipt = serde_json::to_string(&rebuilt).unwrap();
        assert!(!receipt.contains("receipt-first-marker"));
        assert!(!receipt.contains("receipt-after-restart-marker"));
        provider.abort();
    }

    #[tokio::test]
    async fn interaction_auto_keeps_discussion_read_only_and_uses_work_tools_for_delivery() {
        let (base_url, requests, provider) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let id = server.db.create_conversation("互动路由").unwrap().id;
        for (mode, message) in [
            ("auto", "今天有点累，先别改代码，帮我想想记忆模块怎么做"),
            ("auto", "请把刚才的方案写入 notes.md"),
            ("chat", "请把内容写入 notes.md"),
            ("auto", "请记住，以后导出 CSV 用 UTF-8 编码"),
        ] {
            send_chat(
                server.clone(),
                &token,
                json!({
                    "conversation_id": id, "workspace_mode": mode, "message": message
                }),
            )
            .await;
        }
        let requests = requests.lock().await;
        let has_tool = |index: usize, name: &str| {
            requests[index]["tools"]
                .as_array()
                .is_some_and(|tools| tools.iter().any(|tool| tool["function"]["name"] == name))
        };
        assert_eq!(requests.len(), 4);
        assert!(!has_tool(0, "write_file"));
        assert!(!has_tool(0, "bash"));
        assert!(!has_tool(0, "save_memory"));
        assert!(has_tool(1, "write_file"));
        assert!(!has_tool(2, "write_file"));
        assert!(has_tool(3, "save_memory"));
        assert!(!has_tool(3, "write_file"));
        assert!(requests[0]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("通用工作"));
        assert!(requests[2]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("日常交流"));
        provider.abort();
    }

    #[tokio::test]
    async fn interaction_task_status_uses_persisted_turn_without_a_model_request() {
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, "http://127.0.0.1:1");
        let id = server.db.create_conversation("运行中").unwrap().id;
        let message = message_row(&id, "user", "原任务", 1);
        let original = server.db.accept_conversation_turn(&message).unwrap();
        let app = build_router(server.clone());
        for (message, reply) in [
            ("任务状态", "本轮仍在结算执行状态。"),
            ("取消当前任务", "已向当前轮次发出取消请求。"),
            ("任务状态", "上一轮已取消。"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/chat/task-command")
                        .header(CONTROL_SESSION_HEADER, &token)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({"conversation_id":id,"message":message}).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), 1024).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap()["message"],
                reply
            );
        }
        let turns = server.db.conversation_turns(&id).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, original);
        assert_eq!(turns[0].status, "cancelled");
    }

    fn message_row(
        conversation_id: &str,
        role: &str,
        content: &str,
        created_at: i64,
    ) -> MessageRow {
        MessageRow {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at,
        }
    }

    fn current_user_message_count(request: &Value, content: &str) -> usize {
        request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "user" && message["content"] == content)
            .count()
    }

    #[tokio::test]
    async fn new_chat_can_continue_an_unsaved_plan_and_clarifies_ambiguous_sources() {
        let (base_url, requests, mock) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let workspace = crate::workspace::Workspace::new(
            crate::workspace::WorkspaceId::generate(),
            "测试项目".into(),
            String::new(),
            None,
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        let in_workspace = |title: &str| {
            let chat = server.db.create_conversation(title).unwrap();
            server
                .db
                .set_conversation_workspace(&chat.id, Some(workspace.id.as_str()))
                .unwrap();
            chat
        };
        let seed = |chat: &crate::db::ConversationSummary, answer: &str| {
            for (role, text, time) in [
                ("user", chat.title.as_str(), 10),
                ("tool", "INTERNAL_TOOL_TRACE", 20),
                ("assistant", answer, 30),
            ] {
                server
                    .db
                    .add_message(&message_row(&chat.id, role, text, time))
                    .unwrap();
            }
        };
        let first = in_workspace("星桥发布方案");
        seed(&first, "星桥发布方案已确认，第一步先发布到灰度环境。");
        let second = in_workspace("松涛迁移方案");
        seed(&second, "松涛迁移方案先导出旧库快照。");
        let foreign = server.db.create_conversation("星桥发布方案").unwrap();
        seed(&foreign, "OTHER_WORKSPACE_PLAN");

        let current = in_workspace("新对话");
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":current.id,"message":"接着上次的方案做"}),
        )
        .await;
        assert!(
            requests.lock().await.is_empty(),
            "歧义时由本地询问，避免让模型先执行"
        );
        let clarification = server
            .db
            .get_conversation(&current.id)
            .unwrap()
            .messages
            .pop()
            .unwrap();
        assert_eq!(clarification.role, "assistant");
        assert!(
            clarification.content.contains("星桥发布方案")
                && clarification.content.contains("松涛迁移方案")
        );
        let chosen = clarification
            .content
            .lines()
            .find(|line| line.contains("星桥发布方案"))
            .unwrap();
        let number = chosen.trim().split_once('.').unwrap().0;
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":current.id,"message":"第9条"}),
        )
        .await;
        assert!(requests.lock().await.is_empty());
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":current.id,"message":format!("第{number}条")}),
        )
        .await;
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":current.id,"message":"继续"}),
        )
        .await;

        let named = in_workspace("新对话");
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":named.id,"message":"接着上次的星桥发布方案做"}),
        )
        .await;
        let missing = in_workspace("新对话");
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":missing.id,"message":"接着上次的火星采样方案做"}),
        )
        .await;
        assert!(server
            .db
            .get_conversation(&missing.id)
            .unwrap()
            .messages
            .last()
            .unwrap()
            .content
            .contains("没有找到"));
        send_chat(
            server.clone(),
            &token,
            json!({"message":"接着上次的方案做"}),
        )
        .await;
        mock.abort();
        let captured = requests.lock().await;
        assert_eq!(captured.len(), 4);
        for request in &captured[..3] {
            let prompt = request["messages"][0]["content"].as_str().unwrap();
            assert!(prompt.contains("第一步先发布到灰度环境"));
            assert!(!prompt.contains("先导出旧库快照"));
            assert!(
                !prompt.contains("OTHER_WORKSPACE_PLAN") && !prompt.contains("INTERNAL_TOOL_TRACE")
            );
        }
        let unbound_prompt = captured[3]["messages"][0]["content"].as_str().unwrap();
        assert!(unbound_prompt.contains("OTHER_WORKSPACE_PLAN"));
        assert!(!unbound_prompt.contains("第一步先发布到灰度环境"));
        let snapshot =
            crate::execution::context_manifest::load_conversation_manifest(&server.db, &named.id)
                .unwrap()
                .unwrap();
        assert!(snapshot
            .manifest
            .entries
            .iter()
            .any(|entry| entry.kind == "prior_chat" && entry.source_id == first.id));
    }

    #[tokio::test]
    async fn new_desktop_chat_uses_the_explicit_default_workspace() {
        async fn put_default(server: Arc<AppServer>, token: &str, workspace: &str) -> StatusCode {
            build_router(server)
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/workspaces/default")
                        .header(CONTROL_SESSION_HEADER, token)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(json!({"workspace_id":workspace}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        }
        let (base_url, _, mock) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let workspace = crate::workspace::Workspace::new(
            crate::workspace::WorkspaceId::generate(),
            "默认项目".into(),
            String::new(),
            None,
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        let existing = server.db.create_conversation("以前的对话").unwrap();
        assert_eq!(
            put_default(server.clone(), &token, workspace.id.as_str()).await,
            StatusCode::OK
        );
        let response = build_router(server.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/conversations")
                    .header(CONTROL_SESSION_HEADER, &token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"title":"桌面新对话","use_default_workspace":true}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let created: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        let id = created["id"].as_str().unwrap();
        assert_eq!(
            server.db.conversation_workspace(id).unwrap().as_deref(),
            Some(workspace.id.as_str())
        );
        assert!(server
            .db
            .conversation_workspace(&existing.id)
            .unwrap()
            .is_none());
        send_chat(
            server.clone(),
            &token,
            json!({"message":"测试直接创建的对话"}),
        )
        .await;
        let created = server
            .db
            .list_conversations()
            .unwrap()
            .into_iter()
            .find(|conversation| conversation.title == "测试直接创建的对话")
            .unwrap();
        assert_eq!(
            server
                .db
                .conversation_workspace(&created.id)
                .unwrap()
                .as_deref(),
            Some(workspace.id.as_str())
        );
        server.db.archive_workspace(&workspace.id, 2, None).unwrap();
        assert_eq!(
            put_default(server.clone(), &token, workspace.id.as_str()).await,
            StatusCode::BAD_REQUEST
        );
        send_chat(server.clone(), &token, json!({"message":"归档后的新对话"})).await;
        let created = server
            .db
            .list_conversations()
            .unwrap()
            .into_iter()
            .find(|conversation| conversation.title == "归档后的新对话")
            .unwrap();
        assert!(server
            .db
            .conversation_workspace(&created.id)
            .unwrap()
            .is_none());
        mock.abort();
    }

    #[tokio::test]
    async fn chat_memory_respects_conversation_workspace_and_profile_scopes() {
        async fn memory_model(
            State(requests): State<CapturedRequests>,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            let last = body["messages"].as_array().unwrap().last().unwrap();
            let save = match last["content"].as_str().filter(|_| last["role"] == "user") {
                Some("只在本对话记住 GBK 编码") => {
                    Some(("conversation", "本对话编码为 GBK"))
                }
                Some("记住这个工作空间使用 UTF-16 编码") => {
                    Some(("workspace", "项目编码为 UTF-16"))
                }
                Some("以后所有对话记住 UTF-8 编码偏好") => {
                    Some(("profile", "默认编码为 UTF-8"))
                }
                _ => None,
            };
            let (delta, finish) = if let Some((scope, content)) = save {
                let arguments = json!({"content":content,"category":"preference","scope":scope});
                (
                    json!({"tool_calls":[{"index":0,"id":format!("save-{scope}"),"type":"function",
                    "function":{"name":"save_memory","arguments":arguments.to_string()}}]}),
                    "tool_calls",
                )
            } else {
                (json!({"content":"好的。"}), "stop")
            };
            requests.lock().await.push(body);
            (
                [(CONTENT_TYPE, "text/event-stream")],
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({"id":"scoped-memory","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
                ),
            )
        }

        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(memory_model))
            .with_state(requests.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mock = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &format!("http://{address}"));
        let workspace = crate::workspace::Workspace::new(
            crate::workspace::WorkspaceId::generate(),
            "编码项目".into(),
            String::new(),
            None,
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        let origin = server.db.create_conversation("原对话").unwrap();
        let sibling = server.db.create_conversation("同项目新对话").unwrap();
        let outside = server.db.create_conversation("其他对话").unwrap();
        for conversation in [&origin, &sibling] {
            server
                .db
                .set_conversation_workspace(&conversation.id, Some(workspace.id.as_str()))
                .unwrap();
        }
        for message in [
            "只在本对话记住 GBK 编码",
            "记住这个工作空间使用 UTF-16 编码",
            "以后所有对话记住 UTF-8 编码偏好",
        ] {
            send_chat(
                server.clone(),
                &token,
                json!({"conversation_id":origin.id,"message":message}),
            )
            .await;
        }
        let scope_of = |content: &str| {
            let memories = server
                .db
                .list_memories(&crate::db::MemoryQuery {
                    category: None,
                    source: None,
                    q: Some(content.into()),
                    limit: None,
                    offset: None,
                })
                .unwrap();
            assert_eq!(memories.len(), 1);
            let record = server.db.memory_governance(&memories[0].id).unwrap();
            assert_eq!(record.revision, 1);
            assert_eq!(
                server.db.memory_history(&memories[0].id).unwrap()[0].scope,
                record.scope
            );
            record.scope
        };
        assert_eq!(
            scope_of("本对话编码为 GBK"),
            format!("conversation:{}", origin.id)
        );
        assert_eq!(
            scope_of("项目编码为 UTF-16"),
            format!("workspace:{}", workspace.id.as_str())
        );
        assert_eq!(scope_of("默认编码为 UTF-8"), "profile");

        for (conversation, message) in [
            (&origin, "说个笑话"),
            (&sibling, "编码怎么选"),
            (&outside, "编码怎么选"),
            (&sibling, "按刚才的来"),
        ] {
            send_chat(
                server.clone(),
                &token,
                json!({"conversation_id":conversation.id,"message":message}),
            )
            .await;
        }
        mock.abort();
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 10);
        let prompt = |index: usize| requests[index]["messages"][0]["content"].as_str().unwrap();
        assert!(prompt(6).contains("本对话编码为 GBK"));
        assert!(prompt(7).contains("项目编码为 UTF-16") && prompt(7).contains("默认编码为 UTF-8"));
        assert!(!prompt(7).contains("本对话编码为 GBK"));
        assert!(prompt(8).contains("默认编码为 UTF-8"));
        assert!(
            !prompt(8).contains("项目编码为 UTF-16") && !prompt(8).contains("本对话编码为 GBK")
        );
        assert!(prompt(9).contains("项目编码为 UTF-16") && prompt(9).contains("默认编码为 UTF-8"));
        assert!(prompt(6).contains("[preference · 本对话]"));
        assert!(prompt(7).contains("[preference · 本工作空间]"));
        assert!(prompt(8).contains("[preference · 当前档案]"));
    }

    #[tokio::test]
    async fn chat_corrects_and_withdraws_the_original_memory() {
        async fn memory_model(
            State((requests, id)): State<(CapturedRequests, String)>,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            let mut requests = requests.lock().await;
            let index = requests.len();
            let update = match index {
                0 => Some(("correct", 1, Some("导出 CSV 使用 UTF-8 编码"))),
                3 => Some(("withdraw", 2, None)),
                _ => None,
            };
            let prompt = body["messages"][0]["content"].as_str().unwrap_or("");
            let offered = body["tools"].as_array().is_some_and(|tools| {
                tools
                    .iter()
                    .any(|tool| tool["function"]["name"] == "update_memory")
            });
            let (delta, finish) = match update {
                Some((action, revision, content))
                    if offered
                        && prompt.contains(&format!("memory_id={id}; revision={revision}")) =>
                {
                    let mut arguments =
                        json!({"action":action, "memory_id":id, "expected_revision":revision});
                    if let Some(content) = content {
                        arguments["content"] = json!(content);
                    }
                    (
                        json!({"tool_calls":[{"index":0,"id":format!("memory-edit-{index}"),"type":"function",
                        "function":{"name":"update_memory","arguments":arguments.to_string()}}]}),
                        "tool_calls",
                    )
                }
                _ if index == 2 => (
                    json!({"content":"你指 CSV 导出的编码，还是日志编码？"}),
                    "stop",
                ),
                _ => (json!({"content":"记忆操作结束。"}), "stop"),
            };
            requests.push(body);
            (
                [(CONTENT_TYPE, "text/event-stream")],
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({"id":"memory-edit","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
                ),
            )
        }

        let database = TempDatabase::new();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (server, token) = test_server(&database, &format!("http://{address}"));
        let conversation = server.db.create_conversation("记忆管理").unwrap();
        let seed = |content: &str| {
            server
                .db
                .create_memory(&crate::db::CreateMemoryRequest {
                    content: content.into(),
                    category: "preference".into(),
                    source: "manual".into(),
                    source_conversation_id: None,
                    metadata: None,
                })
                .unwrap()
        };
        let memory = seed("导出 CSV 使用 GBK 编码");
        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(memory_model))
            .with_state((requests.clone(), memory.id.clone()));
        let mock = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,
            "message":"之前记的编码改成 UTF-8"}),
        )
        .await;
        assert_eq!(
            server.db.get_memory(&memory.id).unwrap().content,
            "导出 CSV 使用 UTF-8 编码"
        );
        assert_eq!(server.db.memory_governance(&memory.id).unwrap().revision, 2);

        let other = seed("日志使用 Shift-JIS 编码");
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,
            "message":"撤回之前记的编码"}),
        )
        .await;
        assert_eq!(
            server.db.memory_governance(&memory.id).unwrap().status,
            "active"
        );
        assert_eq!(
            server.db.memory_governance(&other.id).unwrap().status,
            "active"
        );

        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,
            "message":"第一条"}),
        )
        .await;
        let withdrawn = server.db.memory_governance(&memory.id).unwrap();
        assert_eq!(
            (withdrawn.status.as_str(), withdrawn.revision),
            ("withdrawn", 3)
        );
        assert_eq!(server.db.memory_governance(&other.id).unwrap().revision, 1);
        assert_eq!(server.db.memory_history(&memory.id).unwrap().len(), 3);
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,
            "message":"记得哪些编码偏好？"}),
        )
        .await;
        mock.abort();
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 6);
        let prompt = requests[2]["messages"][0]["content"].as_str().unwrap();
        assert!(prompt.contains(&memory.id) && prompt.contains(&other.id));
        let prompt = requests[5]["messages"][0]["content"].as_str().unwrap();
        assert!(
            !prompt.contains("导出 CSV 使用 UTF-8 编码")
                && prompt.contains("日志使用 Shift-JIS 编码")
        );
        for index in [1, 4] {
            assert!(requests[index]["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| {
                    message["role"] == "tool" && message["name"] == "update_memory"
                }));
        }
    }

    #[tokio::test]
    async fn long_reply_survives_split_utf8_and_continues_in_one_message() {
        use futures::StreamExt;

        const TOKEN: &str = "这是一段长回复，包含中文和🙂。\n";
        const FIRST: &str = "第一部分已经写完。\n第二部";
        const SECOND: &str = "分会接着写。\n第三";
        const THIRD: &str = "部分是结尾。";
        const INTERRUPTED: &str = "已经生成的另一段正文。";
        async fn long_model(
            State(requests): State<CapturedRequests>,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            let mut requests = requests.lock().await;
            let index = requests.len();
            let (token, count, reason) = match index {
                0 => (TOKEN, 400, "stop"),
                1 => (FIRST, 1, "length"),
                2 => (SECOND, 1, "length"),
                3 => (THIRD, 1, "stop"),
                _ => (INTERRUPTED, 1, "length"),
            };
            requests.push(body);
            drop(requests);
            if index >= 5 {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "fixture continuation unavailable",
                )
                    .into_response();
            }
            let delta = format!(
                "data: {}\n\n",
                json!({"choices":[{"index":0,"delta":{"content":token},"finish_reason":null}]}),
            );
            let wire = format!(
                "{}data: {}\n\ndata: [DONE]\n\n",
                delta.repeat(count),
                json!({"choices":[{"index":0,"delta":{},"finish_reason":reason}]}),
            );
            let cut = wire.find(token.chars().next().unwrap()).unwrap() + 1;
            let (first, rest) = wire.as_bytes().split_at(cut);
            let stream =
                futures::stream::iter([first.to_vec(), rest.to_vec()]).then(|part| async move {
                    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                    Ok::<_, std::convert::Infallible>(part)
                });
            (
                [(CONTENT_TYPE, "text/event-stream")],
                Body::from_stream(stream),
            )
                .into_response()
        }
        async fn events(
            server: Arc<AppServer>,
            token: &str,
            id: &str,
            message: &str,
        ) -> Vec<Value> {
            let response = build_router(server)
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/chat")
                        .header(CONTROL_SESSION_HEADER, token)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({"conversation_id":id,"message":message}).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            std::str::from_utf8(&body)
                .unwrap()
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|data| serde_json::from_str(data.trim()).ok())
                .collect()
        }

        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(long_model))
            .with_state(requests.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mock = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &format!("http://{address}"));
        let conversation = server.db.create_conversation("长回复").unwrap();
        let id = conversation.id.as_str();

        let expected = TOKEN.repeat(400);
        let complete = events(server.clone(), &token, id, "写一份完整长文").await;
        let streamed = complete
            .iter()
            .filter(|event| event["type"] == "token")
            .filter_map(|event| event["token"].as_str())
            .collect::<String>();
        assert!(streamed == expected, "长回复的分片必须完整且没有乱码");
        let done = complete
            .iter()
            .find(|event| event["type"] == "done")
            .unwrap();
        assert!(done["result"].as_str() == Some(expected.as_str()));
        let saved = server.db.get_conversation(id).unwrap();
        assert!(saved.messages.last().unwrap().content == expected);

        let continued = events(server.clone(), &token, id, "展开第二部分").await;
        let expected = format!("{FIRST}{SECOND}{THIRD}");
        let streamed = continued
            .iter()
            .filter(|event| event["type"] == "token")
            .filter_map(|event| event["token"].as_str())
            .collect::<String>();
        assert_eq!(streamed, expected);
        let done = continued
            .iter()
            .filter(|event| event["type"] == "done")
            .collect::<Vec<_>>();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0]["result"], expected);
        assert!(!continued.iter().any(|event| event["type"] == "error"));
        let saved = server.db.get_conversation(id).unwrap();
        assert_eq!(saved.messages.len(), 4);
        assert_eq!(saved.messages.last().unwrap().content, expected);
        assert_eq!(saved.messages[2].content, "展开第二部分");

        let interrupted = events(server.clone(), &token, id, "另写一段长回复").await;
        assert!(interrupted.iter().any(|event| event["type"] == "error"));
        assert!(!interrupted.iter().any(|event| event["type"] == "done"));
        let saved = server.db.get_conversation(id).unwrap();
        assert_eq!(saved.messages.len(), 6);
        assert_eq!(saved.messages.last().unwrap().content, INTERRUPTED);
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 6);
        for (index, prefix) in [(2, FIRST.to_owned()), (3, format!("{FIRST}{SECOND}"))] {
            assert!(requests[index]["tools"].is_null());
            let messages = requests[index]["messages"].as_array().unwrap();
            assert_eq!(messages[messages.len() - 2]["role"], "assistant");
            assert_eq!(messages[messages.len() - 2]["content"], prefix);
        }
        mock.abort();
    }

    #[tokio::test]
    async fn voice_questions_use_the_bound_conversation_and_its_history() {
        use crate::shared::{
            interaction::{ContextAnchorSnapshot, ConversationalAnchor, FocusedSurface},
            voice::VoiceInputOwner,
        };

        let (base_url, requests, mock) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conversation = server.db.create_conversation("开发记录").unwrap();
        let other = server.db.create_conversation("安装包").unwrap();
        let prior = "之前已完成工具调用回归。";
        for message in [
            message_row(&conversation.id, "user", "记一下开发进度", 1),
            message_row(&conversation.id, "assistant", prior, 2),
            message_row(&other.id, "user", "记录安装包的验收方式", 3),
        ] {
            server.db.add_message(&message).unwrap();
        }
        let started = server.voice_runtime.start_default().unwrap();
        let mut session = server
            .voice_runtime
            .update_context(
                &started.voice_session_id,
                started.generation,
                ContextAnchorSnapshot {
                    focused_surface: FocusedSurface::Conversation,
                    conversational_anchor: Some(ConversationalAnchor::new(
                        &conversation.id,
                        &conversation.title,
                        1,
                    )),
                    active_task: None,
                },
            )
            .unwrap();
        for (question, target) in [
            ("接下来还有哪些要做？", conversation.id.as_str()),
            ("总结一下这个对话", conversation.id.as_str()),
            ("安装包怎么验证？", conversation.id.as_str()),
            ("切换到安装包对话", other.id.as_str()),
        ] {
            session = server
                .voice_runtime
                .interrupt(&session.voice_session_id, session.generation)
                .unwrap();
            let lease = server
                .voice_runtime
                .acquire_lease(
                    &session.voice_session_id,
                    session.generation,
                    VoiceInputOwner::BuiltinAsr,
                )
                .unwrap();
            server
                .voice_runtime
                .commit_final(
                    &session.voice_session_id,
                    session.generation,
                    &lease.lease_id,
                    question,
                )
                .unwrap();
            let response = build_router(server.clone()).oneshot(
                Request::builder().method("POST").uri("/api/voice/turns/dispatch")
                    .header(CONTROL_SESSION_HEADER, &token).header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"session_id":session.voice_session_id,
                        "generation":session.generation,"lease_id":lease.lease_id,"final_transcript":question}).to_string()))
                    .unwrap(),
            ).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let outcome: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                    .unwrap();
            assert_eq!(
                outcome["continuation"]["conversation_id"], target,
                "{question}: {outcome}"
            );
            assert_eq!(outcome["continuation"]["message"], question);
            send_chat(
                server.clone(),
                &token,
                json!({
                    "conversation_id":outcome["continuation"]["conversation_id"],
                    "message":outcome["continuation"]["message"],
                }),
            )
            .await;
        }
        mock.abort();

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 4);
        for request in &requests[..3] {
            assert!(request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| { message["role"] == "assistant" && message["content"] == prior }));
        }
        assert!(!requests[3]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["content"] == prior));
    }

    #[tokio::test]
    async fn brief_workspace_inspection_reads_the_bound_readme_first() {
        async fn inspect_model(
            State((requests, root)): State<(CapturedRequests, PathBuf)>,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            let mut requests = requests.lock().await;
            let messages = body["messages"].as_array().unwrap();
            let last_result = messages
                .iter()
                .rev()
                .find(|message| message["role"] == "tool");
            let call = match last_result {
                Some(result) if result["name"] == "glob" => result["content"]
                    .as_str()
                    .unwrap()
                    .lines()
                    .find(|path| {
                        PathBuf::from(path)
                            .file_name()
                            .is_some_and(|name| name == "README.md")
                    })
                    .map(|path| ("read_file", json!({"path":path}))),
                None if messages[0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("默认快速查看当前工作空间") =>
                {
                    Some(("glob", json!({"path":root,"pattern":"*"})))
                }
                _ => None,
            };
            let (delta, finish) = if let Some((name, args)) = call {
                (
                    json!({"tool_calls":[{"index":0,"id":format!("overview-{}",requests.len()),
                    "type":"function","function":{"name":name,"arguments":args.to_string()}}]}),
                    "tool_calls",
                )
            } else {
                (json!({"content":"工作空间概览已整理。"}), "stop")
            };
            requests.push(body);
            (
                [(CONTENT_TYPE, "text/event-stream")],
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({"id":"overview","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
                ),
            )
        }

        let database = TempDatabase::new();
        let root = database.0.with_extension("workspace");
        std::fs::create_dir(&root).unwrap();
        let readme = root.join("README.md");
        std::fs::write(&readme, "# 图书归还管理器\n用于记录图书借还。").unwrap();
        let other_file = root.join("main.rs");
        std::fs::write(&other_file, "fn main() {}").unwrap();
        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(inspect_model))
            .with_state((requests.clone(), root.clone()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mock = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (server, token) = test_server(&database, &format!("http://{address}"));
        let workspace = crate::workspace::Workspace::new(
            crate::workspace::WorkspaceId::generate(),
            "借还管理".into(),
            String::new(),
            Some(root.to_str().unwrap().to_owned()),
            1,
        )
        .unwrap();
        server.db.create_workspace(&workspace).unwrap();
        let conversation = server.db.create_conversation("工作空间查看").unwrap();
        server
            .db
            .set_conversation_workspace(&conversation.id, Some(workspace.id.as_str()))
            .unwrap();
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,"message":"查看下"}),
        )
        .await;
        let with_readme = std::mem::take(&mut *requests.lock().await);

        std::fs::remove_file(&readme).unwrap();
        let conversation = server.db.create_conversation("无入口的工作空间").unwrap();
        server
            .db
            .set_conversation_workspace(&conversation.id, Some(workspace.id.as_str()))
            .unwrap();
        send_chat(
            server,
            &token,
            json!({"conversation_id":conversation.id,"message":"简单看看"}),
        )
        .await;
        mock.abort();
        std::fs::remove_file(&other_file).unwrap();
        std::fs::remove_dir(&root).unwrap();

        assert_eq!(
            with_readme.len(),
            3,
            "应浏览当前空间根目录，读取入口，然后回复"
        );
        let prompt = with_readme[0]["messages"][0]["content"].as_str().unwrap();
        assert!(prompt.contains("借还管理"));
        assert!(prompt.contains(root.to_str().unwrap()));
        assert!(prompt.contains("优先读取 README"));
        let results = with_readme[2]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "tool")
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["name"], "glob");
        assert_eq!(results[1]["name"], "read_file");
        assert!(results[1]["content"]
            .as_str()
            .unwrap()
            .contains("用于记录图书借还"));

        let without_readme = requests.lock().await;
        assert_eq!(without_readme.len(), 2, "没有入口时直接使用顶层结果回复");
        let root_listing = without_readme[1]["messages"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        assert_eq!(root_listing["name"], "glob");
        assert!(root_listing["content"]
            .as_str()
            .unwrap()
            .contains("main.rs"));
    }

    #[tokio::test]
    async fn terse_revision_keeps_the_recent_draft_in_default_chat() {
        let (base_url, requests, mock) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conversation = server.db.create_conversation("活动通知").unwrap();
        let draft = "周六上午九点在图书馆入口集合，参加社区阅读活动。";
        for message in [
            message_row(&conversation.id, "user", "帮我写一条活动通知", 1),
            message_row(&conversation.id, "assistant", draft, 2),
        ] {
            server.db.add_message(&message).unwrap();
        }
        send_chat(
            server,
            &token,
            json!({"conversation_id":conversation.id,"message":"短一点"}),
        )
        .await;
        mock.abort();

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let messages = requests[0]["messages"].as_array().unwrap();
        let prompt = messages[0]["content"].as_str().unwrap();
        assert!(prompt.contains("“短一点”“换个说法”"));
        assert!(prompt.contains("直接给修订结果"));
        assert!(!prompt.contains("目录上下文："));
        assert_eq!(messages[messages.len() - 2]["content"], draft);
        assert_eq!(messages.last().unwrap()["content"], "短一点");
    }

    #[tokio::test]
    async fn complex_chat_batches_independent_reads_before_finishing_the_plan() {
        async fn comparison_model(
            State(requests): State<CapturedRequests>,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            let mut requests = requests.lock().await;
            let round = requests.len();
            let plan = |status| json!({"todos":[{"id":"compare","content":"读取并比较两个项目文件","status":status}]});
            let calls = match round {
                0 => vec![
                    ("write_todos", plan("in_progress")),
                    ("read_file", json!({"path":"Cargo.toml"})),
                    ("read_file", json!({"path":"src/lib.rs"})),
                ],
                1 => vec![("write_todos", plan("completed"))],
                _ => Vec::new(),
            };
            let (delta, finish) = if calls.is_empty() {
                (
                    json!({"content":"Cargo.toml 定义包，src/lib.rs 导出模块。"}),
                    "stop",
                )
            } else {
                let calls = calls.into_iter().enumerate().map(|(index, (name, args))|
                    json!({"index":index,"id":format!("compare-{round}-{index}"),"type":"function",
                        "function":{"name":name,"arguments":args.to_string()}})
                ).collect::<Vec<_>>();
                (json!({"tool_calls":calls}), "tool_calls")
            };
            requests.push(body);
            (
                [(CONTENT_TYPE, "text/event-stream")],
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({"id":"comparison","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
                ),
            )
        }

        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(comparison_model))
            .with_state(requests.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mock = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &format!("http://{address}"));
        send_chat(
            server,
            &token,
            json!({"message":"比较 Cargo.toml 和 src/lib.rs 的用途，记录计划进度，再总结差异"}),
        )
        .await;
        mock.abort();

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 3);
        let prompt = requests[0]["messages"][0]["content"].as_str().unwrap();
        assert!(prompt.contains("按依赖拆成少量阶段"));
        assert!(prompt.contains("互不依赖的读取可以在同一轮提交"));
        let results = requests[1]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "tool")
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 3);
        assert!(results[0]["content"]
            .as_str()
            .unwrap()
            .contains("in_progress"));
        assert!(results[1]["content"]
            .as_str()
            .unwrap()
            .contains("yilian-backend"));
        assert!(results[2]["content"]
            .as_str()
            .unwrap()
            .contains("pub mod agent;"));
        let completed_plan = requests[2]["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(completed_plan["name"], "write_todos");
        assert!(completed_plan["content"]
            .as_str()
            .unwrap()
            .contains("completed"));
    }

    #[tokio::test]
    async fn explicit_chat_memory_is_saved_by_one_tool_call() {
        const CONTENT: &str = "用户偏好导出 CSV 时使用 UTF-8 编码";
        async fn memory_model(
            State(requests): State<CapturedRequests>,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            let mut requests = requests.lock().await;
            let (delta, finish) = if requests.is_empty() {
                let offered = body["tools"].as_array().unwrap().iter().any(|tool| {
                    tool["function"]["name"] == "save_memory"
                        && tool["function"]["parameters"]["properties"]["content"]["type"]
                            == "string"
                });
                if offered {
                    (
                        json!({"tool_calls":[{"index":0,"id":"save-preference","type":"function",
                            "function":{"name":"save_memory","arguments":json!({
                                "content": CONTENT, "category":"preference"
                            }).to_string()}}]}),
                        "tool_calls",
                    )
                } else {
                    (json!({"content":"找不到保存记忆的工具"}), "stop")
                }
            } else {
                (json!({"content":"已记住这个导出偏好。"}), "stop")
            };
            requests.push(body);
            (
                [(CONTENT_TYPE, "text/event-stream")],
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({"id":"memory-test","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
                ),
            )
        }

        let requests = CapturedRequests::default();
        let app = Router::new()
            .route("/chat/completions", post(memory_model))
            .with_state(requests.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mock = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &format!("http://{address}"));
        let conversation = server.db.create_conversation("记忆工具").unwrap();
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,"message":"帮我记一下，以后导出 CSV 用 UTF-8 编码"}),
        )
        .await;
        mock.abort();

        let memories = server
            .db
            .list_memories(&crate::db::MemoryQuery {
                category: None,
                source: None,
                q: None,
                limit: None,
                offset: None,
            })
            .unwrap();
        assert_eq!(memories.len(), 1, "聊天应实际写入一条记忆");
        assert_eq!(memories[0].content, CONTENT);
        assert_eq!(memories[0].category, "preference");
        assert_eq!(
            memories[0].source_conversation_id.as_deref(),
            Some(conversation.id.as_str())
        );
        let audit = server
            .db
            .list_security_audit_events(&crate::db::SecurityAuditQuery {
                conversation_id: Some(conversation.id),
                tool_name: Some("save_memory".into()),
                decision_status: Some("verified".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(audit.len(), 1);

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 2, "保存一次，然后直接回复");
        let tool_results = requests[1]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "tool")
            .collect::<Vec<_>>();
        assert_eq!(tool_results.len(), 1);
        assert_eq!(tool_results[0]["name"], "save_memory");
        assert!(tool_results[0]["content"]
            .as_str()
            .unwrap()
            .contains(&memories[0].id));
    }

    #[tokio::test]
    async fn chat_readiness_uses_scoped_or_default_pool_without_global_key_or_provider_calls() {
        use crate::{
            db::{
                connectors::ConnectorBinding,
                model_pools::{ModelPool, ModelPoolTier},
                LlmModelInput,
            },
            integrations::secret::{
                llm_model_key_ref, InMemorySecretStore, SecretResolver, SecretStore,
            },
            modules::task::{AgentDefinition, AgentId, AgentSource},
        };
        let (url, requests, mock) = start_mock_llm().await;
        let temp = TempDatabase::new();
        let (mut server, token) = test_server(&temp, &url);
        let store = Arc::new(InMemorySecretStore::new());
        let key = llm_model_key_ref("readiness-profile");
        store
            .put(
                &key,
                secrecy::SecretString::new("controlled-readiness-key".into()),
            )
            .await
            .unwrap();
        {
            let server = Arc::get_mut(&mut server).unwrap();
            server.secret_store = store.clone();
            server.secret_resolver = Arc::new(SecretResolver::new(store));
            let mut config = server.config.write();
            config.model.api_key.clear();
            config.model.api_key_env.clear();
            config.model.api_key_ref = None;
        }
        server
            .db
            .create_llm_model(&LlmModelInput {
                id: "readiness-profile".into(),
                provider: "openai".into(),
                label: "Scoped profile".into(),
                model: "actual-scoped-model".into(),
                base_url: url,
                api_format: "openai".into(),
                api_key_ref: key.key,
                api_key_env: String::new(),
                temperature: 0.,
                max_tokens: 10,
                invoke_timeout_ms: 1000,
            })
            .unwrap();
        server
            .db
            .conn()
            .execute("UPDATE llm_models SET active=0", [])
            .unwrap();
        let pool = ModelPool {
            id: "readiness-pool".into(),
            label: "Actual scoped pool".into(),
            service_kind: "chat".into(),
            enabled: true,
            revision: 1,
            attempt_timeout_ms: 1000,
            total_timeout_ms: 1000,
            tiers: vec![ModelPoolTier {
                id: "tier-1".into(),
                label: "Primary".into(),
                profile_ids: vec!["readiness-profile".into()],
            }],
        };
        server
            .db
            .conn()
            .execute(
                "INSERT INTO model_pools(id,revision,payload) VALUES(?1,?2,?3)",
                rusqlite::params![
                    pool.id,
                    pool.revision,
                    serde_json::to_string(&pool).unwrap()
                ],
            )
            .unwrap();
        let conversation = server.db.create_conversation("Scoped readiness").unwrap();
        let agent = AgentDefinition {
            id: AgentId::new("readiness-agent").unwrap(),
            name: "Scoped".into(),
            description: String::new(),
            instructions: String::new(),
            allowed_tools: vec![],
            model: None,
            capabilities: vec![],
            max_iterations: 10,
            enabled: true,
            source: AgentSource::Database,
        };
        server.db.create_agent_definition(&agent).unwrap();
        server
            .db
            .save_connector_binding(
                agent.id.as_str(),
                &ConnectorBinding {
                    id: String::new(),
                    kind: "conversation_agent".into(),
                    source_id: format!("conversation:{}", conversation.id),
                    target_id: format!("agent:{}", agent.id),
                    service_kind: None,
                    model_source: None,
                },
            )
            .unwrap();
        server
            .db
            .set_model_pool_binding(agent.id.as_str(), Some(&pool.id))
            .unwrap();
        for (scoped, expected) in [(false, false), (true, true)] {
            let path = if scoped {
                format!("/api/chat/readiness?conversation_id={}", conversation.id)
            } else {
                "/api/chat/readiness".into()
            };
            let response = build_router(server.clone())
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(path)
                        .header(CONTROL_SESSION_HEADER, &token)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                    .unwrap();
            assert_eq!(body["available"], expected);
            if scoped {
                assert_eq!(body["configured"], true);
                assert_eq!(body["provider"], "model_pool");
                assert_eq!(body["model"], pool.label);
            }
        }
        server
            .db
            .set_model_pool_binding("default", Some(&pool.id))
            .unwrap();
        let default = crate::api::chat::runtime::readiness(&server, None).await;
        assert!(default.available);
        assert_eq!(default.model, pool.label);
        server
            .db
            .conn()
            .execute("DELETE FROM llm_models WHERE id='readiness-profile'", [])
            .unwrap();
        assert!(
            !crate::api::chat::runtime::readiness(&server, Some(&conversation.id))
                .await
                .available
        );
        assert!(requests.lock().await.is_empty());
        assert!(server.active_tasks.lock().is_empty());
        mock.abort();
    }

    #[tokio::test]
    async fn disabled_pool_rejects_chat_before_registering_or_saving_user_message() {
        use crate::{
            db::{connectors::ConnectorBinding, model_pools::ModelPool},
            modules::task::{AgentDefinition, AgentId, AgentSource},
        };
        let (url, requests, mock) = start_mock_llm().await;
        for bound in [false, true] {
            let temp = TempDatabase::new();
            let (server, token) = test_server(&temp, &url);
            let conversation = server.db.create_conversation("Preserve draft").unwrap();
            let pool = ModelPool {
                id: "disabled-startup-pool".into(),
                label: "Disabled".into(),
                service_kind: "chat".into(),
                enabled: false,
                revision: 2,
                attempt_timeout_ms: 100,
                total_timeout_ms: 100,
                tiers: vec![],
            };
            server
                .db
                .conn()
                .execute(
                    "INSERT INTO model_pools(id,revision,payload) VALUES(?1,?2,?3)",
                    rusqlite::params![
                        pool.id,
                        pool.revision,
                        serde_json::to_string(&pool).unwrap()
                    ],
                )
                .unwrap();
            let subject = if bound { "startup-agent" } else { "default" };
            if bound {
                server
                    .db
                    .create_agent_definition(&AgentDefinition {
                        id: AgentId::new(subject).unwrap(),
                        name: "Startup".into(),
                        description: String::new(),
                        instructions: String::new(),
                        allowed_tools: vec![],
                        model: None,
                        capabilities: vec![],
                        max_iterations: 10,
                        enabled: true,
                        source: AgentSource::Database,
                    })
                    .unwrap();
                server
                    .db
                    .save_connector_binding(
                        subject,
                        &ConnectorBinding {
                            id: String::new(),
                            kind: "conversation_agent".into(),
                            source_id: format!("conversation:{}", conversation.id),
                            target_id: format!("agent:{subject}"),
                            service_kind: None,
                            model_source: None,
                        },
                    )
                    .unwrap();
            }
            // An enabled selection may subsequently be disabled; the request must reject that snapshot.
            server
                .db
                .conn()
                .execute(
                    "INSERT INTO model_pool_bindings(subject_id,pool_id) VALUES(?1,?2)",
                    rusqlite::params![subject, pool.id],
                )
                .unwrap();
            let response=build_router(server.clone()).oneshot(Request::builder().method("POST").uri("/api/chat").header(CONTROL_SESSION_HEADER,&token).header(CONTENT_TYPE,"application/json").body(Body::from(json!({"conversation_id":conversation.id,"message":"UNACCEPTED USER DRAFT"}).to_string())).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            assert!(server.active_tasks.lock().is_empty());
            let persisted = server.db.get_conversation(&conversation.id).unwrap();
            let status: String = server
                .db
                .conn()
                .query_row(
                    "SELECT run_status FROM conversations WHERE id=?1",
                    [&conversation.id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(status, "idle");
            assert!(persisted.messages.is_empty());
            assert_eq!(persisted.title, "Preserve draft");
        }
        assert!(requests.lock().await.is_empty());
        mock.abort();
    }

    #[tokio::test]
    async fn connector_chat_uses_bound_tools_memory_and_next_turn_persona() {
        use crate::{
            api::agents::persona::{save_profile, AgentProfile, Persona},
            db::{connectors::ConnectorBinding, CreateMemoryRequest},
            modules::task::{AgentDefinition, AgentId, AgentSource},
        };
        let (url, requests, mock) = start_mock_llm().await;
        let temp = TempDatabase::new();
        let (mut server, token) = test_server(&temp, &url);
        use crate::integrations::secret::{
            llm_model_key_ref, InMemorySecretStore, SecretResolver, SecretStore,
        };
        let store = Arc::new(InMemorySecretStore::new());
        let key = llm_model_key_ref("bound-profile");
        store
            .put(
                &key,
                secrecy::SecretString::new("controlled-mock-key".into()),
            )
            .await
            .unwrap();
        {
            let server = Arc::get_mut(&mut server).unwrap();
            server.secret_store = store.clone();
            server.secret_resolver = Arc::new(SecretResolver::new(store));
        }
        server
            .db
            .create_llm_model(&crate::db::LlmModelInput {
                id: "bound-profile".into(),
                provider: "openai".into(),
                label: "Bound profile".into(),
                model: "gpt-4o".into(),
                base_url: url.clone(),
                api_format: "openai".into(),
                api_key_ref: key.key,
                api_key_env: String::new(),
                temperature: 0.2,
                max_tokens: 100,
                invoke_timeout_ms: 5000,
            })
            .unwrap();
        let agent = AgentDefinition {
            id: AgentId::new("bound-chat-agent").unwrap(),
            name: "Bound".into(),
            description: String::new(),
            instructions: String::new(),
            allowed_tools: vec!["read_file".into()],
            model: None,
            capabilities: vec![],
            max_iterations: 10,
            enabled: true,
            source: AgentSource::Database,
        };
        server.db.create_agent_definition(&agent).unwrap();
        let conversation = server.db.create_conversation("Bound").unwrap();
        server
            .db
            .save_connector_binding(
                agent.id.as_str(),
                &ConnectorBinding {
                    id: String::new(),
                    kind: "conversation_agent".into(),
                    source_id: format!("conversation:{}", conversation.id),
                    target_id: format!("agent:{}", agent.id),
                    service_kind: None,
                    model_source: None,
                },
            )
            .unwrap();
        server
            .db
            .save_connector_binding(
                agent.id.as_str(),
                &ConnectorBinding {
                    id: String::new(),
                    kind: "agent_model".into(),
                    source_id: format!("agent:{}", agent.id),
                    target_id: "model:bound-profile".into(),
                    service_kind: Some("chat".into()),
                    model_source: Some("profile".into()),
                },
            )
            .unwrap();
        let memory = server
            .db
            .create_memory(&CreateMemoryRequest {
                content: "BOUND BACKGROUND ONLY".into(),
                category: "fact".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        server
            .db
            .create_memory(&CreateMemoryRequest {
                content: "remember star UNBOUND MEMORY MUST NOT APPEAR".into(),
                category: "fact".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        server
            .db
            .save_connector_binding(
                agent.id.as_str(),
                &ConnectorBinding {
                    id: String::new(),
                    kind: "agent_memory".into(),
                    source_id: format!("agent:{}", agent.id),
                    target_id: format!("memory:{}", memory.id),
                    service_kind: None,
                    model_source: None,
                },
            )
            .unwrap();
        let knowledge = server
            .db
            .create_memory(&CreateMemoryRequest {
                content: "BOUND KNOWLEDGE ONLY".into(),
                category: "knowledge".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        server
            .db
            .create_memory(&CreateMemoryRequest {
                content: "remember star UNBOUND KNOWLEDGE MUST NOT APPEAR".into(),
                category: "knowledge".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        server
            .db
            .save_connector_binding(
                agent.id.as_str(),
                &ConnectorBinding {
                    id: String::new(),
                    kind: "agent_knowledge".into(),
                    source_id: format!("agent:{}", agent.id),
                    target_id: format!("knowledge:{}", knowledge.id),
                    service_kind: None,
                    model_source: None,
                },
            )
            .unwrap();
        let mut profile = AgentProfile {
            user_prompt: "SELECTED USER INSTRUCTION".into(),
            persona_enabled: true,
            persona: Persona {
                tone: "CALM FIRST TURN".into(),
                ..Default::default()
            },
        };
        save_profile(&server.db, agent.id.as_str(), &profile).unwrap();
        let response = build_router(server.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/api/agents/{}/generation-options", agent.id))
                    .header(CONTROL_SESSION_HEADER, &token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let options: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(options["model"], "gpt-4o");
        assert_eq!(options["temperature"], true);
        assert_eq!(options["max_output_tokens"], 100);
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,"message":"remember star"}),
        )
        .await;
        let response = build_router(server.clone())
            .oneshot(Request::builder()
                .method("PUT")
                .uri(format!("/api/agents/{}/profile", agent.id))
                .header(CONTROL_SESSION_HEADER, &token)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json!({
                    "user_prompt": "SELECTED USER INSTRUCTION",
                    "persona_enabled": true,
                    "persona": {"tone": "PRECISE NEXT TURN", "generation": {"temperature": 0.65, "max_output_tokens": 48}}
                }).to_string())).unwrap())
            .await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        profile.persona.tone = "PRECISE NEXT TURN".into();
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,"message":"remember star again"}),
        )
        .await;
        let memory_binding = server
            .db
            .resolve_connector_bindings(agent.id.as_str())
            .unwrap()
            .into_iter()
            .find(|b| b.kind == "agent_memory")
            .unwrap();
        server
            .db
            .delete_connector_binding(agent.id.as_str(), &memory_binding.id)
            .unwrap();
        server.db.delete_memory(&knowledge.id).unwrap();
        profile.persona_enabled = false;
        save_profile(&server.db, agent.id.as_str(), &profile).unwrap();
        send_chat(server.clone(),&token,json!({"conversation_id":conversation.id,"message":"remember star after removing last memory and knowledge"})).await;
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0]["temperature"], 0.2);
        assert_eq!(requests[0]["max_tokens"], 100);
        assert_eq!(requests[1]["temperature"], 0.65);
        assert_eq!(requests[1]["max_tokens"], 48);
        assert_eq!(requests[2]["temperature"], 0.2);
        assert_eq!(requests[2]["max_tokens"], 100);
        for request in requests.iter().take(2) {
            assert_eq!(request["model"], "gpt-4o");
            let text = request["messages"][0]["content"].as_str().unwrap();
            assert!(text.contains("BOUND BACKGROUND ONLY"));
            assert!(text.contains("BOUND KNOWLEDGE ONLY"));
            assert!(!text.contains("UNBOUND KNOWLEDGE MUST NOT APPEAR"));
            assert!(!text.contains("UNBOUND MEMORY MUST NOT APPEAR"));
            assert!(text.contains("SELECTED USER INSTRUCTION"));
            let exposed_tools = request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool| tool["function"]["name"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>();
            // Binding admits only read_file as an action. ask_user is the
            // mandatory necessary-input control and cannot execute an action.
            assert_eq!(
                exposed_tools,
                std::collections::BTreeSet::from(["ask_user", "read_file"])
            );
        }
        let last_prompt = requests[2]["messages"][0]["content"].as_str().unwrap();
        for forbidden in [
            "BOUND BACKGROUND ONLY",
            "BOUND KNOWLEDGE ONLY",
            "UNBOUND MEMORY MUST NOT APPEAR",
            "UNBOUND KNOWLEDGE MUST NOT APPEAR",
        ] {
            assert!(
                !last_prompt.contains(forbidden),
                "last unbind/delete exposed context: {forbidden}"
            );
        }
        assert!(requests[0]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("CALM FIRST TURN"));
        assert!(requests[1]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("PRECISE NEXT TURN"));
        mock.abort();
    }

    #[test]
    fn legacy_chat_request_defaults_goal_ref_to_none_and_goal_ref_is_strict() {
        let legacy: ChatRequest = serde_json::from_value(json!({ "message": "hello" })).unwrap();
        assert!(legacy.goal_ref.is_none());

        let malformed = serde_json::from_value::<ChatRequest>(json!({
            "message": "hello",
            "goal_ref": {"goal_id": "goal-1", "goal_version": 1, "workspace_id": "other"}
        }));
        assert!(malformed.is_err());
    }

    #[test]
    fn persisted_goal_binding_is_reused_when_omitted_and_cannot_be_rebound() {
        let goal_ref = crate::agent::delegation::GoalRef {
            goal_id: "goal-1".to_string(),
            goal_version: 2,
        };
        let saved = Some(goal_ref.clone());
        assert_eq!(
            resolve_conversation_goal_ref(None, saved.clone()).unwrap(),
            Some(goal_ref)
        );

        let rebound = crate::agent::delegation::GoalRef {
            goal_id: "goal-2".to_string(),
            goal_version: 1,
        };
        let error = resolve_conversation_goal_ref(Some(&rebound), saved).unwrap_err();
        assert_eq!(error.0, StatusCode::CONFLICT);
        assert_eq!(error.1 .0["code"], "proofloop_goal_rebind_rejected");
    }

    #[tokio::test]
    async fn existing_conversation_sends_current_user_message_once_and_keeps_history() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conversation = server.db.create_conversation("existing").unwrap();
        let mut previous_answer = message_row(&conversation.id, "assistant", "previous answer", 2);
        previous_answer.tool_calls = Some(
            json!([{
                "id":"previous-call", "type":"function",
                "function":{"name":"read_file", "arguments":"{}"}
            }])
            .to_string(),
        );
        let mut previous_tool = message_row(&conversation.id, "tool", "previous tool result", 3);
        previous_tool.tool_call_id = Some("previous-call".into());
        previous_tool.tool_name = Some("read_file".into());
        for message in [
            message_row(&conversation.id, "user", "previous question", 1),
            previous_answer,
            previous_tool,
        ] {
            server.db.add_message(&message).unwrap();
        }

        send_chat(
            server.clone(),
            &token,
            json!({
                "conversation_id": conversation.id,
                "message": "帮我检查 README"
            }),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(current_user_message_count(request, "帮我检查 README"), 1);
        assert!(request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["role"] == "assistant" && message["content"] == "previous answer"
            }));
        assert!(request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["role"] == "tool" && message["content"] == "previous tool result"
            }));
        drop(requests);

        let persisted = server.db.get_conversation(&conversation.id).unwrap();
        assert_eq!(
            persisted
                .messages
                .iter()
                .filter(|message| {
                    message.role == "user" && message.content == "帮我检查 README"
                })
                .count(),
            1
        );
        mock_llm.abort();
    }

    #[tokio::test]
    async fn new_conversation_sends_and_persists_current_user_message_once() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);

        send_chat(
            server.clone(),
            &token,
            json!({"message": "帮我检查 README"}),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            current_user_message_count(&requests[0], "帮我检查 README"),
            1
        );
        drop(requests);

        let conversations = server.db.list_conversations().unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(
            conversations[0].run_status,
            crate::db::ConversationRunStatus::Completed
        );
        assert!(conversations[0].run_started_at.is_some());
        assert!(conversations[0].run_finished_at.is_some());
        let persisted = server.db.get_conversation(&conversations[0].id).unwrap();
        assert_eq!(
            persisted
                .messages
                .iter()
                .filter(|message| {
                    message.role == "user" && message.content == "帮我检查 README"
                })
                .count(),
            1
        );
        mock_llm.abort();
    }

    #[tokio::test]
    async fn blank_message_is_rejected_without_creating_a_conversation() {
        let (base_url, _requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);

        let response = build_router(server.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header(CONTROL_SESSION_HEADER, token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"message": "   "}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(server.db.list_conversations().unwrap().is_empty());
        mock_llm.abort();
    }

    #[tokio::test]
    async fn unconfirmed_goal_ref_is_rejected_before_creating_a_conversation() {
        let (base_url, _requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);

        let response = build_router(server.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header(CONTROL_SESSION_HEADER, token)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "message": "continue the goal",
                            "goal_ref": {"goal_id": "missing-goal", "goal_version": 1}
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(server.db.list_conversations().unwrap().is_empty());
        mock_llm.abort();
    }

    #[tokio::test]
    async fn empty_conversations_are_hidden_and_loaded_conversations_include_execution_history() {
        let (base_url, _requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let empty = server.db.create_conversation("新对话").unwrap();

        send_chat(server.clone(), &token, json!({"message": "保留这条记录"})).await;

        let summaries = server.db.list_conversations().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_ne!(summaries[0].id, empty.id);

        let tool_call_id = "call-persisted";
        for event in [
            AgentEvent {
                event_type: "tool_start".to_string(),
                conversation_id: summaries[0].id.clone(),
                token: None,
                tool_call_id: Some(tool_call_id.to_string()),
                tool_name: Some("open_notepad".to_string()),
                args: Some(json!({"title": "记事本"})),
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: Some("low".to_string()),
                reason: None,
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                configuration_source: None,
                should_replan: None,
            },
            AgentEvent {
                event_type: "tool_end".to_string(),
                conversation_id: summaries[0].id.clone(),
                token: None,
                tool_call_id: Some(tool_call_id.to_string()),
                tool_name: Some("open_notepad".to_string()),
                args: None,
                result: Some("已打开记事本。".to_string()),
                status: Some("success".to_string()),
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: None,
                verification_reason: None,
                configuration_source: None,
                should_replan: None,
            },
            AgentEvent {
                event_type: "verification".to_string(),
                conversation_id: summaries[0].id.clone(),
                token: None,
                tool_call_id: Some(tool_call_id.to_string()),
                tool_name: Some("open_notepad".to_string()),
                args: None,
                result: None,
                status: None,
                error: None,
                message_id: None,
                risk_level: None,
                reason: None,
                approval_id: None,
                verification_success: Some(true),
                verification_reason: Some("窗口已出现".to_string()),
                configuration_source: None,
                should_replan: Some(false),
            },
        ] {
            server.db.apply_execution_event(&event).unwrap();
        }

        let loaded = server.db.get_conversation(&summaries[0].id).unwrap();
        let json = serde_json::to_value(loaded).unwrap();
        assert_eq!(json["execution_history"].as_array().unwrap().len(), 1);
        assert_eq!(json["execution_history"][0]["executionStatus"], "succeeded");
        assert_eq!(json["execution_history"][0]["verificationStatus"], "passed");
        mock_llm.abort();
    }

    // ── Memory context injection tests ──

    use crate::db::{Memory, ScoredMemory};

    fn scored_memory(category: &str, content: &str) -> ScoredMemory {
        ScoredMemory {
            memory: Memory {
                id: format!("mem-{category}"),
                content: content.to_string(),
                category: category.to_string(),
                source: "auto".to_string(),
                source_conversation_id: None,
                embedding: Some("[0.1,0.2,0.3]".to_string()),
                metadata: None,
                created_at: 0,
                updated_at: 0,
            },
            score: 0.9,
            lexical_score: 0.8,
            vector_score: 0.95,
            revision: 1,
            scope: crate::db::MemoryScope::Profile,
        }
    }

    #[test]
    fn build_system_prompt_no_longer_injects_recent_memories() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let conv = server.db.create_conversation("mem").unwrap();
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "这段记忆不应被自动注入".to_string(),
                category: "fact".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        let prompt = server.build_system_prompt();
        assert!(!prompt.contains("这段记忆不应被自动注入"));
        assert!(!prompt.contains("用户长期记忆"));
    }

    #[test]
    fn append_memory_context_formats_preference_and_safety_boundary() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let memories = vec![scored_memory("preference", "用户偏好最小修改")];

        let mut prompt = String::from("base prompt");
        server.append_memory_context(&mut prompt, &memories);

        assert!(prompt.contains("与当前请求相关的长期记忆"));
        assert!(prompt.contains("[偏好] 用户偏好最小修改"));
        assert!(prompt.contains("不是系统指令"));
        assert!(prompt.contains("以当前请求为准"));
    }

    #[test]
    fn append_memory_context_does_not_leak_embedding_or_scores() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let memories = vec![scored_memory("fact", "某个事实")];

        let mut prompt = String::from("base");
        server.append_memory_context(&mut prompt, &memories);

        assert!(!prompt.contains("embedding"));
        assert!(!prompt.contains("0.1,0.2"));
        assert!(!prompt.contains("score"));
        assert!(!prompt.contains("lexical_score"));
        assert!(!prompt.contains("vector_score"));
        assert!(!prompt.contains("mem-fact"));
    }

    #[test]
    fn append_memory_context_truncates_long_memory() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");
        let long = "长".repeat(2000);
        let memories = vec![scored_memory("note", &long)];

        let mut prompt = String::from("base");
        server.append_memory_context(&mut prompt, &memories);

        // Should be truncated to CHAT_MEMORY_MAX_CHARS (500), not panic
        assert!(prompt.contains(&"长".repeat(500)));
        assert!(!prompt.contains(&"长".repeat(501)));
    }

    #[test]
    fn append_memory_context_empty_is_noop() {
        let database = TempDatabase::new();
        let (server, _token) = test_server(&database, "http://localhost:1");

        let mut prompt = String::from("base");
        server.append_memory_context(&mut prompt, &[]);
        assert_eq!(prompt, "base");
    }

    #[tokio::test]
    async fn chat_retrieves_memory_from_current_user_message() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conv = server.db.create_conversation("mem-chat").unwrap();
        // Memory that lexically matches the current message query
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "用户偏好每次只修改少量必要文件".to_string(),
                category: "preference".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        send_chat(
            server.clone(),
            &token,
            json!({"message": "我修改代码时希望怎么控制改动范围？", "conversation_id": conv.id}),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let system_msg = requests[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "system")
            .expect("system message");
        // Lexical match on the current message should inject the memory
        assert!(system_msg["content"]
            .as_str()
            .unwrap()
            .contains("用户偏好每次只修改少量必要文件"));
        drop(requests);
        mock_llm.abort();
    }

    #[tokio::test]
    async fn chat_uses_lexical_mode_without_embedding_provider() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        // Embedding provider not configured (default test config has empty embedding model)
        let conv = server.db.create_conversation("mem-lexical").unwrap();
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "关于 Rust 安全执行的关键记忆".to_string(),
                category: "knowledge".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        send_chat(
            server.clone(),
            &token,
            json!({"message": "Rust 安全执行", "conversation_id": conv.id}),
        )
        .await;

        // Chat still worked, LLM was called once
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let system_msg = requests[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "system")
            .expect("system message");
        assert!(system_msg["content"]
            .as_str()
            .unwrap()
            .contains("Rust 安全执行的关键记忆"));
        drop(requests);
        mock_llm.abort();
    }

    #[tokio::test]
    async fn chat_recall_followup_uses_the_current_memory_revision_and_honors_withdrawal() {
        use crate::db::memory_governance::{CorrectMemoryRequest, WithdrawMemoryRequest};

        let (base_url, requests, mock) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conversation = server.db.create_conversation("导出设置").unwrap();
        let memory = server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "导出 CSV 使用 GBK 编码".into(),
                category: "preference".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        server
            .db
            .correct_memory(
                &memory.id,
                &CorrectMemoryRequest {
                    content: "导出 CSV 使用 UTF-8 编码".into(),
                    expected_revision: 1,
                    reason: "用户更正编码".into(),
                    scope: None,
                    source_ref: None,
                },
            )
            .unwrap();
        for row in [
            message_row(&conversation.id, "user", "说一下导出 CSV 的编码", 1),
            message_row(&conversation.id, "assistant", "可以讨论编码设置。", 2),
        ] {
            server.db.add_message(&row).unwrap();
        }
        send_chat(
            server.clone(),
            &token,
            json!({
                "conversation_id":conversation.id, "message":"你还记得吗？",
            }),
        )
        .await;
        server
            .db
            .withdraw_memory(
                &memory.id,
                &WithdrawMemoryRequest {
                    expected_revision: 2,
                    reason: "用户撤回该偏好".into(),
                },
            )
            .unwrap();
        send_chat(
            server,
            &token,
            json!({
                "conversation_id":conversation.id, "message":"导出 CSV 使用什么编码？",
            }),
        )
        .await;
        mock.abort();
        let requests = requests.lock().await;
        let prompt = |index: usize| requests[index]["messages"][0]["content"].as_str().unwrap();
        assert!(prompt(0).contains("导出 CSV 使用 UTF-8 编码"));
        assert!(!prompt(0).contains("导出 CSV 使用 GBK 编码"));
        assert!(!prompt(1).contains("导出 CSV 使用 UTF-8 编码"));
        assert!(!prompt(1).contains("导出 CSV 使用 GBK 编码"));
        assert_eq!(current_user_message_count(&requests[0], "你还记得吗？"), 1);
    }

    #[tokio::test]
    async fn chat_no_related_memory_omits_section() {
        let (base_url, requests, mock_llm) = start_mock_llm().await;
        let database = TempDatabase::new();
        let (server, token) = test_server(&database, &base_url);
        let conv = server.db.create_conversation("mem-none").unwrap();
        server
            .db
            .create_memory(&crate::db::CreateMemoryRequest {
                content: "关于咖啡口味的信息".to_string(),
                category: "fact".to_string(),
                source: "manual".to_string(),
                source_conversation_id: Some(conv.id.clone()),
                metadata: None,
            })
            .unwrap();

        send_chat(
            server.clone(),
            &token,
            json!({"message": "k8s networking ingress", "conversation_id": conv.id}),
        )
        .await;

        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        let system_msg = requests[0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "system")
            .expect("system message");
        assert!(!system_msg["content"]
            .as_str()
            .unwrap()
            .contains("## Relevant memory"));
        drop(requests);
        mock_llm.abort();
    }
}
