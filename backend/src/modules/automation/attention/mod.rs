mod decision;
mod source;
mod types;

pub use types::{
    AttentionAction, AttentionState, AttentionStatus, FactReference, ObservationStatus,
    SourceObservation, WatchSource,
};
use types::{AttentionDecision, AttentionProposal};

use std::{path::Path, sync::Arc};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::app::state::AppServer;

use super::model::{Automation, AutomationTarget, Schedule};
use source::SourceFailure;

pub(super) fn prepare(
    server: &AppServer,
    automation: &mut Automation,
    previous: Option<&Automation>,
    now: i64,
) -> Result<(), String> {
    let Schedule::Watch {
        source,
        min_interval_secs,
        expires_at,
        max_checks,
        max_retries,
    } = &automation.schedule
    else {
        return Ok(());
    };
    if !matches!(
        automation.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return Err("主动关注仅可关联个人主动助手".into());
    }
    if *min_interval_secs == 0 || *max_checks == 0 || max_retries > max_checks {
        return Err("请设置正数复查间隔、检查上限和有效重试上限".into());
    }
    if automation.enabled && *expires_at <= now {
        return Err("关注截止时间须在未来".into());
    }
    match source {
        WatchSource::Http { url, .. } => {
            crate::safety::grant::parse_network_target(url)
                .map_err(|_| "请提供有效且无账号凭据的 HTTP 或 HTTPS 网址")?;
        }
        WatchSource::File { path } => {
            let workspace = super::workspace(server, &automation.workspace_id)?;
            let saved_root = previous
                .filter(|old| old.workspace_id == automation.workspace_id)
                .and_then(|old| match old.target.as_ref() {
                    Some(AutomationTarget::PersonalAssistant { root_path, .. }) => {
                        Some(root_path.as_str())
                    }
                    _ => None,
                });
            let root = Path::new(saved_root.unwrap_or_else(|| {
                workspace
                    .root_path
                    .as_deref()
                    .unwrap_or(&server.workspace_root)
            }))
            .canonicalize()
            .map_err(|error| error.to_string())?;
            super::file_events::open_source(&root, path)?;
        }
    }
    let mut state = previous
        .filter(|old| {
            old.workspace_id == automation.workspace_id
                && matches!(&old.schedule, Schedule::Watch { source: old, .. } if old.same_location(source))
        })
        .and_then(|old| old.attention.clone())
        .unwrap_or_else(AttentionState::waiting);
    state.pending_check = None;
    if automation.enabled {
        if state.checks_used >= *max_checks {
            return Err("关注检查预算已用尽，请编辑增加检查上限".into());
        }
        state.status = AttentionStatus::Waiting;
        state.reason = "已安排下一次复查".into();
    } else {
        state.stop("用户已暂停关注");
    }
    automation.attention = Some(state);
    Ok(())
}

pub(super) fn stop(automation: &mut Automation, reason: &str) {
    if let Some(state) = automation.attention.as_mut() {
        state.stop(reason);
    }
}

