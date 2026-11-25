pub mod attention;
#[cfg(test)]
mod attention_tests;
mod file_events;
pub mod model;
pub mod personal;
mod scheduler;

pub use scheduler::start_scheduler;

use crate::{
    app::state::AppServer,
    modules::workflow::{
        application::{run_inputs::WorkflowNodeInput, run_service},
        definition::WorkflowNodeConfig,
        WorkflowRunId,
    },
    workspace::{Workspace, WorkspaceId, WorkspaceStatus},
};
use model::{
    next_after, Automation, AutomationInput, AutomationPhase, AutomationRuntimeView,
    AutomationTarget, Occurrence,
};
use std::sync::Arc;

pub fn workspace(server: &AppServer, id: &str) -> Result<Workspace, String> {
    let id = WorkspaceId::new(id).map_err(|e| e.to_string())?;
    let value = server.db.get_workspace(&id)?.ok_or("所选工作空间不存在")?;
    if value.status != WorkspaceStatus::Active {
        return Err("所选工作空间已归档".into());
    }
    Ok(value)
}

pub fn save(
    server: &AppServer,
    id: Option<&str>,
    input: AutomationInput,
) -> Result<Automation, String> {
    if input.name.trim().is_empty() {
        return Err("任务名称不能为空".into());
    }
    if input.name.trim().chars().count() > 120 {
        return Err("任务名称最多 120 字符".into());
    }
    workspace(server, &input.workspace_id)?;
    let target = input
        .target
        .clone()
        .unwrap_or_else(|| AutomationTarget::Workflow {
            graph_id: input.workflow_graph_id.clone(),
        });
    let graph_id = match &target {
        AutomationTarget::Workflow { graph_id } => {
            if server.db.get_workflow_graph(graph_id)?.is_none() {
                return Err("所选工作流不存在".into());
            }
            graph_id.clone()
        }
        AutomationTarget::AgentTurn { mission_id } => {
            let mission = server
                .db
                .robot_mission(mission_id)?
                .ok_or("机器人目标任务不存在")?;
            crate::modules::channel::robot::channel(server, &mission.channel_id)?;
            String::new()
        }
        AutomationTarget::PersonalAssistant { goal, max_runs, .. } => {
            personal::validate(goal, *max_runs)?;
            String::new()
        }
    };
    let now = chrono::Utc::now().timestamp_millis();
    let next = next_after(&input.schedule, &input.timezone, now)?;
    if input.enabled
        && next.is_none()
        && !matches!(input.schedule, model::Schedule::FileChanged { .. })
    {
        return Err("一次性计划时间已过去，请选择未来时间".into());
    }
    let existing = id
        .map(|id| server.db.get_automation(id))
        .transpose()?
        .flatten();
    if id.is_some() && existing.is_none() {
        return Err("时间任务不存在".into());
    }
    if let Some(current) = &existing {
        if current.archived {
            return Err("委托已归档，请先恢复归档再编辑".into());
        }
        if input.expected_revision != Some(current.revision) {
            return Err("时间任务已被修改，请刷新后再保存".into());
        }
    }
    let revision = existing.as_ref().map(|item| item.revision + 1).unwrap_or(1);
    let updated_at = existing
        .as_ref()
        .map(|item| now.max(item.updated_at + 1))
        .unwrap_or(now);
    let mut value = Automation {
        revision,
        id: id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name.trim().into(),
        workflow_graph_id: graph_id,
        target: Some(target),
        workspace_id: input.workspace_id,
        timezone: input.timezone,
        schedule: input.schedule,
        enabled: input.enabled,
        next_run_at: if input.enabled { next } else { None },
        created_at: existing.as_ref().map(|a| a.created_at).unwrap_or(now),
        updated_at,
        file_fingerprint: None,
        attention: None,
        archived: false,
    };
    // Validate the source before prepare_save creates a conversation.
    file_events::prepare(server, &mut value)?;
    attention::prepare(server, &mut value, existing.as_ref(), now)?;
    personal::prepare_save(server, &mut value, existing.as_ref())?;
    server.db.save_automation(&value, id.is_none())?;
    Ok(value)
}

pub fn set_enabled(
    server: &AppServer,
    id: &str,
    enabled: bool,
    expected_revision: u64,
) -> Result<Automation, String> {
    let mut current = server.db.get_automation(id)?.ok_or("时间任务不存在")?;
    if enabled && current.archived {
        return Err("委托已归档，请先恢复归档".into());
    }
    if current.revision != expected_revision {
        return Err("时间任务已被修改，请刷新后再操作".into());
    }
    if !enabled {
        current.revision += 1;
        current.enabled = false;
        attention::stop(&mut current, "用户已暂停关注");
        current.next_run_at = None;
        current.updated_at = chrono::Utc::now()
            .timestamp_millis()
            .max(current.updated_at + 1);
        server.db.save_automation(&current, false)?;
        return Ok(current);
    }
    save(
        server,
        Some(id),
        AutomationInput {
            expected_revision: Some(expected_revision),
            name: current.name,
            workflow_graph_id: current.workflow_graph_id,
            target: current.target,
            workspace_id: current.workspace_id,
            timezone: current.timezone,
            schedule: current.schedule,
            enabled,
        },
    )
}

