//! Ranked excerpts from the complete canonical imported corpus. Derived memory
//! chunks and their recency limits are deliberately outside this read path.
use std::collections::{HashSet, VecDeque};

use serde::Serialize;

use super::super::{memories::keyword_score, Database};
use super::chunking::{excerpt, Layout, TextMatch, CHUNK_CHARS};
use crate::utils::text::is_cjk;

const WINDOW_OVERLAP: usize = 240;
const EXCERPT_CHARS: usize = 500;
const EXCERPTS_PER_DOCUMENT: usize = 2;

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeContext {
    pub document_id: String,
    pub name: String,
    pub content: String,
    pub score: f64,
    pub version: String,
    pub start_char: usize,
    pub end_char: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

struct Candidate {
    context: KnowledgeContext,
    anchor: usize,
}

struct Occurrence {
    hit: TextMatch,
    score: f64,
}

fn compare_occurrences(left: &Occurrence, right: &Occurrence) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.hit.character.cmp(&right.hit.character))
}

impl Database {
    /// Every canonical document is scored before the final top K is returned.
    /// At most one source document and its bounded candidates are held while
    /// scanning; nearby hits and repeated overlapping windows collapse to one.
    pub fn knowledge_context(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeContext>, String> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let terms = match_terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT id,name,text_content,mime_type FROM knowledge_documents ORDER BY id")
            .map_err(|error| error.to_string())?;
        let mut rows = statement.query([]).map_err(|error| error.to_string())?;
        let mut best: Vec<Candidate> = Vec::new();
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            let id: String = row.get(0).map_err(|error| error.to_string())?;
            let name: String = row.get(1).map_err(|error| error.to_string())?;
            let text: String = row.get(2).map_err(|error| error.to_string())?;
            let mime: String = row.get(3).map_err(|error| error.to_string())?;
            let document =
                document_candidates(&id, &name, &text, Layout::from_mime(&mime), query, &terms);
            for candidate in document {
                best.push(candidate);
                sort_candidates(&mut best);
                best.truncate(limit);
            }
        }
        Ok(best
            .into_iter()
            .map(|candidate| candidate.context)
            .collect())
    }
}

fn document_candidates(
    document_id: &str,
    name: &str,
    text: &str,
    layout: Layout,
    query: &str,
    terms: &[String],
) -> Vec<Candidate> {
    // Each entry retains the exact source character and byte offset. Overlap
    // catches terms crossing a scoring window or a stored chunk boundary.
    let mut remaining = text.char_indices().enumerate();
    let mut window: VecDeque<(usize, usize, char)> = VecDeque::new();
    let mut best: Vec<Occurrence> = Vec::new();
    loop {
        while window.len() < CHUNK_CHARS {
            let Some((character_index, (byte_index, character))) = remaining.next() else {
                break;
            };
            window.push_back((character_index, byte_index, character));
        }
        if window.is_empty() {
            break;
        }
        let content: String = window.iter().map(|entry| entry.2).collect();
        let score = keyword_score(query, &content);
        if score > 0.0 {
            if let Some((local_anchor, length)) = match_anchor(&content, terms) {
                let (anchor, source_byte, _) = window[local_anchor];
                let candidate = Occurrence {
                    hit: TextMatch {
                        byte: source_byte,
                        character: anchor,
                        length,
                    },
                    score,
                };
                // Two windows containing the same occurrence have the same
                // source anchor; overlapping 500-char excerpts are near hits.
                if let Some(index) = best
                    .iter()
                    .position(|existing| existing.hit.character.abs_diff(anchor) < EXCERPT_CHARS)
                {
                    if compare_occurrences(&candidate, &best[index]).is_lt() {
                        best[index] = candidate;
                    }
                } else {
                    best.push(candidate);
                }
                best.sort_by(compare_occurrences);
                best.truncate(EXCERPTS_PER_DOCUMENT);
            }
        }
        if window.len() < CHUNK_CHARS {
            break;
        }
        for _ in 0..CHUNK_CHARS - WINDOW_OVERLAP {
            window.pop_front();
        }
    }
    if best.is_empty() {
        return Vec::new();
    }
    let version = super::knowledge_text_version(text);
    best.into_iter()
        .map(|occurrence| {
            let source = excerpt(
                text,
                layout,
                occurrence.hit,
                EXCERPT_CHARS / 2,
                EXCERPT_CHARS,
            );
            Candidate {
                anchor: occurrence.hit.character,
                context: KnowledgeContext {
                    document_id: document_id.into(),
                    name: name.into(),
                    content: source.content.into(),
                    score: occurrence.score,
                    version: version.clone(),
                    start_char: source.start_char,
                    end_char: source.start_char + source.content.chars().count(),
                    section: source
                        .section
                        .map(|heading| crate::utils::text::truncate_chars(heading, 120)),
                },
            }
        })
        .collect()
}

fn sort_candidates(candidates: &mut [Candidate]) {
    candidates.sort_by(|left, right| {
        right
            .context
            .score
            .total_cmp(&left.context.score)
            .then_with(|| left.context.document_id.cmp(&right.context.document_id))
            .then_with(|| left.anchor.cmp(&right.anchor))
    });
}

