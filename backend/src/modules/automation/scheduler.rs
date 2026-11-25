use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::{
    app::state::AppServer,
    modules::workflow::{application::run_service, WorkflowRunId},
};

use super::{
    attention, dispatch, file_events,
    model::{next_after, Schedule},
    occurrence, personal,
};

/// Bootstrap restores durable claims before waiting for the next deadline.
pub fn start_scheduler(
    server: Arc<AppServer>,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = server
            .db
            .recover_attention_checks(chrono::Utc::now().timestamp_millis())
        {
            tracing::error!(%error, "attention check recovery failed");
        }
        let previous = match server.db.automation_occurrences_for_recovery() {
            Ok(items) => items,
            Err(error) => {
                tracing::error!(%error, "automation startup recovery failed");
                return;
            }
        };
        for mut item in previous {
            if personal::recover(&server, &mut item).unwrap_or_else(|error| {
                tracing::error!(%error, "personal assistant recovery failed");
                true
            }) {
                continue;
            }
            if let Some(id) = item.robot_run_id.as_deref() {
                match server.db.robot_run(id) {
                    Ok(Some(mut run)) if run.active() => {
                        run.status = "stopped".into();
                        run.error = Some("应用在运行期间退出，本次已中断，不会自动重跑".into());
                        run.updated_at = chrono::Utc::now().timestamp_millis();
                        item.status = "interrupted".into();
                        item.error = run.error.clone();
                        item.updated_at = run.updated_at;
                        if let Err(error) = server
                            .db
                            .save_robot_run(&run)
                            .and_then(|_| server.db.update_automation_occurrence(&item))
                        {
                            tracing::error!(%error,"机器人定时任务恢复失败");
                            return;
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error,"机器人任务记录读取失败");
                        return;
                    }
                    _ => {}
                }
                continue;
            }
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
        let mut next_file_check = tokio::time::Instant::now();
        while !shutdown.is_cancelled() {
            let now = chrono::Utc::now().timestamp_millis();
            let automations = match server.db.list_automations() {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(%error, "automation schedule read failed");
                    tokio::select! {
                        _ = shutdown.cancelled() => break,
                        _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                    }
                    continue;
                }
            };
            let mut next_due: Option<i64> = None;
            let mut retry_claim = false;
            let mut watches_file = false;
            let check_files = tokio::time::Instant::now() >= next_file_check;
            for automation in automations {
                if shutdown.is_cancelled() {
                    return;
                }
                if !automation.enabled || automation.archived {
                    continue;
                }
                if matches!(automation.schedule, Schedule::FileChanged { .. }) {
                    watches_file = true;
                    if check_files {
                        if let Err(error) = file_events::poll(&server, &automation).await {
                            tracing::warn!(%error, automation_id=%automation.id, "file event check failed");
                        }
                    }
                    continue;
                }
                let Some(due) = automation.next_run_at else {
                    continue;
                };
                if due > now {
                    next_due = Some(next_due.map_or(due, |earliest| earliest.min(due)));
                    continue;
                }
                if matches!(automation.schedule, Schedule::Watch { .. }) {
                    let server = Arc::clone(&server);
                    let shutdown = shutdown.clone();
                    tokio::spawn(async move {
                        tokio::select! {
                            result = attention::check(&server, &automation, now) => {
                                if let Err(error) = result {
                                    tracing::warn!(%error, automation_id=%automation.id, "attention check failed");
                                }
                            }
                            _ = shutdown.cancelled() => {}
                        }
                    });
                    continue;
                }
                let missed = startup || now - due > 30_000;
                let next = match next_after(&automation.schedule, &automation.timezone, now) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(%error, automation_id=%automation.id, "automation schedule invalid");
                        continue;
                    }
                };
                if let Some(next) = next {
                    next_due = Some(next_due.map_or(next, |earliest| earliest.min(next)));
                }
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
                    Err(error) => {
                        retry_claim = true;
                        tracing::error!(%error, "automation occurrence claim failed");
                    }
                }
            }
            startup = false;
            let current = tokio::time::Instant::now();
            if watches_file && check_files {
                next_file_check = current + Duration::from_secs(10);
            } else if !watches_file {
                next_file_check = current;
            }
            let timer = next_due.map(|due| {
                let delay = due
                    .saturating_sub(chrono::Utc::now().timestamp_millis())
                    .max(0) as u64;
                current + Duration::from_millis(delay)
            });
            let deadline = [
                timer,
                watches_file.then_some(next_file_check),
                retry_claim.then(|| current + Duration::from_secs(10)),
            ]
            .into_iter()
            .flatten()
            .min();
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = server.db.automation_schedule_changed().notified() => {},
                _ = async {
                    if let Some(deadline) = deadline {
                        tokio::time::sleep_until(deadline).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {}
            }
        }
    })
}
