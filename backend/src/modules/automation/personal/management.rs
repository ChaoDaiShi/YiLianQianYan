//! Read-only management projection over existing delegations, turns and tools.
//! It neither calls a provider nor reads/writes the OS credential store.
mod files;
mod readiness;
#[cfg(test)]
mod tests;

use super::super::model::{Automation, AutomationTarget, Occurrence};
use crate::app::state::AppServer;
use files::lexical_relative;
pub use files::preview;
use readiness::{issue, readiness};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;

const PAGE_SIZE: usize = 25;

fn delegation(server: &AppServer, id: &str) -> Result<Automation, String> {
    let automation = server.db.get_automation(id)?.ok_or("个人委托不存在")?;
    if !matches!(
        automation.target,
        Some(AutomationTarget::PersonalAssistant { .. })
    ) {
        return Err("此入口仅用于个人委托".into());
    }
    Ok(automation)
}

fn execution_rows(
    server: &AppServer,
    conversation: &str,
    pending_only: bool,
) -> Result<Vec<Value>, String> {
    let conn = server.db.conn();
    let mut statement = conn.prepare(
        "SELECT tool_call_id,name,CASE WHEN json_type(args_json,'$.path')='text' THEN json_extract(args_json,'$.path') ELSE NULL END,
         substr(CASE WHEN name='write_file' THEN json_remove(args_json,'$.content') ELSE args_json END,1,1000),
         execution_status,approval_status,verification_status,verification_reason,substr(result,1,4000),started_at,finished_at
         FROM conversation_execution_records WHERE conversation_id=?1 AND (?2=0 OR approval_status='pending')
         ORDER BY sequence DESC,tool_call_id DESC LIMIT 50"
    ).map_err(|e|e.to_string())?;
    let rows = statement.query_map(params![conversation, pending_only], |row| {
        Ok(json!({"tool_call_id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,
            "path":row.get::<_,Option<String>>(2)?,"arguments_preview":row.get::<_,String>(3)?,
            "execution_status":row.get::<_,String>(4)?,"approval_status":row.get::<_,String>(5)?,
            "verification_status":row.get::<_,String>(6)?,"verification_reason":row.get::<_,Option<String>>(7)?,
            "result":row.get::<_,Option<String>>(8)?,"started_at":row.get::<_,Option<i64>>(9)?,
            "finished_at":row.get::<_,Option<i64>>(10)?}))
    }).map_err(|e|e.to_string())?;
    rows.map(|row| row.map_err(|e| e.to_string())).collect()
}

fn files(server: &AppServer, conversation: &str, root: &str) -> Result<Vec<Value>, String> {
    let conn = server.db.conn();
    let mut statement = conn.prepare("SELECT tool_call_id,CASE WHEN json_type(args_json,'$.path')='text' THEN json_extract(args_json,'$.path') ELSE NULL END,execution_status,verification_status,started_at FROM conversation_execution_records WHERE conversation_id=?1 AND name='write_file' AND execution_status='succeeded' ORDER BY sequence DESC,tool_call_id DESC LIMIT 200").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([conversation], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut values = Vec::new();
    for row in rows {
        let (call, path, execution, verification, created) = row.map_err(|e| e.to_string())?;
        let Some(path) = path else {
            continue;
        };
        // This projection does not open files or resolve paths supplied by old records.
        // Preview applies the full current directory and permission boundary.
        let relative = lexical_relative(Path::new(root), Path::new(&path))
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| "目录范围需重新核实".into());
        values.push(
            json!({"tool_call_id":call,"path":path,"relative_path":relative,
            "execution_status":execution,"verification_status":verification,"created_at":created}),
        );
    }
    Ok(values)
}