fn occurrence(
    automation: &Automation,
    scheduled_at: i64,
    manual: bool,
    missed: bool,
) -> Occurrence {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    Occurrence {
        id: id.clone(),
        automation_id: automation.id.clone(),
        automation_revision: automation.revision,
        timezone: automation.timezone.clone(),
        occurrence_key: if manual {
            format!("manual:{id}")
        } else {
            format!("scheduled:{scheduled_at}")
        },
        trigger: if manual { "manual" } else { "scheduled" }.into(),
        scheduled_at,
        workspace_id: automation.workspace_id.clone(),
        workflow_graph_id: automation.workflow_graph_id.clone(),
        run_id: (!missed
            && !matches!(
                automation.target,
                Some(
                    AutomationTarget::AgentTurn { .. } | AutomationTarget::PersonalAssistant { .. }
                )
            ))
        .then_some(id),
        robot_run_id: None,
        robot_channel_id: None,
        conversation_id: None,
        assistant_turn_id: None,
        report: None,
        request_fingerprint: None,
        status: if missed { "skipped" } else { "dispatching" }.into(),
        error: missed.then(|| "已错过执行时间，本次不自动补跑".into()),
        created_at: now,
        updated_at: now,
    }
}

pub fn runtime_view(server: &AppServer, id: &str) -> Result<AutomationRuntimeView, String> {
    let item = server.db.automation_runtime_occurrence(id)?;
    Ok(AutomationRuntimeView {
        phase: item.as_ref().map_or(AutomationPhase::Suspended, |item| {
            AutomationPhase::from_status(&item.status)
        }),
        last_woke_at: item.as_ref().map(|item| item.created_at),
        trigger: item.as_ref().map(|item| item.trigger.clone()),
        latest_status: item.map(|item| item.status),
    })
}

fn publish_runtime(server: &AppServer, item: &Occurrence) {
    let _ = server.event_hub.publish(crate::shared::event::YiEvent::new(
        "automation.runtime.updated",
        "automation",
        serde_json::json!({ "automation_id": item.automation_id,
            "phase": AutomationPhase::from_status(&item.status), "status": item.status,
            "last_woke_at": item.created_at, "trigger": item.trigger }),
    ));
}

fn persist_runtime(server: &AppServer, item: &Occurrence) -> Result<(), String> {
    server.db.update_automation_occurrence(item)?;
    publish_runtime(server, item);
    Ok(())
}

async fn dispatch(
    server: &Arc<AppServer>,
    automation: &Automation,
    occurrence: &mut Occurrence,
) -> Result<(), String> {
    if matches!(
        automation.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return personal::dispatch(server, automation, occurrence).await;
    }
    let _guard = crate::api::chat::lifecycle::acquire_turn(
        server,
        &format!("automation-dispatch:{}", automation.id),
    )
    .await;
    if !server
        .db
        .automation_runtime_occurrence(&automation.id)?
        .is_some_and(|item| item.id == occurrence.id && item.status == "dispatching")
    {
        return Ok(());
    }
    if !server
        .db
        .get_automation(&automation.id)?
        .is_some_and(|current| current.revision == automation.revision)
    {
        occurrence.status = "blocked".into();
        occurrence.run_id = None;
        occurrence.error = Some("任务计划已变更，本次未启动".into());
        return persist_runtime(server, occurrence);
    }
    publish_runtime(server, occurrence);
    if let Some(AutomationTarget::AgentTurn { mission_id }) = &automation.target {
        let result = async {
            workspace(server, &automation.workspace_id)?;
            let mut mission = server
                .db
                .robot_mission(mission_id)?
                .ok_or("机器人目标任务已删除")?;
            mission.workspace_id = Some(automation.workspace_id.clone());
            crate::modules::channel::robot::start_mission(server.clone(), mission, "automation")
                .await
        }
        .await;
        match result {
            Ok(run) => {
                occurrence.robot_run_id = Some(run.id);
                occurrence.robot_channel_id = Some(run.channel_id);
                occurrence.conversation_id = Some(run.conversation_id);
                occurrence.run_id = None;
                occurrence.status = run.status;
            }
            Err(error) => {
                occurrence.status = "failed".into();
                occurrence.error = Some(error);
            }
        }
        occurrence.updated_at = chrono::Utc::now().timestamp_millis();
        return persist_runtime(server, occurrence);
    }
    let result = async {
        let workspace = workspace(server, &automation.workspace_id)?;
        let graph = server
            .db
            .get_workflow_graph(&automation.workflow_graph_id)?
            .ok_or("所选工作流不存在")?;
        let context = format!(
            "\n\n工作空间上下文：{}\n工作空间ID：{}\n根目录提示：{}",
            workspace.name,
            workspace.id,
            workspace.root_path.as_deref().unwrap_or("未设置")
        );
        let mut inputs = std::collections::BTreeMap::new();
        for node in graph.definition.nodes {
            match node.config {
                WorkflowNodeConfig::Agent { prompt } => {
                    inputs.insert(
                        node.id.to_string(),
                        WorkflowNodeInput::Agent {
                            prompt: format!("{prompt}{context}"),
                        },
                    );
                }
                WorkflowNodeConfig::Subagent { task, .. } => {
                    inputs.insert(
                        node.id.to_string(),
                        WorkflowNodeInput::Subagent {
                            task: format!("{task}{context}"),
                        },
                    );
                }
                _ => {}
            }
        }
        run_service::start_workflow_run_identified(
            server,
            &automation.workflow_graph_id,
            inputs,
            WorkflowRunId::new(occurrence.id.clone()).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())
    }
    .await;
    match result {
        Ok(started) => {
            occurrence.run_id = Some(started.run_id);
            occurrence.status = started.status;
        }
        Err(error) => {
            occurrence.run_id = None;
            occurrence.status = "failed".into();
            occurrence.error = Some(error);
        }
    }
    occurrence.updated_at = chrono::Utc::now().timestamp_millis();
    persist_runtime(server, occurrence)
}

