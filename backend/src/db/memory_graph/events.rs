//! Change-event identities and explicit version provenance for reviewed corrections.
use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::storage;
use crate::db::{
    memory_governance::MemoryVersion,
    migrations::{MigrationOwner, ProductMigrationSpec},
};

pub(super) const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1094,
    "1094_memory_graph_change_events",
    MigrationOwner::V1TaskWorld,
    r#"
CREATE TABLE memory_graph_correction_pins (
 candidate_id TEXT PRIMARY KEY REFERENCES memory_graph_corrections(candidate_id) ON DELETE CASCADE,
 source_memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 source_revision INTEGER NOT NULL
);
CREATE TABLE memory_graph_reviewed_events (
 memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL,
 source_memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 source_revision INTEGER NOT NULL,
 relation_id TEXT NOT NULL REFERENCES memory_graph_links(id) ON DELETE CASCADE,
 PRIMARY KEY(memory_id,revision)
);
CREATE TRIGGER memory_graph_event_purge BEFORE DELETE ON memories BEGIN
 DELETE FROM memory_graph_correction_pins WHERE source_memory_id=OLD.id;
 DELETE FROM memory_graph_reviewed_events WHERE memory_id=OLD.id OR source_memory_id=OLD.id;
END;
CREATE TRIGGER memory_graph_pin_decided AFTER UPDATE OF decision ON memory_candidates
WHEN NEW.decision IS NOT NULL BEGIN
 DELETE FROM memory_graph_correction_pins WHERE candidate_id=NEW.id;
END;
CREATE TRIGGER memory_graph_pin_removed BEFORE DELETE ON memory_graph_corrections BEGIN
 DELETE FROM memory_graph_correction_pins WHERE candidate_id=OLD.candidate_id;
END;
"#,
);

#[derive(Serialize)]
pub(crate) struct MemoryChangeEvent {
    pub id: String,
    pub memory_id: String,
    pub version_id: String,
    pub previous_version_id: Option<String>,
    pub recorded_at: i64,
    pub kind: &'static str,
    pub related_event_id: Option<String>,
    pub related_memory_id: Option<String>,
    pub related_version_id: Option<String>,
    pub relation_id: Option<String>,
}

fn event_id(version_id: &str) -> String {
    format!("memory-version:{version_id}")
}

pub(super) fn eligible_source_version(
    conn: &Connection,
    source: &str,
    revision: i64,
    target: &str,
) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM memory_versions v
         JOIN memory_governance source ON source.memory_id=v.memory_id
         JOIN memory_governance target ON target.memory_id=?3
         WHERE v.memory_id=?1 AND v.revision=?2 AND v.status='active' AND source.status='active'
           AND v.scope=source.scope AND source.scope=target.scope
           AND (v.valid_from IS NULL OR v.valid_from<=?4))",
        params![
            source,
            revision,
            target,
            chrono::Utc::now().timestamp_millis()
        ],
        |row| row.get(0),
    )
    .map_err(storage)
}

type PinnedVersion = (String, i64);
pub(super) fn correction_pin(
    conn: &Connection,
    candidate_id: &str,
) -> Result<Option<PinnedVersion>, String> {
    conn.query_row(
        "SELECT source_memory_id,source_revision FROM memory_graph_correction_pins WHERE candidate_id=?1",
        [candidate_id], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional().map_err(storage)
}

pub(in crate::db) fn record_reviewed_correction_on(
    conn: &Connection,
    candidate_id: &str,
    memory_id: &str,
    prior_revision: i64,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO memory_graph_reviewed_events(memory_id,revision,source_memory_id,source_revision,relation_id)
         SELECT g.memory_id,g.revision,p.source_memory_id,p.source_revision,c.relation_id
         FROM memory_graph_correction_pins p JOIN memory_graph_corrections c ON c.candidate_id=p.candidate_id
         JOIN memory_governance g ON g.memory_id=c.target_memory_id
         WHERE p.candidate_id=?1 AND g.memory_id=?2 AND g.revision>?3",
        params![candidate_id,memory_id,prior_revision],
    ).map_err(storage)?;
    Ok(())
}

pub(super) fn changes(
    conn: &Connection,
    histories: &HashMap<String, Vec<MemoryVersion>>,
    visible_relationships: &HashSet<&str>,
) -> Result<Vec<MemoryChangeEvent>, String> {
    let ids = serde_json::to_string(&histories.keys().collect::<Vec<_>>())
        .map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT memory_id,revision,source_memory_id,source_revision,relation_id
         FROM memory_graph_reviewed_events WHERE memory_id IN (SELECT value FROM json_each(?1))",
        )
        .map_err(storage)?;
    let rows = stmt
        .query_map([ids], |row| {
            let id: String = row.get(0)?;
            let revision: i64 = row.get(1)?;
            let source: String = row.get(2)?;
            let source_revision: i64 = row.get(3)?;
            let relation: String = row.get(4)?;
            Ok((
                format!("{id}:{revision}"),
                (source, source_revision, relation),
            ))
        })
        .map_err(storage)?;
    let provenance = rows
        .collect::<rusqlite::Result<HashMap<_, _>>>()
        .map_err(storage)?;
    let known_versions = histories
        .values()
        .flatten()
        .map(|version| version.version_id.as_str())
        .collect::<HashSet<_>>();
    let mut events = Vec::new();
    for (id, history) in histories {
        for (index, version) in history.iter().enumerate() {
            // The first row of imported history is a recorded baseline of unknown origin.
            let previous = history
                .get(index + 1)
                .filter(|old| old.layer + 1 == version.layer);
            let mut event = MemoryChangeEvent {
                id: event_id(&version.version_id),
                memory_id: id.clone(),
                version_id: version.version_id.clone(),
                previous_version_id: previous.map(|old| old.version_id.clone()),
                recorded_at: version.recorded_at,
                kind: if version.baseline {
                    "baseline"
                } else if version.status == "withdrawn" {
                    "withdrawn"
                } else if version.layer == 0 {
                    "created"
                } else {
                    "changed"
                },
                related_event_id: None,
                related_memory_id: None,
                related_version_id: None,
                relation_id: None,
            };
            if let Some((source, revision, relation)) = provenance.get(&version.version_id) {
                let source_version = format!("{source}:{revision}");
                if known_versions.contains(source_version.as_str())
                    && visible_relationships.contains(relation.as_str())
                {
                    event.kind = "reviewed_correction";
                    event.related_event_id = Some(event_id(&source_version));
                    event.related_memory_id = Some(source.clone());
                    event.related_version_id = Some(source_version);
                    event.relation_id = Some(relation.clone());
                }
            }
            events.push(event);
        }
    }
    events.sort_by(|a, b| a.recorded_at.cmp(&b.recorded_at).then(a.id.cmp(&b.id)));
    Ok(events)
}
