pub mod model;

use crate::{
    app::state::AppServer,
    modules::workflow::{
        application::{run_inputs::WorkflowNodeInput, run_service},
        definition::WorkflowNodeConfig,
        WorkflowRunId,
    },
    workspace::{Workspace, WorkspaceId, WorkspaceStatus},
};
use model::{next_after, Automation, AutomationInput, Occurrence};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

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
    workspace(server, &input.workspace_id)?;
    if server
        .db
        .get_workflow_graph(&input.workflow_graph_id)?
        .is_none()
    {
        return Err("所选工作流不存在".into());
    }
    let now = chrono::Utc::now().timestamp_millis();
    let next = next_after(&input.schedule, &input.timezone, now)?;
    if input.enabled && next.is_none() {
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
        if input.expected_revision != Some(current.revision) {
            return Err("时间任务已被修改，请刷新后再保存".into());
        }
    }
    let revision = existing.as_ref().map(|item| item.revision + 1).unwrap_or(1);
    let updated_at = existing
        .as_ref()
        .map(|item| now.max(item.updated_at + 1))
        .unwrap_or(now);
    let value = Automation {
        revision,
        id: id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name.trim().into(),
        workflow_graph_id: input.workflow_graph_id,
        workspace_id: input.workspace_id,
        timezone: input.timezone,
        schedule: input.schedule,
        enabled: input.enabled,
        next_run_at: if input.enabled { next } else { None },
        created_at: existing.map(|a| a.created_at).unwrap_or(now),
        updated_at,
    };
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
    if current.revision != expected_revision {
        return Err("时间任务已被修改，请刷新后再操作".into());
    }
    if !enabled {
        current.revision += 1;
        current.enabled = false;
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
        run_id: (!missed).then_some(id),
        status: if missed { "skipped" } else { "dispatching" }.into(),
        error: missed.then(|| "已错过执行时间，本次不自动补跑".into()),
        created_at: now,
        updated_at: now,
    }
}

async fn dispatch(
    server: &AppServer,
    automation: &Automation,
    occurrence: &mut Occurrence,
) -> Result<(), String> {
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
    server.db.update_automation_occurrence(occurrence)
}

pub async fn run_now(server: &AppServer, id: &str) -> Result<Occurrence, String> {
    let automation = server.db.get_automation(id)?.ok_or("时间任务不存在")?;
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

/// Bootstrap calls this once after database migrations. Shutdown cancels future
/// ticks; running workflows retain their existing cancellation and approval UI.
pub fn start_scheduler(
    server: Arc<AppServer>,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let previous = match server.db.automation_occurrences_for_recovery() {
            Ok(items) => items,
            Err(error) => {
                tracing::error!(%error, "automation startup recovery failed");
                return;
            }
        };
        for mut item in previous {
            let mut interrupted = item.status == "dispatching";
            if let Some(run_id) = &item.run_id {
                let result = async {
                    let id = WorkflowRunId::new(run_id.clone()).map_err(|e| e.to_string())?;
                    if let Some(stored) = server.db.get_workflow_run(&id)? {
                        if !stored.run.status.is_terminal() {
                            run_service::cancel_workflow_run(&server, run_id)
                                .await
                                .map_err(|e| e.to_string())?;
                            return Ok::<(bool, bool), String>((true, true));
                        }
                        return Ok((false, true));
                    }
                    Ok((false, false))
                }
                .await;
                match result {
                    Ok((active, exists)) => {
                        interrupted |= active;
                        if !exists {
                            item.run_id = None;
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "automation run recovery failed");
                        return;
                    }
                }
            }
            if interrupted {
                item.status = "interrupted".into();
                item.error = Some("应用在运行期间退出，本次已中断，不会自动重跑".into());
                item.updated_at = chrono::Utc::now().timestamp_millis();
                if let Err(error) = server.db.update_automation_occurrence(&item) {
                    tracing::error!(%error, "automation recovery persistence failed");
                    return;
                }
            }
        }
        let mut startup = true;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { _ = shutdown.cancelled() => break, _ = interval.tick() => {} }
            let now = chrono::Utc::now().timestamp_millis();
            let automations = match server.db.list_automations() {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(%error, "automation schedule read failed");
                    continue;
                }
            };
            for automation in automations {
                if !automation.enabled {
                    continue;
                }
                let Some(due) = automation.next_run_at.filter(|due| *due <= now) else {
                    continue;
                };
                let missed = startup || now - due > 30_000;
                let next = match next_after(&automation.schedule, &automation.timezone, now) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(%error, automation_id=%automation.id, "automation schedule invalid");
                        continue;
                    }
                };
                let mut item = occurrence(&automation, due, false, missed);
                match server
                    .db
                    .claim_automation_occurrence(&automation, next, &mut item, false)
                {
                    Ok(true) if item.status == "dispatching" => {
                        let server = Arc::clone(&server);
                        tokio::spawn(async move {
                            if let Err(error) = dispatch(&server, &automation, &mut item).await {
                                tracing::error!(%error, "automation dispatch persistence failed");
                            }
                        });
                    }
                    Ok(_) => {}
                    Err(error) => tracing::error!(%error, "automation occurrence claim failed"),
                }
            }
            startup = false;
        }
    })
}