pub(super) async fn check(
    server: &Arc<AppServer>,
    expected: &Automation,
    now: i64,
) -> Result<(), String> {
    let Some(automation) = server.db.claim_attention_check(expected, now)? else {
        return Ok(());
    };
    let Schedule::Watch {
        source,
        min_interval_secs,
        expires_at,
        max_checks,
        max_retries,
    } = &automation.schedule
    else {
        unreachable!("only watch checks can be claimed")
    };
    let Some(AutomationTarget::PersonalAssistant {
        goal,
        conversation_id,
        root_path,
        ..
    }) = automation.target.as_ref()
    else {
        return Err("关注未关联个人委托".into());
    };
    let mut state = automation.attention.clone().expect("claim creates state");
    let check_id = state
        .pending_check
        .as_ref()
        .expect("claim creates check")
        .clone();
    let (source_kind, location) = source.description();
    let sample = match super::workspace(server, &automation.workspace_id) {
        Ok(_) => source::collect(server, source, root_path, conversation_id, &check_id).await,
        Err(reason) => Err(SourceFailure::Blocked(reason)),
    };
    if !server
        .db
        .get_automation(&automation.id)?
        .is_some_and(|current| {
            current.enabled
                && current.revision == automation.revision
                && current
                    .attention
                    .and_then(|state| state.pending_check)
                    .as_deref()
                    == Some(&check_id)
        })
    {
        return Ok(());
    }
    let observed_at = now.max(chrono::Utc::now().timestamp_millis());
    state.last_checked_at = Some(observed_at);
    let mut evidence = SourceObservation {
        id: check_id,
        automation_id: automation.id.clone(),
        automation_revision: automation.revision,
        workspace_id: automation.workspace_id.clone(),
        source_kind: source_kind.into(),
        source: location,
        authority: "local-user".into(),
        source_at: None,
        observed_at,
        fingerprint: None,
        http_status: None,
        facts: vec![],
        status: ObservationStatus::Unknown,
        previous_observation_id: state.observation_id.clone(),
    };
    let mut log = true;
    let mut action = None;
    let previous_decision = state.decision;
    let previous_proposal = state.proposal.clone();
    let delay = if observed_at >= *expires_at {
        log = false;
        state.stop("关注已到截止时间");
        None
    } else {
        match sample {
            Err(SourceFailure::Blocked(reason)) => {
                state.stop(reason);
                state.status = AttentionStatus::Blocked;
                None
            }
            Err(SourceFailure::Unavailable(reason)) => {
                retry(&mut state, reason, *min_interval_secs, *max_retries)
            }
            Ok(sample) => {
                evidence.source_at = sample.source_at;
                evidence.http_status = sample.http_status;
                evidence.fingerprint = sample.fingerprint;
                let failure = evidence
                    .http_status
                    .filter(|status| !(200..300).contains(status));
                if failure.is_none() {
                    evidence.status = ObservationStatus::Captured;
                }
                let out_of_order = evidence.fingerprint.is_some()
                    && evidence.fingerprint != state.fingerprint
                    && sample
                        .source_at
                        .zip(state.source_at)
                        .is_some_and(|(incoming, previous)| incoming < previous);
                if out_of_order {
                    evidence.status = ObservationStatus::Conflict;
                    state.status = AttentionStatus::Conflict;
                    state.reason = "来源时间早于已确认的观察，等待复查".into();
                    Some(wait_longer(&mut state, *min_interval_secs))
                } else {
                    if failure.is_none() {
                        state.fingerprint = evidence.fingerprint.clone();
                        state.source_at = sample.source_at;
                        state.last_observed_at = Some(observed_at);
                    }
                    let (refs, facts) =
                        current_facts(server, &automation.id, conversation_id, source)
                            .unwrap_or_else(|_| (vec![], json!({"availability":"unknown"})));
                    let key = format!(
                        "{:x}",
                        Sha256::digest(
                            json!({
                                "fingerprint": evidence.fingerprint,
                                "http_status": evidence.http_status,
                                "facts": facts,
                            })
                            .to_string()
                        )
                    );
                    let unchanged = state.decision_key.as_ref() == Some(&key);
                    state.facts = refs.clone();
                    evidence.facts = refs;
                    if unchanged && !state.decision_pending {
                        log = false;
                        if let Some(status) = failure {
                            retry(
                                &mut state,
                                format!("来源仍返回 HTTP {status}"),
                                *min_interval_secs,
                                *max_retries,
                            )
                        } else {
                            state.status = AttentionStatus::Unchanged;
                            state.reason = "来源及相关任务状态暂无新证据，延后复查".into();
                            Some(wait_longer(&mut state, *min_interval_secs))
                        }
                    } else {
                        if unchanged {
                            log = false;
                            evidence.id = state
                                .observation_id
                                .clone()
                                .expect("decision key has observation");
                        } else {
                            state.decision_key = Some(key);
                            state.observation_id = Some(evidence.id.clone());
                        }
                        if observed_at.max(chrono::Utc::now().timestamp_millis()) >= *expires_at {
                            state.stop("关注已到截止时间");
                            None
                        } else {
                            let result = decision::decide(
                                server,
                                conversation_id,
                                goal,
                                &evidence,
                                sample.preview.as_deref(),
                                facts,
                                *expires_at,
                            )
                            .await;
                            if observed_at.max(chrono::Utc::now().timestamp_millis()) >= *expires_at
                            {
                                state.stop("关注已到截止时间");
                                None
                            } else {
                                match result {
                                    Ok(decision) => {
                                        state.decision_pending = false;
                                        action = Some(decision.action);
                                        let next = apply_decision(
                                            &mut state,
                                            decision,
                                            &evidence.id,
                                            *min_interval_secs,
                                        );
                                        match (failure, next) {
                                            (Some(status), Some(delay)) => {
                                                schedule_failed_response(
                                                    &mut state,
                                                    status,
                                                    delay,
                                                    *max_retries,
                                                )
                                            }
                                            _ => next,
                                        }
                                    }
                                    Err(reason) => {
                                        state.decision_pending = true;
                                        retry(&mut state, reason, *min_interval_secs, *max_retries)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    let finish_at = observed_at.max(chrono::Utc::now().timestamp_millis());
    let next = delay
        .map(|seconds| {
            finish_at.saturating_add(seconds.saturating_mul(1000).min(i64::MAX as u64) as i64)
        })
        .filter(|due| *due < *expires_at && state.checks_used < *max_checks);
    if finish_at >= *expires_at {
        state.stop("关注已到截止时间");
        state.decision = previous_decision;
        state.proposal = previous_proposal;
        action = None;
    } else if next.is_none() && delay.is_some() {
        state.reason.push_str("。下次复查超出关注期限或检查上限。");
    }
    let mut item = (action == Some(AttentionAction::Act)).then(|| {
        let mut item = super::occurrence(&automation, finish_at, false, false);
        item.trigger = "attention".into();
        item.occurrence_key = format!("attention:{}", evidence.id);
        item.report = Some(format!(
            "来源 {}；观察 {}；{}。请沿用已保存的目标与授权范围。",
            evidence.source, evidence.id, state.reason
        ));
        item
    });
    let committed = server.db.settle_attention_check(
        &automation,
        state,
        next,
        log.then_some(&evidence),
        item.as_ref(),
    )?;
    if committed.is_some() {
        let _ = server.event_hub.publish(crate::shared::event::YiEvent::new(
            "automation.attention.updated",
            "automation",
            json!({"automation_id": automation.id}),
        ));
    }
    if committed == Some(true) {
        super::personal::dispatch(
            server,
            &automation,
            item.as_mut().expect("queued occurrence"),
        )
        .await?;
    }
    Ok(())
}

fn schedule_failed_response(
    state: &mut AttentionState,
    status: u16,
    delay: u64,
    max_retries: u32,
) -> Option<u64> {
    state.reason = format!("来源返回 HTTP {status}。{}", state.reason);
    if state.status == AttentionStatus::Deferred {
        state.status = AttentionStatus::Unknown;
    }
    if state.retries_used >= max_retries {
        state.reason.push_str("。复查重试次数已用完。");
        return None;
    }
    state.retries_used += 1;
    Some(delay)
}

fn apply_decision(
    state: &mut AttentionState,
    decision: AttentionDecision,
    observation: &str,
    minimum: u64,
) -> Option<u64> {
    state.decision = Some(decision.action);
    state.reason = decision.reason;
    state.interval_secs = decision.recheck_in_secs.unwrap_or(minimum).max(minimum);
    state.status = match decision.action {
        AttentionAction::Defer => AttentionStatus::Deferred,
        AttentionAction::Stop => AttentionStatus::Stopped,
        AttentionAction::Act => AttentionStatus::ActionQueued,
        AttentionAction::Propose => {
            state.proposal = decision.proposal.map(|goal| AttentionProposal {
                goal,
                observation_id: observation.into(),
            });
            AttentionStatus::Proposed
        }
    };
    (decision.action != AttentionAction::Stop).then_some(state.interval_secs)
}

fn retry(
    state: &mut AttentionState,
    reason: String,
    minimum: u64,
    max_retries: u32,
) -> Option<u64> {
    if state.retries_used >= max_retries {
        state.stop(format!("{reason}，复查重试次数已用完"));
        return None;
    }
    state.retries_used += 1;
    state.status = AttentionStatus::Unknown;
    state.reason = reason;
    Some(wait_longer(state, minimum))
}

fn wait_longer(state: &mut AttentionState, minimum: u64) -> u64 {
    state.interval_secs = state.interval_secs.saturating_mul(2).max(minimum);
    state.interval_secs
}

fn current_facts(
    server: &AppServer,
    automation: &str,
    conversation: &str,
    source: &WatchSource,
) -> Result<(Vec<FactReference>, Value), String> {
    let latest = super::personal::attempts(server, automation)?
        .into_iter()
        .find(|item| !matches!(item.status.as_str(), "skipped" | "blocked"));
    let goal = server.db.load_conversation_goal_binding(conversation)?;
    let mut refs = vec![FactReference {
        kind: "conversation".into(),
        id: conversation.into(),
    }];
    if let Some(item) = &latest {
        refs.push(FactReference {
            kind: "automation_occurrence".into(),
            id: item.id.clone(),
        });
    }
    if let Some(binding) = &goal {
        refs.push(FactReference {
            kind: "goal_version".into(),
            id: format!("{}:{}", binding.goal_id, binding.goal_version),
        });
    }
    let service = match source {
        WatchSource::Http { url, .. } => server.plugin_services.attention_service_for_url(url)?,
        WatchSource::File { .. } => None,
    };
    if let Some((id, instance, _)) = &service {
        refs.push(FactReference {
            kind: "plugin_service".into(),
            id: instance.map_or_else(|| id.clone(), |instance| format!("{id}#{instance}")),
        });
    }
    let facts = json!({
        "last_occurrence": latest.map(|item| json!({"id":item.id,"status":item.status})),
        "conversation_status": server.db.conversation_run_status(conversation)?.map(|status| status.as_str()),
        "goal_binding": goal.map(|binding| json!({
            "goal_id": binding.goal_id,
            "goal_version": binding.goal_version,
        })),
        "plugin_service": service.map(|(id, instance, health)| json!({
            "id": id,
            "instance_id": instance.map(|id| id.to_string()),
            "health": health,
        })),
    });
    Ok((refs, facts))
}