pub fn detail(server: &AppServer, id: &str) -> Result<Value, String> {
    let automation = delegation(server, id)?;
    let Some(AutomationTarget::PersonalAssistant {
        conversation_id,
        root_path,
        max_runs,
        allow_file_discovery,
        allow_file_writes,
        ..
    }) = &automation.target
    else {
        unreachable!()
    };
    let activity = super::activity(server, id)?;
    let items = super::attempts(server, id)?;
    let used = activity["used_runs"].as_u64().unwrap_or(0);
    let (run_status, run_error): (String, Option<String>) = server
        .db
        .conn()
        .query_row(
            "SELECT run_status,run_error FROM conversations WHERE id=?1",
            [conversation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("任务对话无法读取：{e}"))?;
    let active_occurrence = items.iter().any(|item| super::active(&item.status));
    let pending_approvals = execution_rows(server, conversation_id, true)?;
    // Restart recovery interrupts the occurrence, but persisted approval records
    // remain unresolved until stop clears them. They must not become new work.
    let pending_recovery = !active_occurrence && !pending_approvals.is_empty();
    let protocol_recovery = !active_occurrence
        && server
            .db
            .conversation_protocol_issue(conversation_id)?
            .is_some();
    let unfinished_execution = !active_occurrence && server.db.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM conversation_execution_records WHERE conversation_id=?1 AND execution_status IN ('running','queued','awaiting_approval'))",
        [conversation_id], |row|row.get::<_,bool>(0)).map_err(|e|e.to_string())?;
    let active = active_occurrence
        || !pending_approvals.is_empty()
        || protocol_recovery
        || unfinished_execution
        || matches!(
            run_status.as_str(),
            "running" | "waiting_input" | "waiting_approval"
        );
    let latest = items
        .iter()
        .find(|item| !matches!(item.status.as_str(), "skipped" | "blocked"));
    let requires_review = latest.is_some_and(|item| {
        matches!(
            item.status.as_str(),
            "failed" | "cancelled" | "interrupted" | "needs_review"
        )
    });
    let mut issues = readiness(
        server,
        &automation,
        conversation_id,
        root_path,
        *allow_file_discovery,
        *allow_file_writes,
        activity["source_error"].as_str(),
    );
    if pending_recovery {
        issue(&mut issues,"pending_recovery","上轮中断后仍有遗留审批；请先使用“停止当前轮次”清理遗留操作记录，再核实文件与执行记录后继续新一轮","error");
    }
    if protocol_recovery {
        issue(&mut issues,"protocol_recovery","上轮工具调用与结果尚未配对，实际结果可能不确定；请先停止当前轮次清理记录，并核实文件和操作结果后再继续，不会自动重试","error");
    }
    if unfinished_execution {
        issue(&mut issues,"unfinished_execution","上轮仍有未终结的工具执行记录，不能据此判断操作是否完成；请先停止当前轮次清理记录并核实实际结果，再继续新一轮","error");
    }
    if used >= *max_runs as u64 {
        issue(
            &mut issues,
            "budget",
            "委托轮次预算已用完；核对记录后可编辑增加上限，总上限仍为 20 轮",
            "error",
        );
    }
    let ready = !issues.iter().any(|item| item["severity"] == "error");
    let pending_inputs = server.db.pending_conversation_inputs(conversation_id)?.into_iter().map(|input|
        json!({"question_id":input.question_id,"title":input.request.title,"status":input.status})).collect::<Vec<_>>();
    Ok(json!({"automation":&automation,"activity":activity,
        "actions":{"can_start":ready&&!active&&!automation.archived,
            "can_edit":!active&&!automation.archived,"can_stop":active,
            "requires_review":requires_review,"review_occurrence_id":if requires_review { latest.map(|item|item.id.as_str()) } else {None}},
        "readiness":{"issues":issues,"can_start":ready&&!active&&!automation.archived},
        "conversation":{"id":conversation_id,"run_status":run_status,"run_error":run_error},
        "executions":execution_rows(server,conversation_id,false)?,
        "pending_approvals":pending_approvals,"pending_inputs":pending_inputs,
        "files":files(server,conversation_id,root_path)?,"history_total":items.len()}))
}

fn statuses(status: Option<&str>) -> Result<Vec<&'static str>, String> {
    Ok(match status.unwrap_or("all") {
        "" | "all" => vec![],
        "attention" => vec![
            "waiting_approval",
            "waiting_input",
            "failed",
            "cancelled",
            "interrupted",
            "needs_review",
            "blocked",
        ],
        "completed" => vec!["completed"],
        "failed" => vec!["failed"],
        "cancelled" => vec!["cancelled"],
        "interrupted" => vec!["interrupted"],
        "needs_review" => vec!["needs_review"],
        "waiting_input" => vec!["waiting_input"],
        "waiting_approval" => vec!["waiting_approval"],
        "running" => vec!["dispatching", "running"],
        "blocked" => vec!["blocked"],
        "skipped" => vec!["skipped"],
        _ => return Err("不支持的历史状态筛选".into()),
    })
}

pub fn history(
    server: &AppServer,
    id: &str,
    cursor: Option<&str>,
    status: Option<&str>,
) -> Result<Value, String> {
    delegation(server, id)?;
    let selected = statuses(status)?;
    let conn = server.db.conn();
    let cursor = cursor.filter(|value| !value.is_empty());
    if cursor.is_some_and(|value| value.len() > 128) {
        return Err("历史游标无效".into());
    }
    let boundary = cursor.map(|cursor|conn.query_row("SELECT created_at,rowid FROM automation_occurrences WHERE automation_id=?1 AND id=?2",
        params![id,cursor], |row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?))).optional().map_err(|e|e.to_string())?
        .ok_or_else(||"历史游标无效或不属于当前委托".to_string())).transpose()?;
    let selected = serde_json::to_string(&selected).map_err(|e| e.to_string())?;
    let total:i64 = conn.query_row("SELECT count(*) FROM automation_occurrences WHERE automation_id=?1 AND (?2='[]' OR json_extract(payload,'$.status') IN (SELECT value FROM json_each(?2)))",
        params![id,selected],|row|row.get(0)).map_err(|e|e.to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT payload FROM automation_occurrences WHERE automation_id=?1
        AND (?2='[]' OR json_extract(payload,'$.status') IN (SELECT value FROM json_each(?2)))
        AND (?3 IS NULL OR created_at < ?3 OR (created_at=?3 AND rowid < ?4))
        ORDER BY created_at DESC,rowid DESC LIMIT 26",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(
            params![id, selected, boundary.map(|v| v.0), boundary.map(|v| v.1)],
            |row| row.get::<_, String>(0),
        )
        .map_err(|e| e.to_string())?;
    let mut values = rows
        .map(|row| {
            serde_json::from_str::<Occurrence>(&row.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let more = values.len() > PAGE_SIZE;
    values.truncate(PAGE_SIZE);
    let next = if more {
        values.last().map(|item| item.id.clone())
    } else {
        None
    };
    Ok(json!({"occurrences":values,"total":total,"has_more":more,"next_cursor":next}))
}
