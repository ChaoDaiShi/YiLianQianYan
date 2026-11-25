use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::{endpoint_revisions, events, link_on, storage};
use crate::db::{memory_governance::content_hash, Database};

#[derive(Deserialize)]
pub(crate) struct StageLinkCorrection {
    pub target_memory_id: String,
    pub content: String,
    pub reason: String,
    pub expected_revision: i64,
    pub source_version_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CorrectionSource {
    pub relation_id: String,
    pub label: String,
    pub target_memory_id: String,
    pub target_revision: i64,
    pub reason: String,
    pub source_version_id: Option<String>,
}

fn correction_source_on(
    conn: &Connection,
    id: &str,
) -> Result<Option<(CorrectionSource, bool)>, String> {
    let source = conn.query_row(
        "SELECT c.relation_id,l.label,c.target_memory_id,c.target_revision,c.reason,c.relation_revision,
                l.status,l.revision,l.source_memory_id,l.target_memory_id
         FROM memory_graph_corrections c JOIN memory_graph_links l ON l.id=c.relation_id
         WHERE c.candidate_id=?1 AND c.reason IS NOT NULL",
        [id],
        |row| Ok((CorrectionSource {
            relation_id:row.get(0)?, label:row.get(1)?, target_memory_id:row.get(2)?,
            target_revision:row.get(3)?, reason:row.get(4)?, source_version_id:None,
        }, row.get::<_, i64>(5)?, row.get::<_, String>(6)?, row.get::<_, i64>(7)?,
        row.get::<_, String>(8)?, row.get::<_, String>(9)?)),
    ).optional().map_err(storage)?;
    source
        .map(|(mut source, reviewed_relation, status, revision, a, b)| {
            let valid = status == "confirmed"
                && revision == reviewed_relation
                && endpoint_revisions(conn, &a, &b).is_ok_and(|(ra, rb)| {
                    source.target_revision == if a == source.target_memory_id { ra } else { rb }
                });
            if let Some((id, revision)) = events::correction_pin(conn, id)? {
                if !events::eligible_source_version(conn, &id, revision, &source.target_memory_id)?
                {
                    return Ok((source, false));
                }
                source.source_version_id = Some(format!("{id}:{revision}"));
            }
            Ok((source, valid))
        })
        .transpose()
}

pub(in crate::db) fn resolve_correction(
    conn: &Connection,
    candidate: &mut crate::db::memory_candidates::CandidateReview,
) -> Result<(), String> {
    if let Some((source, valid)) = correction_source_on(conn, &candidate.id)? {
        candidate.source_kind = "related_memory_correction";
        candidate.source_removed = !valid;
        candidate.source_title = valid.then(|| source.label.clone());
        if valid {
            candidate.source_excerpt = conn
                .query_row(
                    "SELECT content FROM memories WHERE id=?1",
                    [&source.target_memory_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage)?;
            candidate.correction = Some(source);
        }
    }
    Ok(())
}

impl Database {
    pub(crate) fn stage_link_correction(
        &self,
        id: &str,
        input: StageLinkCorrection,
    ) -> Result<Option<String>, String> {
        let content = input.content.trim();
        let reason = input.reason.trim();
        if content.is_empty()
            || content.chars().count() > 2000
            || reason.is_empty()
            || reason.chars().count() > 500
            || crate::safety::contains_sensitive_content(content)
            || crate::safety::contains_sensitive_content(reason)
        {
            return Err("invalid_memory_correction".into());
        }
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let relation = link_on(&tx, id)?;
        if relation.status != "confirmed" {
            return Err("memory_link_not_confirmed".into());
        }
        let (a, b) =
            endpoint_revisions(&tx, &relation.source_memory_id, &relation.target_memory_id)?;
        let (revision, related_memory) = if input.target_memory_id == relation.source_memory_id {
            (a, &relation.target_memory_id)
        } else if input.target_memory_id == relation.target_memory_id {
            (b, &relation.source_memory_id)
        } else {
            return Err("invalid_memory_link".into());
        };
        if revision != input.expected_revision {
            return Err("memory_revision_conflict".into());
        }
        let pin = if let Some(source_version) = &input.source_version_id {
            let source_revision = source_version
                .rsplit_once(':')
                .and_then(|(id, value)| {
                    (id == related_memory)
                        .then(|| value.parse::<i64>().ok())
                        .flatten()
                })
                .ok_or("memory_link_source_version_unavailable")?;
            if !events::eligible_source_version(
                &tx,
                related_memory,
                source_revision,
                &input.target_memory_id,
            )? {
                return Err("memory_link_source_version_unavailable".into());
            }
            Some(source_revision)
        } else {
            None
        };
        let (old, category): (String, String) = tx
            .query_row(
                "SELECT content,category FROM memories WHERE id=?1",
                [&input.target_memory_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage)?;
        if old.trim() == content {
            return Ok(None);
        }
        let fingerprint = content_hash(
            &serde_json::json!([
                "related_memory_correction",
                id,
                relation.revision,
                input.target_memory_id,
                revision,
                content,
                pin,
            ])
            .to_string(),
        );
        let candidate = uuid::Uuid::new_v4().to_string();
        if tx
            .execute(
                "INSERT INTO memory_candidates(id,category,content,fingerprint,created_at)
             VALUES (?1,?2,?3,?4,?5) ON CONFLICT(fingerprint) DO NOTHING",
                params![
                    candidate,
                    category,
                    content,
                    fingerprint,
                    chrono::Utc::now().timestamp_millis()
                ],
            )
            .map_err(storage)?
            == 0
        {
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO memory_graph_corrections VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                candidate,
                id,
                relation.revision,
                input.target_memory_id,
                revision,
                reason
            ],
        )
        .map_err(storage)?;
        if let Some(pin) = pin {
            tx.execute(
                "INSERT INTO memory_graph_correction_pins VALUES (?1,?2,?3)",
                params![candidate, related_memory, pin],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(Some(candidate))
    }
}
