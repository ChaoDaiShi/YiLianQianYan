use rusqlite::{params, Connection};

use super::{memories::lexical::LexicalQuery, Database};

pub(crate) const CHOICE_INTRO: &str = "我找到了多个可能的旧对话。";

pub(crate) struct PriorChat {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
    pub excerpts: Vec<(String, String)>,
}

const SOURCES: &str = "
    SELECT c.id,c.title,c.updated_at FROM conversations c
    LEFT JOIN settings w ON w.key='conversation_workspace:'||c.id
    WHERE c.id<>?1 AND (?2 IS NULL OR c.id=?2)
      AND w.value IS (SELECT value FROM settings WHERE key='conversation_workspace:'||?1)
      AND NOT EXISTS (SELECT 1 FROM settings WHERE key IN
        ('conversation_origin:'||c.id,'personal_assistant:'||c.id))
      AND NOT EXISTS (SELECT 1 FROM connector_bindings WHERE kind='conversation_agent'
        AND source_id='conversation:'||c.id)
      AND NOT EXISTS (SELECT 1 FROM conversation_goal_bindings WHERE conversation_id=c.id)
      AND NOT EXISTS (SELECT 1 FROM message_canvas_branches WHERE conversation_id=c.id)
      AND NOT EXISTS (SELECT 1 FROM conversation_lanes WHERE conversation_id=c.id)
      AND NOT EXISTS (SELECT 1 FROM channel_robot_runs WHERE conversation_id=c.id)
      AND EXISTS (SELECT 1 FROM messages WHERE conversation_id=c.id AND role='assistant'
        AND length(trim(content))>0 AND content NOT LIKE ?4||'%')
    ORDER BY CASE WHEN ?3<>'' AND instr(lower(c.title),lower(?3))>0 THEN 0 ELSE 1 END,
             c.updated_at DESC,c.rowid DESC
    LIMIT 50";

fn read_sources(
    conn: &Connection,
    conversation: &str,
    source: Option<&str>,
    topic: &str,
) -> Result<Vec<PriorChat>, String> {
    let mut statement = conn.prepare(SOURCES).map_err(|error| error.to_string())?;
    let mut chats = statement
        .query_map(params![conversation, source, topic, CHOICE_INTRO], |row| {
            Ok(PriorChat {
                id: row.get(0)?,
                title: row.get(1)?,
                updated_at: row.get(2)?,
                excerpts: Vec::new(),
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut excerpts = conn
        .prepare(
            "WITH visible AS NOT MATERIALIZED (
                SELECT rowid AS seq, created_at, role,
                    CASE WHEN length(content)>640 THEN substr(content,1,419)||'…'||substr(content,-220)
                         ELSE content END AS excerpt
                FROM messages
                WHERE conversation_id=?1 AND role IN ('user','assistant')
                  AND length(trim(content))>0 AND content NOT LIKE ?2||'%'
             ), first AS (
                SELECT * FROM visible ORDER BY created_at,seq LIMIT 2
             ), recent AS (
                SELECT * FROM visible ORDER BY created_at DESC,seq DESC LIMIT 4
             )
             SELECT role,excerpt FROM (SELECT * FROM first UNION SELECT * FROM recent)
             ORDER BY created_at,seq",
        )
        .map_err(|error| error.to_string())?;
    for chat in &mut chats {
        chat.excerpts = excerpts
            .query_map(params![chat.id, CHOICE_INTRO], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
    }
    Ok(chats)
}

impl Database {
    pub(crate) fn find_prior_chat(
        &self,
        conversation: &str,
        source: &str,
    ) -> Result<Option<PriorChat>, String> {
        self.initialize_conversation_goal_bindings()?;
        Ok(read_sources(&self.conn(), conversation, Some(source), "")?.pop())
    }

    pub(crate) fn search_prior_chats(
        &self,
        conversation: &str,
        topic: &str,
        plan: bool,
    ) -> Result<Vec<PriorChat>, String> {
        self.initialize_conversation_goal_bindings()?;
        let candidates = read_sources(&self.conn(), conversation, None, topic)?;
        let query = LexicalQuery::new(topic);
        let describes_plan = |text: &str| {
            ["方案", "计划", "规划", "步骤"]
                .iter()
                .any(|word| text.contains(word))
        };
        let mut ranked: Vec<_> = candidates
            .into_iter()
            .filter_map(|chat| {
                if query.is_empty() {
                    let relevant = !plan
                        || describes_plan(&chat.title)
                        || chat
                            .excerpts
                            .iter()
                            .any(|(role, text)| role == "assistant" && describes_plan(text));
                    return relevant.then_some((1.0, chat));
                }
                let title_score = query.score(&chat.title);
                let body_score = chat
                    .excerpts
                    .iter()
                    .map(|(_, text)| query.score(text))
                    .fold(0.0, f64::max);
                let relevance = title_score.max(body_score);
                let score = relevance + if title_score >= 0.9 { 0.2 } else { 0.0 };
                (relevance >= 0.24).then_some((score, chat))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        let best = ranked.first().map_or(0.0, |item| item.0);
        Ok(ranked
            .into_iter()
            .take_while(|(score, _)| best - score < 0.12)
            .take(3)
            .map(|(_, chat)| chat)
            .collect())
    }
}
