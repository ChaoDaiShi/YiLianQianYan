use super::{Database, MessageRow};
use crate::integrations::llm::types::ToolCall;
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;

pub struct MessageMutation {
    pub deleted_message_ids: Vec<String>,
    pub removed_protocol_count: usize,
    pub draft_text: Option<String>,
}

fn visible(row: &MessageRow) -> bool {
    matches!(row.role.as_str(), "user" | "assistant") && !row.content.trim().is_empty()
}

fn remove_protocol_row(
    index: usize,
    rows: &[MessageRow],
    removed: &mut HashSet<usize>,
    stripped: &mut HashSet<usize>,
) {
    if removed.contains(&index) {
        return;
    }
    if visible(&rows[index]) {
        stripped.insert(index);
    } else {
        removed.insert(index);
    }
}

#[derive(serde::Serialize)]
pub struct HistoryProtocolIssue {
    pub affected_user_message_id: Option<String>,
    pub error: String,
}

/// Diagnose the proposed sequence without changing its retained prefix.
fn protocol_issue(
    rows: &[MessageRow],
    removed: &HashSet<usize>,
    stripped: &HashSet<usize>,
) -> Option<HistoryProtocolIssue> {
    let mut pending = HashSet::<String>::new();
    let mut user = None;
    let mut owner = None;
    let issue = |user: Option<String>| HistoryProtocolIssue {
        affected_user_message_id: user,
        error: "历史工具调用与结果未配对，请显式回退到受影响的用户消息后重发".into(),
    };
    for (index, row) in rows.iter().enumerate() {
        if removed.contains(&index) {
            continue;
        }
        if row.role == "tool" {
            if !row
                .tool_call_id
                .as_ref()
                .is_some_and(|id| pending.remove(id))
            {
                return Some(issue(user.clone()));
            }
            continue;
        }
        if !pending.is_empty() {
            return Some(issue(owner));
        }
        if row.role == "user" {
            user = Some(row.id.clone());
        }
        if stripped.contains(&index) {
            continue;
        }
        if let Some(raw) = &row.tool_calls {
            let calls = serde_json::from_str::<Vec<ToolCall>>(raw);
            match calls {
                Ok(calls)
                    if row.role == "assistant" && calls.iter().all(|call| !call.id.is_empty()) =>
                {
                    for call in &calls {
                        pending.insert(call.id.clone());
                    }
                    if pending.len() != calls.len() {
                        return Some(issue(user));
                    }
                    owner = user.clone();
                }
                _ => return Some(issue(user)),
            }
        }
    }
    if !pending.is_empty() {
        return Some(issue(owner));
    }
    None
}

