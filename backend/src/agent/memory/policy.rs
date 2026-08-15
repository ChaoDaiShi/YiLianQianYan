// ============================================================
// Memory write policy — safe, conservative validation of candidates.
//
// A candidate is only persisted when it is non-empty, bounded in length, above
// the confidence threshold, free of obvious secrets, and not a near-duplicate
// of an existing memory. Validation is deterministic and secret-aware.
// ============================================================

use thiserror::Error;

use super::candidate::MemoryCandidate;
use crate::db::Memory;

/// Default maximum characters of a single stored memory content.
pub const DEFAULT_MAX_CONTENT_CHARS: usize = 2000;
/// Default minimum characters of a single stored memory content.
pub const DEFAULT_MIN_CONTENT_CHARS: usize = 3;
/// Default minimum confidence to accept a candidate (0.0..=1.0).
pub const DEFAULT_MIN_CONFIDENCE: f32 = 0.5;

/// Conservative secret-like substrings rejected from memory content.
const SECRET_MARKERS: &[&str] = &[
    "api_key",
    "api key",
    "apikey",
    "authorization",
    "bearer ",
    "token=",
    "password",
    "secret",
    "sk-",
];

#[derive(Debug, Clone)]
pub struct MemoryWritePolicy {
    pub max_content_chars: usize,
    pub min_content_chars: usize,
    pub min_confidence: f32,
}

impl Default for MemoryWritePolicy {
    fn default() -> Self {
        Self {
            max_content_chars: DEFAULT_MAX_CONTENT_CHARS,
            min_content_chars: DEFAULT_MIN_CONTENT_CHARS,
            min_confidence: DEFAULT_MIN_CONFIDENCE,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("memory content is empty")]
    EmptyContent,
    #[error("memory content is too short")]
    ContentTooShort,
    #[error("memory content exceeds {0} characters")]
    ContentTooLong(usize),
    #[error("memory confidence is below the threshold")]
    LowConfidence,
    #[error("memory content appears to contain a secret")]
    ContainsSecret,
    #[error("memory is a duplicate of existing memory")]
    Duplicate,
}

/// Validate a candidate against the policy and existing memories.
pub fn validate_candidate(
    candidate: &MemoryCandidate,
    policy: &MemoryWritePolicy,
    existing: &[Memory],
) -> Result<(), ValidationError> {
    let content = candidate.content.trim();
    if content.is_empty() {
        return Err(ValidationError::EmptyContent);
    }
    let char_count = content.chars().count();
    if char_count < policy.min_content_chars {
        return Err(ValidationError::ContentTooShort);
    }
    if char_count > policy.max_content_chars {
        return Err(ValidationError::ContentTooLong(policy.max_content_chars));
    }
    if candidate.confidence < policy.min_confidence {
        return Err(ValidationError::LowConfidence);
    }
    if contains_secret(content) {
        return Err(ValidationError::ContainsSecret);
    }
    if is_duplicate(content, existing) {
        return Err(ValidationError::Duplicate);
    }
    Ok(())
}

fn contains_secret(content: &str) -> bool {
    let lower = content.to_lowercase();
    SECRET_MARKERS.iter().any(|marker| lower.contains(marker))
}

/// A near-duplicate is an exact match (case/whitespace-insensitive) or a
/// containment relationship with an existing memory.
fn is_duplicate(content: &str, existing: &[Memory]) -> bool {
    let normalized = normalize(content);
    existing.iter().any(|memory| {
        let existing_norm = normalize(&memory.content);
        normalized == existing_norm
            || existing_norm.contains(&normalized)
            || normalized.contains(&existing_norm)
    })
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
