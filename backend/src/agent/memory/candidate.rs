// ============================================================
// Memory candidate — a proposed memory produced by reflection,
// before validation and persistence.
// ============================================================

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::task::model::TaskId;

/// Memory category, mirroring the persisted `Memory.category` string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Fact,
    Preference,
    Knowledge,
    Note,
}

impl MemoryCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Knowledge => "knowledge",
            Self::Note => "note",
        }
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryCategory {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fact" => Ok(Self::Fact),
            "preference" => Ok(Self::Preference),
            "knowledge" => Ok(Self::Knowledge),
            "note" => Ok(Self::Note),
            other => Err(format!("unknown memory category: {other}")),
        }
    }
}

/// A proposed memory: the raw material a learning loop wants to persist.
///
/// `confidence` is a float in `0.0..=1.0`; the write policy rejects candidates
/// below its minimum threshold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub category: MemoryCategory,
    pub content: String,
    pub source_task_id: Option<TaskId>,
    pub agent_name: String,
    pub confidence: f32,
}

impl MemoryCandidate {
    pub fn new(
        category: MemoryCategory,
        content: impl Into<String>,
        source_task_id: Option<TaskId>,
        agent_name: impl Into<String>,
        confidence: f32,
    ) -> Self {
        Self {
            category,
            content: content.into(),
            source_task_id,
            agent_name: agent_name.into(),
            confidence,
        }
    }
}