impl Database {
    pub fn conversation_protocol_issue(
        &self,
        conversation: &str,
    ) -> Result<Option<HistoryProtocolIssue>, String> {
        let rows = self.get_conversation(conversation)?.messages;
        Ok(protocol_issue(&rows, &HashSet::new(), &HashSet::new()))
    }
    /// The caller holds the conversation turn gate. Nothing outside this
    /// conversation, including Resource blobs and independent forks, is deleted.
    pub fn mutate_conversation_message(
        &self,
        conversation: &str,
        message: &str,
        rollback: bool,
    ) -> Result<MessageMutation, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        let status: Option<String> = tx
            .query_row(
                "SELECT run_status FROM conversations WHERE id=?1",
                [conversation],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let status = status.ok_or("conversation_not_found")?;
        if status == "running" {
            return Err("conversation_running".into());
        }
        let rows = {
            let mut statement = tx.prepare("SELECT id,conversation_id,role,content,tool_calls,tool_call_id,tool_name,tool_result,created_at FROM messages WHERE conversation_id=?1 ORDER BY created_at,rowid").map_err(|error|error.to_string())?;
            let rows = statement
                .query_map([conversation], |row| {
                    Ok(MessageRow {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        role: row.get(2)?,
                        content: row.get(3)?,
                        tool_calls: row.get(4)?,
                        tool_call_id: row.get(5)?,
                        tool_name: row.get(6)?,
                        tool_result: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            rows
        };
        let target = rows
            .iter()
            .position(|row| row.id == message)
            .ok_or("message_not_found")?;
        if !visible(&rows[target]) {
            return Err("message_not_visible".into());
        }
        if rollback && rows[target].role != "user" {
            return Err("rollback_requires_user_message".into());
        }
        let draft_text = rollback.then(|| rows[target].content.clone());
        let mut removed = HashSet::new();
        let mut stripped = HashSet::new();
        if rollback {
            removed.extend(target..rows.len());
        } else {
            removed.insert(target);
            let start = (0..=target)
                .rev()
                .find(|index| rows[*index].role == "user")
                .unwrap_or(0);
            let end = ((target + 1)..rows.len())
                .find(|index| rows[*index].role == "user")
                .unwrap_or(rows.len());
            for index in start..end {
                if rows[index].role == "tool" || rows[index].tool_calls.is_some() {
                    remove_protocol_row(index, &rows, &mut removed, &mut stripped);
                }
            }
        }
        if let Some(issue) = protocol_issue(&rows, &removed, &stripped) {
            return Err(format!(
                "history_protocol:{}",
                issue.affected_user_message_id.unwrap_or_default()
            ));
        }
        let now = chrono::Utc::now().timestamp_millis();
        tx.execute("UPDATE conversation_inputs SET status='cancelled',revision=revision+1,resolved_at=?2 WHERE conversation_id=?1 AND status='pending'",params![conversation,now]).map_err(|error|error.to_string())?;
        tx.execute("UPDATE security_approvals SET status='cancelled',resolved_at=?2 WHERE conversation_id=?1 AND status='pending'",params![conversation,now]).map_err(|error|error.to_string())?;
        tx.execute("UPDATE conversation_turns SET status='cancelled',finished_at=?2 WHERE conversation_id=?1 AND finished_at IS NULL",params![conversation,now]).map_err(|error|error.to_string())?;
        let mut deleted_message_ids = Vec::new();
        let mut removed_protocol_count = stripped.len();
        for (index, row) in rows.iter().enumerate() {
            if removed.contains(&index) || stripped.contains(&index) {
                if let Some(raw) = &row.tool_calls {
                    if let Ok(calls) = serde_json::from_str::<Vec<ToolCall>>(raw) {
                        for call in calls {
                            tx.execute("DELETE FROM conversation_execution_records WHERE conversation_id=?1 AND tool_call_id=?2",params![conversation,call.id]).map_err(|error|error.to_string())?;
                        }
                    }
                }
            }
            if removed.contains(&index) {
                if !visible(row) {
                    removed_protocol_count += 1;
                }
                tx.execute(
                    "DELETE FROM settings WHERE key=?1 OR key=?2",
                    params![
                        format!("chat_reasoning:{}", row.id),
                        format!("chat_turn_reasoning:{}", row.id)
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.execute("DELETE FROM conversation_turns WHERE conversation_id=?1 AND user_message_id=?2",params![conversation,row.id]).map_err(|error|error.to_string())?;
                tx.execute(
                    "DELETE FROM messages WHERE conversation_id=?1 AND id=?2",
                    params![conversation, row.id],
                )
                .map_err(|error| error.to_string())?;
                deleted_message_ids.push(row.id.clone());
            } else if stripped.contains(&index) {
                tx.execute(
                    "UPDATE messages SET tool_calls=NULL WHERE conversation_id=?1 AND id=?2",
                    params![conversation, row.id],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        tx.execute(
            "DELETE FROM settings WHERE key=?1",
            [format!("conversation_compaction:{conversation}")],
        )
        .map_err(|error| error.to_string())?;
        tx.execute("UPDATE conversations SET run_status='idle',run_error=NULL,run_started_at=NULL,run_finished_at=NULL,updated_at=?2 WHERE id=?1",params![conversation,now]).map_err(|error|error.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok(MessageMutation {
            deleted_message_ids,
            removed_protocol_count,
            draft_text,
        })
    }
}

#[cfg(test)]
mod tests;
