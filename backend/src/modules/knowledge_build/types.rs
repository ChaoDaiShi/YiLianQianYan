use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    Quick,
    Deep,
}

impl BuildMode {
    pub fn rounds(self) -> usize {
        match self {
            Self::Quick => 1,
            Self::Deep => 3,
        }
    }
    pub fn source_limit(self) -> usize {
        match self {
            Self::Quick => 12,
            Self::Deep => 40,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgress {
    pub stage: String,
    pub queries_completed: usize,
    pub queries_total: usize,
    pub sources_collected: usize,
    pub rounds_completed: usize,
    pub rounds_total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSource {
    pub id: String,
    pub title: String,
    pub url: String,
    pub query: String,
    pub excerpt: String,
    /// What the search provider actually returned; never implies full-page retrieval.
    pub evidence_kind: String,
    pub source_type: String,
    /// Reading priority inferred from visible material, not publisher certification.
    pub source_priority: String,
    pub source_type_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRelation {
    Prerequisite,
    Related,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeEdge {
    pub from: String,
    pub to: String,
    pub relation: EdgeRelation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStance {
    Supports,
    Contradicts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimEvidence {
    pub source_id: String,
    pub quote: String,
    pub stance: EvidenceStance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeClaim {
    pub id: String,
    pub text: String,
    /// Derived from validated citations, never accepted as a model confidence score.
    pub status: String,
    pub evidence: Vec<ClaimEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contradiction {
    pub claim_id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResult {
    pub summary: String,
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<KnowledgeEdge>,
    pub claims: Vec<KnowledgeClaim>,
    pub contradictions: Vec<Contradiction>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBuild {
    pub id: String,
    pub topic: String,
    pub mode: BuildMode,
    pub status: String,
    pub progress: BuildProgress,
    pub sources: Vec<BuildSource>,
    pub result: Option<BuildResult>,
    pub error: Option<String>,
    pub document_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl KnowledgeBuild {
    pub fn new(topic: String, mode: BuildMode) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            topic,
            mode,
            status: "queued".into(),
            progress: BuildProgress {
                stage: "等待检索".into(),
                queries_completed: 0,
                queries_total: 4 * mode.rounds(),
                sources_collected: 0,
                rounds_completed: 0,
                rounds_total: mode.rounds(),
            },
            sources: Vec::new(),
            result: None,
            error: None,
            document_id: None,
            created_at: now,
            updated_at: now,
        }
    }
    pub fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "queued" | "researching" | "synthesizing"
        )
    }
}
