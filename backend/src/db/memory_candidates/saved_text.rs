//! Source-pinned graph projections: the memory's text and version stay authoritative.
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;

use super::{CandidateReview, MemoryRelationship, RelationshipCertainty};
use crate::db::{
    memory_governance::content_hash,
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};

pub(super) const SCHEMA: ProductMigrationSpec = ProductMigrationSpec::new(
    1093,
    "1093_memory_text_relationships",
    MigrationOwner::V1TaskWorld,
    r#"
CREATE TABLE memory_text_relationships (
 id TEXT PRIMARY KEY,
 memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 memory_revision INTEGER NOT NULL,
 revision INTEGER NOT NULL,
 relationship_json TEXT NOT NULL,
 identity TEXT NOT NULL,
 recorded_at INTEGER NOT NULL,
 evidence_start INTEGER NOT NULL, evidence_end INTEGER NOT NULL,
 UNIQUE(memory_id,memory_revision,identity)
);
CREATE TABLE memory_text_relationship_history (
 id TEXT NOT NULL REFERENCES memory_text_relationships(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL,
 relationship_json TEXT NOT NULL,
 recorded_at INTEGER NOT NULL,
 PRIMARY KEY(id,revision)
);
CREATE TABLE memory_text_candidate_sources (
 candidate_id TEXT PRIMARY KEY REFERENCES memory_candidates(id) ON DELETE CASCADE,
 memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 memory_revision INTEGER NOT NULL,
 evidence_start INTEGER NOT NULL, evidence_end INTEGER NOT NULL,
 relationship_id TEXT,
 relationship_revision INTEGER
);
CREATE INDEX memory_text_candidate_memory ON memory_text_candidate_sources(memory_id);
CREATE TRIGGER memory_text_source_purge BEFORE DELETE ON memories BEGIN
 UPDATE memory_candidates SET content=NULL, relationship_json=NULL, decision='dismissed', decided_at=unixepoch()*1000
 WHERE decision IS NULL AND id IN (SELECT candidate_id FROM memory_text_candidate_sources WHERE memory_id=OLD.id);
 DELETE FROM memory_text_candidate_sources WHERE memory_id=OLD.id;
 DELETE FROM memory_text_relationship_history WHERE id IN (SELECT id FROM memory_text_relationships WHERE memory_id=OLD.id);
 DELETE FROM memory_text_relationships WHERE memory_id=OLD.id;
END;
"#,
);

#[derive(Debug)]
pub(crate) struct SavedTextSource {
    pub(crate) content: String,
    pub(crate) revision: i64,
    scope: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExtractedMemoryRelationship {
    pub(crate) id: String,
    pub(crate) source_memory_id: String,
    pub(crate) source_memory_revision: i64,
    pub(crate) relationship: MemoryRelationship,
    pub(crate) recorded_at: i64,
    pub(crate) evidence_start: usize,
    pub(crate) evidence_end: usize,
}

#[derive(Debug)]
struct SourcePin {
    memory: String,
    revision: i64,
    start: usize,
    end: usize,
    previous: Option<(String, i64)>,
}

const LIVE: &str = "g.status='active'
    AND (g.valid_from IS NULL OR g.valid_from<=?2)
    AND (g.valid_until IS NULL OR g.valid_until>?2)
    AND NOT EXISTS(SELECT 1 FROM knowledge_document_chunks c WHERE c.memory_id=m.id)";

fn source_on(conn: &Connection, id: &str) -> Result<Option<SavedTextSource>, String> {
    conn.query_row(
        &format!(
            "SELECT m.content,g.revision,g.scope FROM memories m JOIN memory_governance g ON m.id=g.memory_id
            JOIN memory_versions v ON v.memory_id=m.id AND v.revision=g.revision AND v.content=m.content
            WHERE m.id=?1 AND {LIVE}"
        ),
        params![id, chrono::Utc::now().timestamp_millis()],
        |row| Ok(SavedTextSource { content:row.get(0)?, revision:row.get(1)?, scope:row.get(2)? }),
    ).optional().map_err(|error| error.to_string())
}

fn parse_relationship(column: usize, json: String) -> rusqlite::Result<MemoryRelationship> {
    serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn read_pin(conn: &Connection, candidate: &str) -> Result<Option<SourcePin>, String> {
    conn.query_row(
        "SELECT memory_id,memory_revision,evidence_start,evidence_end,relationship_id,relationship_revision
         FROM memory_text_candidate_sources WHERE candidate_id=?1",
        [candidate],
        |row| Ok(SourcePin {
            memory:row.get(0)?, revision:row.get(1)?, start:row.get(2)?, end:row.get(3)?,
            previous:row.get::<_,Option<String>>(4)?.zip(row.get::<_,Option<i64>>(5)?),
        }),
    ).optional().map_err(|error| error.to_string())
}

fn exact_excerpt(text: &str, start: usize, end: usize) -> Option<String> {
    let size = end.checked_sub(start)?;
    if size == 0 || size > 2000 {
        return None;
    }
    let quote = text.chars().skip(start).take(size).collect::<String>();
    (quote.chars().count() == size).then_some(quote)
}

fn supported(quote: &str, relation: &MemoryRelationship) -> bool {
    quote.contains(&relation.subject)
        && quote.contains(&relation.object)
        && (relation.occurred_at.is_empty() || quote.contains(&relation.occurred_at))
        && !crate::safety::contains_sensitive_content(&relation.content())
}

fn pinned_excerpt(conn: &Connection, pin: &SourcePin) -> Result<Option<(String, String)>, String> {
    let Some(source) = source_on(conn, &pin.memory)?.filter(|source| {
        source.revision == pin.revision
            && !crate::safety::contains_sensitive_content(&source.content)
    }) else {
        return Ok(None);
    };
    if let Some((id, revision)) = &pin.previous {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM memory_text_relationships WHERE id=?1 AND revision=?2 AND memory_id=?3 AND memory_revision=?4)",
            params![id, revision, pin.memory, pin.revision], |row| row.get(0),
        ).map_err(|error| error.to_string())?;
        if !exists {
            return Ok(None);
        }
    }
    Ok(exact_excerpt(&source.content, pin.start, pin.end).map(|quote| (quote, source.scope)))
}

pub(super) fn resolve_saved_text(
    conn: &Connection,
    candidate: &mut CandidateReview,
) -> Result<(), String> {
    let Some(pin) = read_pin(conn, &candidate.id)? else {
        return Ok(());
    };
    let source = pinned_excerpt(conn, &pin)?;
    candidate.source_kind = "memory_version";
    candidate.source_removed = source.is_none();
    if let Some((quote, scope)) = source {
        candidate.source_excerpt = Some(quote);
        candidate.source_memory_id = Some(pin.memory);
        candidate.source_memory_revision = Some(pin.revision);
        candidate.source_memory_scope = Some(scope);
    }
    Ok(())
}

fn insert_candidate(
    tx: &Transaction<'_>,
    pin: &SourcePin,
    relationship: &MemoryRelationship,
) -> Result<Option<String>, String> {
    if let Some((id, revision)) = &pin.previous {
        let pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM memory_text_candidate_sources s JOIN memory_candidates c ON c.id=s.candidate_id
             WHERE s.relationship_id=?1 AND s.relationship_revision=?2 AND c.decision IS NULL)",
            params![id,revision], |row| row.get(0),
        ).map_err(|error| error.to_string())?;
        if pending {
            return Ok(None);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let fingerprint = content_hash(
        &serde_json::json!([
            "memory_version",
            pin.memory,
            pin.revision,
            relationship.identity(),
            pin.previous.as_ref().map(|_| &id)
        ])
        .to_string(),
    );
    let relation_json = serde_json::to_string(relationship).map_err(|error| error.to_string())?;
    if tx.execute("INSERT INTO memory_candidates(id,category,content,fingerprint,created_at,relationship_json)
        VALUES(?1,'fact',?2,?3,?4,?5) ON CONFLICT(fingerprint) DO NOTHING",
        params![id,relationship.content(),fingerprint,chrono::Utc::now().timestamp_millis(),relation_json],
    ).map_err(|error| error.to_string())? == 0 { return Ok(None); }
    tx.execute(
        "INSERT INTO memory_text_candidate_sources VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            id,
            pin.memory,
            pin.revision,
            pin.start,
            pin.end,
            pin.previous.as_ref().map(|x| &x.0),
            pin.previous.as_ref().map(|x| x.1)
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(Some(id))
}

pub(super) fn accept_saved_text(
    tx: &Transaction<'_>,
    candidate: &str,
    relationship: Option<MemoryRelationship>,
) -> Result<String, String> {
    let pin = read_pin(tx, candidate)?.ok_or("memory_candidate_source_changed")?;
    let (quote, _) = pinned_excerpt(tx, &pin)?.ok_or("memory_candidate_source_changed")?;
    let relationship = relationship
        .and_then(MemoryRelationship::normalized)
        .filter(|relation| supported(&quote, relation))
        .ok_or("saved_text_relationship_unsupported")?;
    let identity = content_hash(&relationship.identity().to_string());
    let json = serde_json::to_string(&relationship).map_err(|error| error.to_string())?;
    let duplicate: Option<String> = tx.query_row(
        "SELECT id FROM memory_text_relationships WHERE memory_id=?1 AND memory_revision=?2 AND identity=?3",
        params![pin.memory,pin.revision,identity], |row| row.get(0),
    ).optional().map_err(|error| error.to_string())?;
    let now = chrono::Utc::now().timestamp_millis();
    if let Some((id, revision)) = pin.previous {
        if duplicate.is_some_and(|duplicate| duplicate != id) {
            return Err("saved_text_relationship_exists".into());
        }
        if tx.execute(
            "UPDATE memory_text_relationships SET revision=revision+1,relationship_json=?2,identity=?3,recorded_at=?4
             WHERE id=?1 AND relationship_json<>?2 AND revision=?5",
            params![id,json,identity,now,revision],
        ).map_err(|error| error.to_string())? > 0 {
            tx.execute("INSERT INTO memory_text_relationship_history VALUES(?1,?2,?3,?4)", params![id,revision+1,json,now])
                .map_err(|error| error.to_string())?;
        }
    } else {
        if duplicate.is_some() {
            return Err("saved_text_relationship_exists".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO memory_text_relationships VALUES(?1,?2,?3,1,?4,?5,?6,?7,?8)",
            params![
                id,
                pin.memory,
                pin.revision,
                json,
                identity,
                now,
                pin.start,
                pin.end
            ],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO memory_text_relationship_history VALUES(?1,1,?2,?3)",
            params![id, json, now],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(pin.memory)
}

impl Database {
    pub(crate) fn saved_text_source(
        &self,
        id: &str,
    ) -> Result<Option<super::SavedTextSource>, String> {
        source_on(&self.conn(), id)
    }

    pub(crate) fn stage_saved_text_relationships(
        &self,
        id: &str,
        revision: i64,
        relationships: Vec<(MemoryRelationship, usize, usize)>,
    ) -> Result<Vec<String>, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let source = source_on(&tx, id)?
            .filter(|source| {
                source.revision == revision
                    && !crate::safety::contains_sensitive_content(&source.content)
            })
            .ok_or("memory_candidate_source_changed")?;
        let mut created = Vec::new();
        for (relation, start, end) in relationships {
            let Some((mut relation, quote)) =
                relation
                    .normalized()
                    .zip(exact_excerpt(&source.content, start, end))
            else {
                continue;
            };
            if !supported(&quote, &relation) {
                continue;
            }
            relation.certainty = RelationshipCertainty::Inferred;
            let pin = SourcePin {
                memory: id.into(),
                revision,
                start,
                end,
                previous: None,
            };
            if let Some(id) = insert_candidate(&tx, &pin, &relation)? {
                created.push(id);
            }
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(created)
    }

    pub(crate) fn stage_saved_text_correction(
        &self,
        id: &str,
        relationship_id: &str,
    ) -> Result<Vec<String>, String> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let (pin, mut relationship) = tx
            .query_row(
                "SELECT memory_revision,evidence_start,evidence_end,revision,relationship_json
             FROM memory_text_relationships WHERE id=?1 AND memory_id=?2",
                params![relationship_id, id],
                |row| {
                    Ok((
                        SourcePin {
                            memory: id.into(),
                            revision: row.get(0)?,
                            start: row.get(1)?,
                            end: row.get(2)?,
                            previous: Some((relationship_id.into(), row.get(3)?)),
                        },
                        parse_relationship(4, row.get(4)?)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or("saved_text_relationship_missing")?;
        pinned_excerpt(&tx, &pin)?.ok_or("memory_candidate_source_changed")?;
        relationship.certainty = RelationshipCertainty::Inferred;
        let created = insert_candidate(&tx, &pin, &relationship)?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok(created.into_iter().collect())
    }

    pub(crate) fn effective_extracted_relationships_on(
        conn: &Connection,
        revisions: &std::collections::HashMap<String, i64>,
    ) -> Result<Vec<ExtractedMemoryRelationship>, String> {
        if revisions.is_empty() {
            return Ok(Vec::new());
        }
        let revisions = serde_json::to_string(revisions).map_err(|error| error.to_string())?;
        let mut statement = conn.prepare(
            "SELECT r.id,r.memory_id,r.memory_revision,r.relationship_json,r.recorded_at,r.evidence_start,r.evidence_end
             FROM memory_text_relationships r
             JOIN json_each(?1) authorized ON r.memory_id=authorized.key AND r.memory_revision=authorized.value
             ORDER BY r.recorded_at,r.id",
        ).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([revisions], read_extracted_relationship)
            .map_err(|error| error.to_string())?;
        rows.collect::<rusqlite::Result<_>>()
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    pub(crate) fn extracted_memory_relationships_on(
        conn: &Connection,
        ids: &[String],
    ) -> Result<Vec<ExtractedMemoryRelationship>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = serde_json::to_string(ids).map_err(|error| error.to_string())?;
        let mut stmt = conn.prepare(&format!(
            "SELECT r.id,r.memory_id,r.memory_revision,r.relationship_json,r.recorded_at,r.evidence_start,r.evidence_end
             FROM memory_text_relationships r JOIN memories m ON m.id=r.memory_id
             JOIN memory_governance g ON g.memory_id=m.id AND r.memory_revision=g.revision
             WHERE m.id IN (SELECT value FROM json_each(?1)) AND {LIVE} ORDER BY r.recorded_at,r.id"
        )).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(
                params![ids, chrono::Utc::now().timestamp_millis()],
                read_extracted_relationship,
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())
    }
}

fn read_extracted_relationship(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ExtractedMemoryRelationship> {
    Ok(ExtractedMemoryRelationship {
        id: row.get(0)?,
        source_memory_id: row.get(1)?,
        source_memory_revision: row.get(2)?,
        relationship: parse_relationship(3, row.get(3)?)?,
        recorded_at: row.get(4)?,
        evidence_start: row.get(5)?,
        evidence_end: row.get(6)?,
    })
}
