//! Correspondence between two saved memories. Link revisions do not write either endpoint.
use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::{
    memory_candidates::ExtractedMemoryRelationship,
    memory_governance::MemoryVersion,
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database, Memory, MemoryQuery,
};

mod corrections;
mod events;
mod semantic;
mod visibility;
pub(crate) use corrections::StageLinkCorrection;
pub(super) use corrections::{resolve_correction, CorrectionSource};
pub(super) use events::record_reviewed_correction_on;
pub(crate) use events::MemoryChangeEvent;
pub(crate) use semantic::MemoryGraphSemantic;
pub(crate) use visibility::GraphScope;

const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1092,
    "1092_memory_graph_links",
    MigrationOwner::V1TaskWorld,
    r#"
CREATE TABLE memory_graph_links (
 id TEXT PRIMARY KEY, source_memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 target_memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 label TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('confirmed','pending','withdrawn')),
 revision INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 CHECK(source_memory_id <> target_memory_id)
);
CREATE INDEX memory_graph_link_target ON memory_graph_links(target_memory_id);
CREATE UNIQUE INDEX memory_graph_link_pair ON memory_graph_links(
 min(source_memory_id,target_memory_id),max(source_memory_id,target_memory_id)
);
CREATE TABLE memory_graph_link_versions (
 relation_id TEXT NOT NULL REFERENCES memory_graph_links(id) ON DELETE CASCADE, revision INTEGER NOT NULL,
 label TEXT NOT NULL, status TEXT NOT NULL, recorded_at INTEGER NOT NULL,
 source_version_id TEXT NOT NULL, target_version_id TEXT NOT NULL,
 PRIMARY KEY(relation_id,revision)
);
CREATE TABLE memory_graph_corrections (
 candidate_id TEXT PRIMARY KEY REFERENCES memory_candidates(id) ON DELETE CASCADE,
 relation_id TEXT NOT NULL REFERENCES memory_graph_links(id) ON DELETE CASCADE,
 relation_revision INTEGER NOT NULL, target_memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 target_revision INTEGER NOT NULL, reason TEXT
);
CREATE INDEX memory_graph_correction_relation ON memory_graph_corrections(relation_id);
CREATE TRIGGER memory_graph_correction_removed BEFORE DELETE ON memory_graph_corrections BEGIN
 UPDATE memory_candidates SET content=NULL,relationship_json=NULL,decision='dismissed',decided_at=unixepoch()*1000
 WHERE id=OLD.candidate_id AND decision IS NULL;
END;
CREATE TRIGGER memory_graph_correction_decided AFTER UPDATE OF decision ON memory_candidates
WHEN NEW.decision IS NOT NULL BEGIN
 UPDATE memory_graph_corrections SET reason=NULL WHERE candidate_id=NEW.id;
END;
CREATE TRIGGER memory_graph_link_purge BEFORE DELETE ON memories BEGIN
 DELETE FROM memory_graph_corrections WHERE target_memory_id=OLD.id OR relation_id IN (
  SELECT id FROM memory_graph_links WHERE source_memory_id=OLD.id OR target_memory_id=OLD.id);
 DELETE FROM memory_graph_link_versions WHERE relation_id IN (
  SELECT id FROM memory_graph_links WHERE source_memory_id=OLD.id OR target_memory_id=OLD.id);
 DELETE FROM memory_graph_links WHERE source_memory_id=OLD.id OR target_memory_id=OLD.id;
END;
"#,
);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LinkStatus {
    Confirmed,
    Pending,
    Withdrawn,
}