pub async fn stop(server: &Arc<AppServer>, id: &str) -> Result<Automation, String> {
    let automation = server.db.get_automation(id)?.ok_or("定时任务不存在")?;
    if matches!(
        automation.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return personal::stop(server, id).await;
    }
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(server, &format!("automation-dispatch:{id}"))
            .await;
    let automation = server.db.get_automation(id)?.ok_or("定时任务不存在")?;
    let paused = set_enabled(server, id, false, automation.revision)?;
    if let Some(mut item) = server.db.automation_runtime_occurrence(id)? {
        if AutomationPhase::from_status(&item.status) == AutomationPhase::Suspended {
            return Ok(paused);
        }
        if item.status == "dispatching" {
            item.run_id = None;
            item.status = "cancelled".into();
        } else if let Some(run) = &item.run_id {
            item.status = run_service::cancel_workflow_run(server, run)
                .await
                .map_err(|error| error.to_string())?
                .1
                .status
                .to_string();
        } else if let Some(run) = &item.robot_run_id {
            item.status = crate::modules::channel::robot::stop(server, run)
                .await?
                .status;
        }
        item.updated_at = chrono::Utc::now().timestamp_millis();
        persist_runtime(server, &item)?;
    }
    Ok(paused)
}

pub async fn run_now(server: &Arc<AppServer>, id: &str) -> Result<Occurrence, String> {
    let automation = server.db.get_automation(id)?.ok_or("时间任务不存在")?;
    let _management_guard = if matches!(
        automation.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        Some(
            crate::api::chat::lifecycle::acquire_turn(server, &format!("personal-management:{id}"))
                .await,
        )
    } else {
        None
    };
    run_now_managed(server, id).await
}

/// The HTTP manager holds the per-delegation gate across preflight and claim.
pub(crate) async fn run_now_managed(
    server: &Arc<AppServer>,
    id: &str,
) -> Result<Occurrence, String> {
    let automation = server.db.get_automation(id)?.ok_or("时间任务不存在")?;
    if automation.archived {
        return Err("委托已归档，请先恢复归档".into());
    }
    let mut item = occurrence(
        &automation,
        chrono::Utc::now().timestamp_millis(),
        true,
        false,
    );
    if !server.db.claim_automation_occurrence(
        &automation,
        automation.next_run_at,
        &mut item,
        true,
    )? {
        return Err("任务配置已变更，请刷新后再运行".into());
    }
    if item.status == "dispatching" {
        dispatch(server, &automation, &mut item).await?;
    }
    Ok(item)
}

pub fn history(server: &AppServer, id: &str) -> Result<Vec<Occurrence>, String> {
    let mut items = server.db.automation_history(id)?;
    for item in &mut items {
        if item.status == "interrupted" {
            continue;
        }
        if let Some(run_id) = item.robot_run_id.as_deref() {
            if let Some(run) = server.db.robot_run(run_id)? {
                item.status = run.status;
                item.error = run.error;
                item.updated_at = run.updated_at;
                item.robot_channel_id = Some(run.channel_id);
                item.conversation_id = Some(run.conversation_id);
            } else {
                item.status = "failed".into();
                item.error = Some("关联的机器人运行已删除".into());
            }
            continue;
        }
        if let Some(run_id) = &item.run_id {
            let id = WorkflowRunId::new(run_id.clone()).map_err(|e| e.to_string())?;
            if let Some(stored) = server.db.get_workflow_run(&id)? {
                item.status = stored.run.status.to_string();
                item.updated_at = stored.run.updated_at;
                item.error = stored
                    .run
                    .node_states
                    .iter()
                    .find_map(|node| node.error.clone());
            }
        }
    }
    Ok(items)
}