/// Query terms select an actual matching location, instead of returning the
/// beginning of an otherwise relevant document. Longer query phrases win.
fn match_terms(query: &str) -> Vec<String> {
    let normalized = query.to_lowercase();
    let mut terms = vec![normalized.clone()];
    for token in normalized.split(|character: char| !character.is_alphanumeric()) {
        if token.chars().count() >= 2 {
            terms.push(token.into());
        }
    }
    // Adjacent CJK characters provide natural-language anchors without a new
    // tokenizer dependency; longer matching fragments prefer specific phrases.
    for segment in normalized.split(|character: char| !is_cjk(character)) {
        let characters: Vec<char> = segment.chars().collect();
        if characters.len() == 1 && normalized.chars().count() == 1 {
            terms.push(segment.into());
        }
        for width in 2..=characters.len().min(8) {
            for part in characters.windows(width) {
                terms.push(part.iter().collect());
            }
        }
    }
    let mut seen = HashSet::new();
    terms.retain(|term| !term.trim().is_empty() && seen.insert(term.clone()));
    terms.sort_by(|left, right| {
        right
            .chars()
            .count()
            .cmp(&left.chars().count())
            .then_with(|| left.cmp(right))
    });
    terms
}

fn match_anchor(content: &str, terms: &[String]) -> Option<(usize, usize)> {
    // Unicode case folding can expand a source character (e.g. capital I with
    // dot); keep the mapping so a normalized match always points into source.
    let mut normalized = String::new();
    let mut original_indices = Vec::new();
    for (source_index, character) in content.chars().enumerate() {
        for lower in character.to_lowercase() {
            normalized.push(lower);
            original_indices.push(source_index);
        }
    }
    for term in terms {
        if let Some(byte_index) = normalized.find(term) {
            let normalized_index = normalized[..byte_index].chars().count();
            let start = original_indices[normalized_index];
            let end = original_indices[normalized_index + term.chars().count() - 1];
            return Some((start, end - start + 1));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Fixture {
        db: Option<Database>,
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("knowledge-context-{}.db", uuid::Uuid::new_v4()));
            Self {
                db: Some(Database::new(&path).unwrap()),
                path,
            }
        }
        fn db(&self) -> &Database {
            self.db.as_ref().unwrap()
        }
        fn import(&self, name: &str, text: &str) -> super::super::KnowledgeDocument {
            self.db()
                .import_knowledge(name, name, "text/plain", name, text.len() as i64, text)
                .unwrap()
                .0
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.db.take());
            std::fs::remove_file(&self.path).unwrap();
        }
    }

    #[test]
    fn canonical_tail_beyond_five_hundred_chunks_enters_context_and_source_excerpt() {
        let fixture = Fixture::new();
        let text = format!(
            "{}\n尾部专属编号 TAIL_NEEDLE_9834，必须从原文返回。",
            "填充资料。".repeat(150_000)
        );
        let document = fixture.import("长资料.txt", &text);
        assert!(document.chunk_count > 500);
        let results = fixture
            .db()
            .knowledge_context("TAIL_NEEDLE_9834", 6)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id, document.id);
        assert!(results[0].content.contains("TAIL_NEEDLE_9834"));
        assert!(text.contains(&results[0].content));
        assert!(results[0].content.chars().count() <= EXCERPT_CHARS);
    }

    #[test]
    fn overlapping_windows_find_boundary_phrase_without_duplicate_hits() {
        let fixture = Fixture::new();
        let text = format!(
            "{}CROSS_BOUNDARY_NEEDLE{}",
            "x".repeat(1195),
            "y".repeat(1000)
        );
        fixture.import("边界.txt", &text);
        let results = fixture
            .db()
            .knowledge_context("CROSS_BOUNDARY_NEEDLE", 6)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("CROSS_BOUNDARY_NEEDLE"));
        assert!(text.contains(&results[0].content));
    }

    #[test]
    fn completes_corpus_ranking_and_limits_repeated_source_windows() {
        let fixture = Fixture::new();
        let repeated = format!("{}RELEVANT_MARKER\n", "padding ".repeat(250)).repeat(8);
        let first = fixture.import("重复资料.txt", &repeated);
        let second = fixture.import("另一来源.txt", "另一个实际来源 RELEVANT_MARKER 位于此处。");
        let results = fixture
            .db()
            .knowledge_context("RELEVANT_MARKER", 6)
            .unwrap();
        assert!(
            results
                .iter()
                .filter(|result| result.document_id == first.id)
                .count()
                <= EXCERPTS_PER_DOCUMENT
        );
        assert!(results.iter().any(|result| result.document_id == second.id));
        assert!(results
            .iter()
            .all(|result| result.content.contains("RELEVANT_MARKER")));
        assert_eq!(
            fixture
                .db()
                .knowledge_context("RELEVANT_MARKER", 1)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn chinese_question_excerpt_centers_actual_phrase_and_unicode_remains_source_text() {
        let fixture = Fixture::new();
        let text = format!(
            "{}尾部专属编号为涟-9834，验证结果已记录。",
            "普通说明𠮷。".repeat(1000)
        );
        fixture.import("中文资料.txt", &text);
        let results = fixture
            .db()
            .knowledge_context("请查找尾部专属编号", 2)
            .unwrap();
        assert!(!results.is_empty());
        assert!(results[0].content.contains("尾部专属编号为涟-9834"));
        assert!(text.contains(&results[0].content));
        let source = &results[0];
        assert_eq!(
            source.version,
            crate::db::knowledge::knowledge_text_version(&text)
        );
        let anchored = text
            .chars()
            .skip(source.start_char)
            .take(source.end_char - source.start_char)
            .collect::<String>();
        assert_eq!(anchored, source.content);
        assert!(results[0].content.chars().count() <= EXCERPT_CHARS);
        assert!(fixture.db().knowledge_context("", 2).unwrap().is_empty());
        assert!(fixture
            .db()
            .knowledge_context("尾部专属编号", 0)
            .unwrap()
            .is_empty());
    }
}