impl LinkStatus {
    fn code(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Pending => "pending",
            Self::Withdrawn => "withdrawn",
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryLink {
    pub id: String,
    pub source_memory_id: String,
    pub target_memory_id: String,
    pub label: String,
    pub status: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryLinkVersion {
    pub relation_id: String,
    pub revision: i64,
    pub label: String,
    pub status: String,
    pub recorded_at: i64,
    pub source_version_id: String,
    pub target_version_id: String,
}

#[derive(Deserialize)]
pub(crate) struct CreateMemoryLink {
    pub source_memory_id: String,
    pub target_memory_id: String,
    pub label: String,
    pub status: Option<LinkStatus>,
}

#[derive(Deserialize)]
pub(crate) struct ChangeMemoryLink {
    pub expected_revision: i64,
    pub label: Option<String>,
    pub status: Option<LinkStatus>,
}

pub(crate) struct MemoryGraphSnapshot {
    pub memories: Vec<Memory>,
    pub source_removed: Vec<bool>,
    pub histories: HashMap<String, Vec<MemoryVersion>>,
    pub relations: Vec<MemoryLink>,
    pub relation_versions: Vec<MemoryLinkVersion>,
    pub next_offset: Option<usize>,
    pub semantic: MemoryGraphSemantic,
    pub extracted_relations: Vec<ExtractedMemoryRelationship>,
    pub current_version_ids: HashMap<String, String>,
    pub change_events: Vec<MemoryChangeEvent>,
}

fn storage(err: rusqlite::Error) -> String {
    err.to_string()
}

const LINK_FIELDS: &str =
    "id,source_memory_id,target_memory_id,label,status,revision,created_at,updated_at";

fn read_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryLink> {
    Ok(MemoryLink {
        id: row.get(0)?,
        source_memory_id: row.get(1)?,
        target_memory_id: row.get(2)?,
        label: row.get(3)?,
        status: row.get(4)?,
        revision: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn link_on(conn: &Connection, id: &str) -> Result<MemoryLink, String> {
    conn.query_row(
        &format!("SELECT {LINK_FIELDS} FROM memory_graph_links WHERE id=?1"),
        [id],
        read_link,
    )
    .optional()
    .map_err(storage)?
    .ok_or_else(|| "memory_link_not_found".into())
}

fn endpoint_revisions(conn: &Connection, a: &str, b: &str) -> Result<(i64, i64), String> {
    let now = chrono::Utc::now().timestamp_millis();
    let active = |id: &str| {
        conn.query_row(
            "SELECT revision FROM memory_governance g WHERE memory_id=?1 AND status='active'
             AND (valid_from IS NULL OR valid_from<=?2) AND (valid_until IS NULL OR valid_until>?2)
             AND NOT EXISTS(SELECT 1 FROM knowledge_document_chunks c WHERE c.memory_id=g.memory_id)",
            params![id, now],
            |row| row.get(0),
        ).optional().map_err(storage)?.ok_or_else(|| "memory_link_inactive_endpoint".to_string())
    };
    Ok((active(a)?, active(b)?))
}

fn record_link(
    tx: &Transaction<'_>,
    link: &MemoryLink,
    endpoint_revisions: (i64, i64),
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO memory_graph_link_versions VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            link.id,
            link.revision,
            link.label,
            link.status,
            link.updated_at,
            format!("{}:{}", link.source_memory_id, endpoint_revisions.0),
            format!("{}:{}", link.target_memory_id, endpoint_revisions.1),
        ],
    )
    .map_err(storage)?;
    Ok(())
}

fn validate_label(label: &str) -> Result<&str, String> {
    let label = label.trim();
    if label.is_empty()
        || label.chars().count() > 80
        || crate::safety::contains_sensitive_content(label)
    {
        return Err("invalid_memory_link".into());
    }
    Ok(label)
}

impl Database {
    pub(super) fn initialize_memory_graph(&self) -> rusqlite::Result<()> {
        self.apply_product_migrations(&[SCHEMA, events::SCHEMA])
    }

    #[cfg(test)]
    pub(crate) fn memory_graph(
        &self,
        query: MemoryQuery,
        withdrawn: bool,
        embedding_model: Option<&str>,
    ) -> Result<MemoryGraphSnapshot, String> {
        self.scoped_memory_graph(query, withdrawn, embedding_model, &GraphScope::default())
    }

    pub(crate) fn scoped_memory_graph(
        &self,
        mut query: MemoryQuery,
        withdrawn: bool,
        embedding_model: Option<&str>,
        scope: &GraphScope,
    ) -> Result<MemoryGraphSnapshot, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let scopes = scope.resolve(&tx)?;
        let limit = query.limit.unwrap_or(100).clamp(1, 250);
        let offset = query.offset.unwrap_or_default().min(i64::MAX as usize);
        query.limit = Some(limit + 1);
        query.offset = Some(offset);
        let mut heads = visibility::read_heads(
            &tx,
            &query,
            &scopes,
            withdrawn,
            chrono::Utc::now().timestamp_millis(),
        )?;
        let next_offset = (heads.len() > limit).then(|| offset.saturating_add(limit));
        heads.truncate(limit);
        let effective_revisions: HashMap<_, _> = heads
            .iter()
            .filter(|_| !withdrawn)
            .map(|head| (head.memory.id.clone(), head.revision))
            .collect();
        let current_version_ids: HashMap<_, _> = heads
            .iter()
            .filter(|_| !withdrawn)
            .map(|head| {
                (
                    head.memory.id.clone(),
                    format!("{}:{}", head.memory.id, head.revision),
                )
            })
            .collect();
        let mut memories: Vec<_> = heads.into_iter().map(|head| head.memory).collect();
        let source_removed = Self::memory_source_status_on(&tx, &mut memories).map_err(storage)?;
        let ids = memories
            .iter()
            .map(|memory| memory.id.clone())
            .collect::<Vec<_>>();
        let mut histories = Self::memory_histories_on(&tx, &ids)?;
        for history in histories.values_mut() {
            let original_count = history.len();
            history.retain(|version| scopes.contains(&version.scope));
            if history.len() != original_count {
                for (index, version) in history.iter_mut().rev().enumerate() {
                    version.layer = index;
                    version.baseline = index == 0;
                }
            }
        }
        let (mut relations, mut relation_versions) = read_links(&tx, &ids)?;
        let allowed_versions: HashSet<_> = histories
            .values()
            .flatten()
            .map(|version| version.version_id.as_str())
            .collect();
        relation_versions.retain(|edge| {
            allowed_versions.contains(edge.source_version_id.as_str())
                && allowed_versions.contains(edge.target_version_id.as_str())
        });
        let current_evidence = |version: &str| {
            version.rsplit_once(':').is_some_and(|(id, revision)| {
                effective_revisions.get(id).is_some_and(|effective| {
                    revision
                        .parse::<i64>()
                        .is_ok_and(|value| value <= *effective)
                })
            })
        };
        let allowed_link_revisions: HashSet<_> = relation_versions
            .iter()
            .filter(|edge| {
                withdrawn
                    || (current_evidence(&edge.source_version_id)
                        && current_evidence(&edge.target_version_id))
            })
            .map(|edge| (edge.relation_id.as_str(), edge.revision))
            .collect();
        relations
            .retain(|edge| allowed_link_revisions.contains(&(edge.id.as_str(), edge.revision)));
        let related_ids = relation_versions
            .iter()
            .map(|edge| edge.relation_id.as_str())
            .collect();
        let change_events = events::changes(&tx, &histories, &related_ids)?;
        let semantic = semantic::read_semantic(&tx, &memories, embedding_model, withdrawn)?;
        let extracted_relations =
            Self::effective_extracted_relationships_on(&tx, &effective_revisions)?;
        tx.commit().map_err(storage)?;
        Ok(MemoryGraphSnapshot {
            memories,
            source_removed,
            histories,
            relations,
            relation_versions,
            next_offset,
            semantic,
            extracted_relations,
            current_version_ids,
            change_events,
        })
    }

    pub(crate) fn create_memory_link(&self, input: CreateMemoryLink) -> Result<MemoryLink, String> {
        let (a, b) = (input.source_memory_id.trim(), input.target_memory_id.trim());
        if a == b {
            return Err("invalid_memory_link".into());
        }
        let label = validate_label(&input.label)?;
        let status = input.status.unwrap_or(LinkStatus::Confirmed);
        if status == LinkStatus::Withdrawn {
            return Err("invalid_memory_link".into());
        }
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let endpoints = endpoint_revisions(&tx, a, b)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        if tx.execute(
            "INSERT INTO memory_graph_links VALUES (?1,?2,?3,?4,?5,1,?6,?6) ON CONFLICT DO NOTHING",
            params![id, a, b, label, status.code(), now],
        ).map_err(storage)? == 0 {
            return Err("memory_link_exists".into());
        }
        let link = link_on(&tx, &id)?;
        record_link(&tx, &link, endpoints)?;
        tx.commit().map_err(storage)?;
        Ok(link)
    }

    pub(crate) fn change_memory_link(
        &self,
        id: &str,
        input: ChangeMemoryLink,
    ) -> Result<MemoryLink, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let before = link_on(&tx, id)?;
        if before.revision != input.expected_revision {
            return Err("memory_link_revision_conflict".into());
        }
        let label = input
            .label
            .as_deref()
            .map(validate_label)
            .transpose()?
            .unwrap_or(&before.label);
        let status = input.status.map(LinkStatus::code).unwrap_or(&before.status);
        if label == before.label && status == before.status {
            return Ok(before);
        }
        let endpoints =
            endpoint_revisions(&tx, &before.source_memory_id, &before.target_memory_id)?;
        tx.execute(
            "UPDATE memory_graph_links SET label=?2,status=?3,revision=revision+1,updated_at=?4 WHERE id=?1",
            params![id, label, status, chrono::Utc::now().timestamp_millis()],
        ).map_err(storage)?;
        let link = link_on(&tx, id)?;
        record_link(&tx, &link, endpoints)?;
        tx.commit().map_err(storage)?;
        Ok(link)
    }
}

fn read_links(
    conn: &Connection,
    ids: &[String],
) -> Result<(Vec<MemoryLink>, Vec<MemoryLinkVersion>), String> {
    if ids.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let selected = ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let ids_json = serde_json::to_string(ids).map_err(|error| error.to_string())?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {LINK_FIELDS} FROM memory_graph_links WHERE source_memory_id IN (SELECT value FROM json_each(?1)) OR target_memory_id IN (SELECT value FROM json_each(?1)) ORDER BY created_at,id"
    )).map_err(storage)?;
    let relations = stmt
        .query_map([&ids_json], read_link)
        .map_err(storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(storage)?
        .into_iter()
        .filter(|edge| {
            selected.contains(edge.source_memory_id.as_str())
                && selected.contains(edge.target_memory_id.as_str())
        })
        .collect::<Vec<_>>();
    let link_ids =
        serde_json::to_string(&relations.iter().map(|link| &link.id).collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
    let mut versions = conn.prepare(
        "SELECT relation_id,revision,label,status,recorded_at,source_version_id,target_version_id
         FROM memory_graph_link_versions WHERE relation_id IN (SELECT value FROM json_each(?1)) ORDER BY relation_id,revision",
    ).map_err(storage)?;
    let rows = versions
        .query_map([link_ids], |row| {
            Ok(MemoryLinkVersion {
                relation_id: row.get(0)?,
                revision: row.get(1)?,
                label: row.get(2)?,
                status: row.get(3)?,
                recorded_at: row.get(4)?,
                source_version_id: row.get(5)?,
                target_version_id: row.get(6)?,
            })
        })
        .map_err(storage)?;
    Ok((
        relations,
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage)?,
    ))
}

#[cfg(test)]
mod tests;
