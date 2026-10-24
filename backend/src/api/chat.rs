// ============================================================
// Chat API — POST /api/chat (SSE streaming)
// ============================================================

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[path = "chat_grounding.rs"]
pub(crate) mod grounding;
#[path = "chat_lifecycle.rs"]
pub(crate) mod lifecycle;
#[path = "chat_options.rs"]
pub(crate) mod options;
#[path = "chat_runtime.rs"]
pub(crate) mod runtime;

pub use crate::agent::delegation::GoalRef as ChatGoalRef;
use crate::agent::engine::{self, AgentEvent};
use crate::agent::state::AgentState;
use crate::agent::verifier::DefaultVerifier;
use crate::app::state::{AppServer, CHAT_MEMORY_TOP_K};
use crate::db::{Database, MessageRow, RetrieveQuery};
use crate::integrations::llm::client::LlmClient;
use crate::safety::SecurityExecutionGateway;
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
    reservation: GoalCallReservation,
    activity_lease: crate::modules::task::proofloop::budget::BudgetLease,
}

pub(crate) struct GoalChatRun {
    pub(crate) scope: crate::agent::delegation::ExecutionScope,
    pub(crate) goal_ref: ChatGoalRef,
    pub(crate) goal_summary: String,
    reservation: GoalCallReservation,
    activity_lease: crate::modules::task::proofloop::budget::BudgetLease,
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

struct GoalCallReservation {
    db: Database,
    reservation_id: String,
    budget: Arc<crate::modules::task::resource_budget::SharedExecutionBudget>,
    started_at_ms: i64,
    started: std::time::Instant,
    attempted: bool,
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

impl GoalCallReservation {
    fn finish(&mut self) -> Result<(), String> {
        if self.attempted {
            return Ok(());
        }
        self.attempted = true;
        let usage = match self.budget.usage() {
            Ok((model_calls, total_tokens)) => crate::modules::task::proofloop::PlanningUsage {
                model_calls,
                total_tokens,
                started_at_ms: self.started_at_ms,
                elapsed_ms: Some(self.started.elapsed().as_millis().min(u64::MAX as u128) as u64),
            },
            Err(error) => {
                self.db
                    .finish_goal_model_call(&self.reservation_id, None)
                    .map_err(|finish_error| finish_error.to_string())?;
                return Err(error);
            }
        };
        self.db
            .finish_goal_model_call(&self.reservation_id, Some(&usage))
            .map_err(|error| error.to_string())
    }
}

impl Drop for GoalCallReservation {
    fn drop(&mut self) {
        if !self.attempted {
            if let Err(error) = self.finish() {
                tracing::error!(error = %error, "abandoned ProofLoop chat reservation could not be settled");
            }
        }
    }
}

fn chat_error(status: StatusCode, code: &str, message: impl Into<String>) -> ChatApiError {
    (
        status,
        Json(serde_json::json!({ "code": code, "error": message.into() })),
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
    let bound = db.bind_conversation_goal(&binding).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "proofloop_goal_binding_persist_failed",
            error,
        )
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
    let combined_usage = db
        .combined_goal_model_usage(&contract, &draft.planning_usage)
        .map_err(proof_error_response)?;
    let Some(used_tokens) = combined_usage.total_tokens else {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_usage_unavailable",
            "目标累计 token 用量尚未核实，无法启动目标范围内的聊天",
        ));
    };
    let remaining_calls = contract
        .budget
        .max_model_calls
        .saturating_sub(combined_usage.model_calls);
    let remaining_tokens = contract.budget.max_total_tokens.saturating_sub(used_tokens);
    if remaining_calls == 0 || remaining_tokens == 0 {
        return Err(chat_error(
            StatusCode::CONFLICT,
            "proofloop_budget_exhausted",
            "目标的模型调用或 token 预算已耗尽",
        ));
    }
    let reservation_id = db
        .reserve_goal_model_call(&contract, &draft.planning_usage, "goal-chat", now)
        .map_err(proof_error_response)?;
    let budget = crate::modules::task::resource_budget::SharedExecutionBudget::new(
        remaining_calls,
        remaining_tokens,
    );
    let mut reservation = GoalCallReservation {
        db: db.clone_connection(),
        reservation_id,
        budget: Arc::clone(&budget),
        started_at_ms: now,
        started: std::time::Instant::now(),
        attempted: false,
    };
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
    })
}

