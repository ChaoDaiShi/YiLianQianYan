//! Read-only semantic neighbors, kept separate from stored correspondences.
use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde::Serialize;

use super::storage;
use crate::db::{memory_governance::current_embedding_vector, Memory};

// This is a layout/reading shortlist, not the threshold for accepting a relation.
const MIN_SIMILARITY: f64 = 0.65;
const NEIGHBORS_PER_MEMORY: usize = 4;

#[derive(Debug, Serialize)]
pub(crate) struct MemoryGraphSemantic {
    pub configured: bool,
    pub indexed_memory_ids: Vec<String>,
    pub similarities: Vec<MemorySimilarity>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemorySimilarity {
    pub source_memory_id: String,
    pub target_memory_id: String,
    pub score: f64,
}

struct IndexedMemory {
    id: String,
    scope: String,
    vector: Vec<f64>,
}

fn unit_vector(vector: Vec<f32>) -> Vec<f64> {
    let norm = vector
        .iter()
        .map(|&value| f64::from(value).powi(2))
        .sum::<f64>()
        .sqrt();
    vector
        .into_iter()
        .map(|value| f64::from(value) / norm)
        .collect()
}

pub(super) fn read_semantic(
    conn: &Connection,
    memories: &[Memory],
    model: Option<&str>,
    withdrawn: bool,
) -> Result<MemoryGraphSemantic, String> {
    let mut semantic = MemoryGraphSemantic {
        configured: model.is_some(),
        indexed_memory_ids: Vec::new(),
        similarities: Vec::new(),
    };
    let Some(model) = model.filter(|_| !withdrawn && !memories.is_empty()) else {
        return Ok(semantic);
    };
    // IDs are restricted to this snapshot page; the governing revision,
    // validity and active model generation are checked before vectors are read.
    let memories_by_id = memories
        .iter()
        .map(|memory| (memory.id.as_str(), memory))
        .collect::<HashMap<_, _>>();
    let ids = serde_json::to_string(&memories_by_id.keys().collect::<Vec<_>>())
        .map_err(|error| error.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT g.memory_id,g.scope,g.embedding_hash,g.embedding_dimension
         FROM memory_governance g
         JOIN memory_embedding_config cfg ON cfg.id=1 AND cfg.model=?2 AND cfg.generation=g.embedding_generation
         WHERE g.memory_id IN (SELECT value FROM json_each(?1))
           AND g.status='active' AND g.embedding_revision=g.revision AND g.embedding_model=?2
           AND g.embedding_hash IS NOT NULL AND g.embedding_dimension>0
           AND (g.valid_from IS NULL OR g.valid_from<=?3) AND (g.valid_until IS NULL OR g.valid_until>?3)
           AND (g.scope='profile'
                OR (substr(g.scope,1,13)='conversation:' AND EXISTS(SELECT 1 FROM conversations c WHERE c.id=substr(g.scope,14)))
                OR (substr(g.scope,1,10)='workspace:' AND EXISTS(SELECT 1 FROM workspaces w WHERE w.id=substr(g.scope,11) AND w.status='active')))
         ORDER BY g.memory_id",
    ).map_err(storage)?;
    let rows = stmt
        .query_map(
            params![ids, model, chrono::Utc::now().timestamp_millis()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, usize>(3)?,
                ))
            },
        )
        .map_err(storage)?;
    let mut indexed = Vec::new();
    for row in rows {
        let (id, scope, hash, dimension) = row.map_err(storage)?;
        let Some(memory) = memories_by_id.get(id.as_str()) else {
            continue;
        };
        let Some(vector) = current_embedding_vector(
            &memory.content,
            memory.embedding.as_deref(),
            Some(hash.as_str()),
            Some(dimension),
        ) else {
            continue;
        };
        semantic.indexed_memory_ids.push(id.clone());
        indexed.push(IndexedMemory {
            id,
            scope,
            vector: unit_vector(vector),
        });
    }
    semantic.similarities = ranked_similarities(&indexed);
    Ok(semantic)
}

fn ranked_similarities(indexed: &[IndexedMemory]) -> Vec<MemorySimilarity> {
    let mut ranked = Vec::new();
    for (index, source) in indexed.iter().enumerate() {
        for (target_index, target) in indexed.iter().enumerate().skip(index + 1) {
            if source.scope != target.scope || source.vector.len() != target.vector.len() {
                continue;
            }
            let score = source
                .vector
                .iter()
                .zip(&target.vector)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                .clamp(-1.0, 1.0);
            if score >= MIN_SIMILARITY {
                ranked.push((index, target_index, score));
            }
        }
    }
    ranked.sort_by(|(a1, b1, score1), (a2, b2, score2)| {
        score2.total_cmp(score1).then(a1.cmp(a2)).then(b1.cmp(b2))
    });
    let mut counts = vec![0; indexed.len()];
    let mut shortlist = Vec::new();
    for (a, b, score) in ranked {
        if counts[a] >= NEIGHBORS_PER_MEMORY || counts[b] >= NEIGHBORS_PER_MEMORY {
            continue;
        }
        counts[a] += 1;
        counts[b] += 1;
        shortlist.push(MemorySimilarity {
            source_memory_id: indexed[a].id.clone(),
            target_memory_id: indexed[b].id.clone(),
            score,
        });
    }
    shortlist
}
