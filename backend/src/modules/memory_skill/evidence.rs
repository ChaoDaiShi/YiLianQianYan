//! Evidence screening for the completed-task sources a candidate cites.
//!
//! A candidate may only cite a bounded number of explicitly completed work
//! items, and the cited text is re-read from storage and re-screened — a row
//! that was clean when it was written is not assumed to still be clean.

use crate::{db::validate_completed_source, modules::task::artifact::ArtifactSource, safety};

use super::sensitivity::screen_lesson;

pub(crate) fn screen_evidence(
    conn: &rusqlite::Connection,
    sources: &[ArtifactSource],
    lesson: &str,
) -> Result<(), String> {
    screen_lesson(lesson)?;
    if sources.is_empty()
        || sources.len() > 8
        || serde_json::to_string(sources)
            .map_err(|_| "来源无效")?
            .len()
            > 4096
    {
        return Err("须明确选择 1-8 个有界完成任务来源".into());
    }
    for source in sources {
        validate_completed_source(conn, source)?;
        let text = match source {
            ArtifactSource::Task {
                task_id,
                execution_id,
            } => {
                let mut value: String = conn.query_row("SELECT title || char(10) || description FROM tasks WHERE id=?1",[task_id],|r|r.get(0)).map_err(|_|"来源任务不可用")?;
                let mut q=conn.prepare("SELECT summary FROM artifacts WHERE task_id=?1 AND task_execution_id=?2 LIMIT 8").map_err(|_|"来源结果不可用")?;
                for summary in q
                    .query_map(rusqlite::params![task_id, execution_id], |r| {
                        r.get::<_, String>(0)
                    })
                    .map_err(|_| "来源结果不可用")?
                {
                    value.push_str(&summary.map_err(|_| "来源结果无效")?);
                }
                value
            }
            ArtifactSource::Node { execution_id, .. } => conn.query_row("SELECT context_json || char(10) || COALESCE(output_json,'') FROM task_node_executions WHERE execution_id=?1",[execution_id],|r|r.get::<_,String>(0)).map_err(|_|"来源执行不可用")?,
        };
        if safety::contains_sensitive_content(&text) {
            return Err("所选任务证据含敏感信息，未保存候选".into());
        }
    }
    Ok(())
}