pub(crate) fn resolve_goal_chat_run(
    server: &AppServer,
    db: &Database,
    reference: &ChatGoalRef,
    cancel: CancellationToken,
) -> Result<GoalChatRun, ChatApiError> {
    let context = resolve_goal_chat_context(server, db, reference)?;
    Ok(GoalChatRun {
        scope: crate::agent::delegation::ExecutionScope {
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
    })
}

/// POST /api/chat — SSE streaming agent response
pub async fn chat_handler(
    State(server): State<Arc<AppServer>>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<serde_json::Value>)>
{
    let message = req.message.trim().to_string();
    if message.is_empty() {
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
    let persisted_goal_ref = match requested_conversation_id.as_deref() {
        Some(conversation_id) => load_conversation_goal_ref(&db, conversation_id)?,
        None => None,
    };
    let conversation_goal_ref =
        resolve_conversation_goal_ref(req.goal_ref.as_ref(), persisted_goal_ref)?;
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
    let cancel_token = CancellationToken::new();
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
            db.create_conversation("新对话")
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
    let turn_guard = lifecycle::acquire_turn(&server, &conv_id).await;
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
    let mut runtime = runtime::prepare_with_model(
        &server,
        Some(&conv_id),
        Some(&event_tx),
        req.options.model_profile_id.as_deref(),
    )
    .await
    .map_err(|error| chat_error(StatusCode::BAD_REQUEST, "connector_runtime_invalid", error))?;
    req.options
        .apply(&server, &mut runtime)
        .map_err(|error| chat_error(StatusCode::BAD_REQUEST, "chat_options_invalid", error))?;
    // Finish fallible model/pool setup before accepting the user message or registering a run.
    let llm_client = runtime::client(&server, &runtime, &conv_id, &event_tx)
        .map_err(|error| chat_error(StatusCode::BAD_REQUEST, "connector_model_invalid", error))?;
    let llm_client = req.options.client(llm_client);
    db.save_chat_options(&conv_id, &req.options)
        .map_err(|error| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chat_options_save_failed",
                error,
            )
        })?;
    let config = runtime.config.clone();
    // Build one MCP-aware runtime registry snapshot for this whole chat run.
    // It is used by BOTH the LLM tool definitions and the Security Gateway so
    // the LLM, evaluation, and execution all see the same tool set.
    let tool_registry = runtime.tools.clone();
    let mode = req
        .options
        .workspace_mode
        .unwrap_or(db.conversation_mode(&conv_id).map_err(|e| {
            chat_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "conversation_mode_read_failed",
                e,
            )
        })?);
    db.set_conversation_mode(&conv_id, mode).map_err(|e| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "conversation_mode_save_failed",
            e,
        )
    })?;
    let _ = event_tx
        .send(crate::api::conversation_modes::event(
            &conv_id,
            mode,
            false,
            "当前会话模式",
        ))
        .await;
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

    // Retrieve relevant memories using the current user message as the query.
    // Best-effort enhancement: any retrieval/embedding failure must not block chat.
    let mut memory_mode = "lexical";
    let mut query_embedding: Option<Vec<f32>> = None;
    if !goal_bound
        && !channel_context
        && !(runtime.bindings.restrict_memory && runtime.bindings.restrict_knowledge)
        && config.model.has_embedding()
    {
        let llm = LlmClient::new(&config.model, Arc::clone(&server.secret_resolver));
        match llm.embed(&message).await {
            Ok(vec) => {
                query_embedding = Some(vec);
                memory_mode = "hybrid";
            }
            Err(e) => {
                tracing::warn!(error = %e, "chat query embedding failed, falling back to lexical memory retrieval");
                memory_mode = "lexical_fallback";
            }
        }
    }

    let retrieval_query = RetrieveQuery {
        q: message.clone(),
        top_k: CHAT_MEMORY_TOP_K,
        category: None,
    };
    match db.retrieve_scoped_memories(
        &retrieval_query,
        query_embedding.as_deref(),
        Some(&crate::db::memory_governance::embedding_identity(
            &config.model,
        )),
        Some(&conv_id),
        true,
    ) {
        Ok(mut scored) => {
            scored.retain(|m| {
                (!runtime.bindings.restrict_memory || m.memory.category == "knowledge")
                    && (!runtime.bindings.restrict_knowledge || m.memory.category != "knowledge")
            });
            if !scored.is_empty() {
                server.append_memory_context(&mut system_prompt, &scored);
                tracing::info!(
                    mode = memory_mode,
                    count = scored.len(),
                    "memory context injected"
                );
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "memory retrieval failed; continuing chat without memory context");
        }
    }

    grounding::append(
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

    if let Some(goal_ref) = conversation_goal_ref.as_ref() {
        persist_conversation_goal_ref(&db, &conv_id, goal_ref)?;
    }

    // Create cancel token
    server
        .active_tasks
        .lock()
        .insert(conv_id.clone(), cancel_token.clone());

    // Build agent state from history before persisting the current user message.
    let mut agent_state = AgentState::new(system_prompt);
    agent_state.goal_ref = conversation_goal_ref;
    let history = db.load_chat_messages(&conv_id).map_err(|error| {
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "chat_history_invalid",
            error,
        )
    })?;
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
    let now = chrono::Utc::now().timestamp_millis();
    let user_message_id = uuid::Uuid::new_v4().to_string();
    db.accept_conversation_turn_with_resources(
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
        server.active_tasks.lock().remove(&conv_id);
        chat_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "conversation_turn_write_failed",
            error,
        )
    })?;

    // Auto-title: use first user message (trim to 40 chars)
    let title = truncate_chars(&message, 40);
    let _ = db.update_conversation_title(&conv_id, &title);

    if let Some(run) = goal_run.as_ref() {
        agent_state.add_user_message(run.goal_summary.clone());
    }

    // Add the current user message to the LLM context exactly once.
    agent_state.add_user_message(message.clone());

    // Log chat request
    let msg_preview = truncate_chars(&message, 60);
    server
        .log_buffer
        .push("chat", "api", &format!("收到消息: {}", msg_preview));

    // Spawn agent loop
    let conv_clone = conv_id.clone();
    let config_clone = config.clone();
    let log_buffer = server.log_buffer.clone();

    let db_clone = db.clone();
    let bound_agent = runtime.agent_id.clone();
    tokio::spawn(async move {
        let _turn_guard = turn_guard;
        log_buffer.push("info", "agent", "Agent 循环开始");
        let security_gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            config_clone.sandbox.clone(),
            server.workspace_root.clone(),
            Arc::clone(&tool_registry),
            Arc::new(DefaultVerifier::new(&server.workspace_root)),
            Arc::new(server.audit_recorder.clone()),
        )
        .with_db(Arc::new(db_clone.clone()))
        .with_agent_context(bound_agent)
        .with_permissions_config(server.config.clone())
        .with_grant_enforcement();

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
        let goal_scope = goal_run.as_ref().map(|run| run.scope.clone());
        let mut result = match goal_scope {
            Some(scope) => {
                crate::agent::delegation::EXECUTION_SCOPE
                    .scope(scope, execution)
                    .await
            }
            None => execution.await,
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
        for msg in &agent_state.messages[new_start.min(total_msgs)..] {
            if msg.role == "user" || msg.role == "system" {
                continue;
            }
            if let Err(error) = db_clone.persist_chat_message(&conv_clone, msg, now) {
                result = Err(error);
                break;
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
    });

    // Return SSE stream
    let conv_stream = conv_id.clone();
    let stream = async_stream::stream! {
        yield Ok(Event::default()
            .event("connected")
            .data(serde_json::json!({"conversation_id": conv_stream,"user_message_id":user_message_id}).to_string()));

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
    if !conv_id.is_empty() {
        if let Some(token) = server.active_tasks.lock().get(conv_id) {
            token.cancel();
            return Json(serde_json::json!({"status": "cancelled"}));
        }
    }
    if !conv_id.is_empty() {
        let _guard = lifecycle::acquire_turn(&server, conv_id).await;
        match server.db.cancel_conversation_inputs(conv_id) {
            Ok(true) => return Json(serde_json::json!({"status":"cancelled"})),
            Err(error) => return Json(serde_json::json!({"status":"error","error":error})),
            Ok(false) => {}
        }
    }
    Json(serde_json::json!({"status": "not_found"}))
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
                model: "bound-request-model".into(),
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
        send_chat(
            server.clone(),
            &token,
            json!({"conversation_id":conversation.id,"message":"remember star"}),
        )
        .await;
        profile.persona.tone = "PRECISE NEXT TURN".into();
        save_profile(&server.db, agent.id.as_str(), &profile).unwrap();
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
        send_chat(server.clone(),&token,json!({"conversation_id":conversation.id,"message":"remember star after removing last memory and knowledge"})).await;
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 3);
        for request in requests.iter().take(2) {
            assert_eq!(request["model"], "bound-request-model");
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
            json!({"message": "我开发代码时希望怎么控制改动范围？", "conversation_id": conv.id}),
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
            .contains("与当前请求相关的长期记忆"));
        drop(requests);
        mock_llm.abort();
    }
}
