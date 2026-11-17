use super::{knowledge_text_version, Database, MigrationOwner, ProductMigrationSpec};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(super) const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1085,
    "1085_knowledge_entries",
    MigrationOwner::V1TaskWorld,
    "CREATE TABLE knowledge_entries (
        id TEXT PRIMARY KEY, topic TEXT NOT NULL, title TEXT NOT NULL, summary TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE knowledge_entry_sources (
        entry_id TEXT NOT NULL REFERENCES knowledge_entries(id) ON DELETE CASCADE,
        position INTEGER NOT NULL, document_id TEXT NOT NULL, document_name TEXT NOT NULL,
        version TEXT NOT NULL, start_char INTEGER NOT NULL, end_char INTEGER NOT NULL,
        PRIMARY KEY (entry_id, position)
    );",
);

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSourceAnchor {
    pub document_id: String,
    pub version: String,
    pub start_char: usize,
    pub end_char: usize,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceStatus {
    Available,
    Changed,
    Removed,
}

#[derive(Serialize)]
pub struct KnowledgeEntrySource {
    #[serde(flatten)]
    pub anchor: KnowledgeSourceAnchor,
    pub document_name: String,
    pub status: KnowledgeSourceStatus,
    pub excerpt: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeEntryDraft {
    pub topic: String,
    pub title: String,
    pub summary: String,
    pub sources: Vec<KnowledgeSourceAnchor>,
}

#[derive(Serialize)]
pub struct KnowledgeEntrySummary {
    pub id: String,
    pub topic: String,
    pub title: String,
    pub summary: String,
    pub updated_at: i64,
    pub source_count: usize,
}

#[derive(Serialize)]
pub struct KnowledgeEntry {
    #[serde(flatten)]
    pub entry: KnowledgeEntrySummary,
    pub sources: Vec<KnowledgeEntrySource>,
}

#[derive(Debug)]
pub enum KnowledgeEntryError {
    NotFound,
    Invalid(&'static str),
    Storage(rusqlite::Error),
}

impl From<rusqlite::Error> for KnowledgeEntryError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Storage(value)
    }
}

const SELECT_ENTRY: &str = "SELECT e.id,e.topic,e.title,e.summary,e.updated_at,
    (SELECT COUNT(*) FROM knowledge_entry_sources s WHERE s.entry_id=e.id)
    FROM knowledge_entries e";

fn summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeEntrySummary> {
    Ok(KnowledgeEntrySummary {
        id: row.get(0)?,
        topic: row.get(1)?,
        title: row.get(2)?,
        summary: row.get(3)?,
        updated_at: row.get(4)?,
        source_count: row.get(5)?,
    })
}

fn anchor(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeSourceAnchor> {
    Ok(KnowledgeSourceAnchor {
        document_id: row.get(0)?,
        version: row.get(1)?,
        start_char: row.get(2)?,
        end_char: row.get(3)?,
    })
}

fn resolve_excerpt(anchor: &KnowledgeSourceAnchor, text: &str) -> Option<String> {
    if knowledge_text_version(text) != anchor.version {
        return None;
    }
    let length = anchor.end_char.checked_sub(anchor.start_char)?;
    if !(1..=4000).contains(&length) {
        return None;
    }
    let excerpt: String = text.chars().skip(anchor.start_char).take(length).collect();
    (excerpt.chars().count() == length && !excerpt.trim().is_empty()).then_some(excerpt)
}

fn read_entry(conn: &Connection, id: &str) -> rusqlite::Result<Option<KnowledgeEntry>> {
    let entry = conn
        .query_row(&format!("{SELECT_ENTRY} WHERE e.id=?1"), [id], summary)
        .optional()?;
    let Some(entry) = entry else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT s.document_id,s.version,s.start_char,s.end_char,s.document_name,d.text_content
         FROM knowledge_entry_sources s LEFT JOIN knowledge_documents d ON d.id=s.document_id
         WHERE s.entry_id=?1 ORDER BY s.position",
    )?;
    let sources = stmt
        .query_map([id], |row| {
            let anchor = anchor(row)?;
            let text: Option<String> = row.get(5)?;
            let excerpt = text
                .as_deref()
                .and_then(|text| resolve_excerpt(&anchor, text));
            let status = if excerpt.is_some() {
                KnowledgeSourceStatus::Available
            } else if text.is_some() {
                KnowledgeSourceStatus::Changed
            } else {
                KnowledgeSourceStatus::Removed
            };
            Ok(KnowledgeEntrySource {
                anchor,
                document_name: row.get(4)?,
                status,
                excerpt,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(KnowledgeEntry { entry, sources }))
}

impl Database {
    pub fn knowledge_entries(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<KnowledgeEntrySummary>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "{SELECT_ENTRY}
             WHERE ?1='' OR instr(lower(e.title),lower(?1))>0
                         OR instr(lower(e.topic),lower(?1))>0
                         OR instr(lower(e.summary),lower(?1))>0
             ORDER BY e.updated_at DESC,e.id LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt.query_map(params![query, limit, offset], summary)?;
        rows.collect()
    }

    pub fn knowledge_entry(&self, id: &str) -> rusqlite::Result<Option<KnowledgeEntry>> {
        read_entry(&self.conn(), id)
    }

    pub fn save_knowledge_entry(
        &self,
        id: Option<&str>,
        draft: &KnowledgeEntryDraft,
    ) -> Result<KnowledgeEntry, KnowledgeEntryError> {
        let topic = draft.topic.trim();
        let title = draft.title.trim();
        let summary = draft.summary.trim();
        if !(1..=120).contains(&topic.chars().count())
            || !(1..=160).contains(&title.chars().count())
            || !(1..=8000).contains(&summary.chars().count())
        {
            return Err(KnowledgeEntryError::Invalid(
                "请填写主题、标题与摘要，长度分别不超过 120、160 和 8000 个字符。",
            ));
        }
        let unique: HashSet<_> = draft.sources.iter().collect();
        if !(1..=16).contains(&draft.sources.len()) || unique.len() != draft.sources.len() {
            return Err(KnowledgeEntryError::Invalid(
                "请保留 1 至 16 条互不重复的原文引用。",
            ));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = chrono::Utc::now().timestamp_millis();
        let entry_id = if let Some(id) = id {
            if tx.execute(
                "UPDATE knowledge_entries SET topic=?2,title=?3,summary=?4,updated_at=?5 WHERE id=?1",
                params![id, topic, title, summary, now],
            )? == 0 {
                return Err(KnowledgeEntryError::NotFound);
            }
            id.to_owned()
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO knowledge_entries(id,topic,title,summary,updated_at) VALUES(?1,?2,?3,?4,?5)",
                params![id, topic, title, summary, now],
            )?;
            id
        };
        let mut sources = Vec::with_capacity(draft.sources.len());
        for anchor in &draft.sources {
            let retained: Option<String> = tx.query_row(
                "SELECT document_name FROM knowledge_entry_sources
                 WHERE entry_id=?1 AND document_id=?2 AND version=?3 AND start_char=?4 AND end_char=?5",
                params![entry_id, anchor.document_id, anchor.version, anchor.start_char, anchor.end_char],
                |row| row.get(0),
            ).optional()?;
            if let Some(name) = retained {
                sources.push((anchor, name));
                continue;
            }
            let source: Option<(String, String)> = tx
                .query_row(
                    "SELECT name,text_content FROM knowledge_documents WHERE id=?1",
                    [&anchor.document_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((name, text)) = source else {
                return Err(KnowledgeEntryError::Invalid(
                    "所选资料已移除，请重新选择引用。",
                ));
            };
            if resolve_excerpt(anchor, &text).is_none() {
                return Err(KnowledgeEntryError::Invalid(
                    "原文版本或所选范围已变化，请重新选择引用。",
                ));
            }
            sources.push((anchor, name));
        }
        tx.execute(
            "DELETE FROM knowledge_entry_sources WHERE entry_id=?1",
            [&entry_id],
        )?;
        for (position, (anchor, name)) in sources.into_iter().enumerate() {
            tx.execute(
                "INSERT INTO knowledge_entry_sources VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    entry_id,
                    position,
                    anchor.document_id,
                    name,
                    anchor.version,
                    anchor.start_char,
                    anchor.end_char
                ],
            )?;
        }
        let entry = read_entry(&tx, &entry_id)?.ok_or(KnowledgeEntryError::NotFound)?;
        tx.commit()?;
        Ok(entry)
    }

    pub fn delete_knowledge_entry(&self, id: &str) -> rusqlite::Result<bool> {
        self.conn()
            .execute("DELETE FROM knowledge_entries WHERE id=?1", [id])
            .map(|count| count != 0)
    }
}
