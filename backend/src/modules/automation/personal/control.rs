//! User management commands: new turns are explicit and idempotent. A failed
//! occurrence is never replayed; its effects remain available for review.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartInput {
    pub expected_revision: u64,
    pub request_id: String,
    #[serde(default)]
    pub reviewed_occurrence_id: Option<String>,
    #[serde(default)]
    pub acknowledged: bool,
}

pub async fn start(
    server: &Arc<AppServer>,
    id: &str,
    input: StartInput,
) -> Result<Occurrence, String> {
    uuid::Uuid::parse_str(&input.request_id).map_err(|_| "请求标识无效，请刷新后重新操作")?;
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(server, &format!("personal-management:{id}"))
            .await;
    let key = format!("management:{}", input.request_id);
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&input).map_err(|e| e.to_string())?)
    );
    let existing: Option<String> = server.db.conn().query_row(
        "SELECT payload FROM automation_occurrences WHERE automation_id=?1 AND occurrence_key=?2", params![id, key], |row| row.get(0),
    ).optional().map_err(|e| e.to_string())?;
    if let Some(existing) = existing {
        let item: Occurrence = serde_json::from_str(&existing).map_err(|e| e.to_string())?;
        if item.request_fingerprint.as_deref() != Some(&hash) {
            return Err("该请求标识已用于不同参数；请刷新核实上次结果，不要重复启动".into());
        }
        return Ok(item);
    }
    let automation = server.db.get_automation(id)?.ok_or("个人委托不存在")?;
    if !matches!(
        automation.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return Err("此入口仅用于个人委托".into());
    }
    if input.expected_revision != automation.revision {
        return Err("委托版本已变化，请刷新后再操作".into());
    }
    let state = super::management::detail(server, id)?;
    if state["actions"]["can_start"] != true {
        let issues = state["readiness"]["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|issue| issue["severity"] == "error")
            .filter_map(|issue| issue["message"].as_str())
            .collect::<Vec<_>>()
            .join("；");
        return Err(if issues.is_empty() {
            "当前状态不能启动：请检查运行、等待、归档或预算情况".into()
        } else {
            issues
        });
    }
    if state["actions"]["requires_review"] == true {
        let expected = state["actions"]["review_occurrence_id"].as_str();
        if !input.acknowledged || input.reviewed_occurrence_id.as_deref() != expected {
            return Err("请先核实所示轮次的文件和执行记录，明确确认后再继续新一轮".into());
        }
    }
    let mut item = super::super::occurrence(
        &automation,
        chrono::Utc::now().timestamp_millis(),
        true,
        false,
    );
    item.occurrence_key = key;
    item.request_fingerprint = Some(hash);
    if !server.db.claim_automation_occurrence(
        &automation,
        automation.next_run_at,
        &mut item,
        true,
    )? {
        return Err("委托已变更或已有轮次开始，请刷新，不会重复启动".into());
    }
    super::dispatch(server, &automation, &mut item).await?;
    Ok(item)
}

pub async fn archive(server: &Arc<AppServer>, id: &str) -> Result<Automation, String> {
    let _guard =
        crate::api::chat::lifecycle::acquire_turn(server, &format!("personal-management:{id}"))
            .await;
    let current = server.db.get_automation(id)?.ok_or("个人委托不存在")?;
    if !matches!(
        current.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return Err("此入口仅用于个人委托".into());
    }
    if current.archived {
        return Ok(current);
    }
    let mut value = super::stop(server, id).await?;
    value.archived = true;
    value.revision += 1;
    value.updated_at = chrono::Utc::now()
        .timestamp_millis()
        .max(value.updated_at + 1);
    server.db.save_automation(&value, false)?;
    Ok(value)
}

pub fn restore(server: &AppServer, id: &str, expected_revision: u64) -> Result<Automation, String> {
    let mut value = server.db.get_automation(id)?.ok_or("个人委托不存在")?;
    if !matches!(
        value.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return Err("此入口仅用于个人委托".into());
    }
    if !value.archived {
        return Ok(value);
    }
    if value.revision != expected_revision {
        return Err("委托版本已变化，请刷新后再操作".into());
    }
    value.archived = false;
    value.enabled = false;
    value.next_run_at = None;
    value.revision += 1;
    value.updated_at = chrono::Utc::now()
        .timestamp_millis()
        .max(value.updated_at + 1);
    server.db.save_automation(&value, false)?;
    Ok(value)
}
