use super::{
    migrations::{MigrationOwner, ProductMigrationSpec},
    Database,
};
use crate::task::artifact::ArtifactSource;
use serde::{Deserialize, Serialize};

const SKILL_CANDIDATES: ProductMigrationSpec = ProductMigrationSpec::new(1012,"1012_skill_candidates_versions",MigrationOwner::V1TaskWorld,
"CREATE TABLE skill_candidates (id TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK(revision>0), status TEXT NOT NULL CHECK(status IN ('draft','validated','rejected','confirmed')), source_json TEXT NOT NULL, lesson TEXT NOT NULL, sensitivity TEXT NOT NULL, skill_name TEXT, skill_version INTEGER, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE TABLE skill_candidate_revisions(candidate_id TEXT NOT NULL,revision INTEGER NOT NULL,lesson TEXT NOT NULL,status TEXT NOT NULL,created_at INTEGER NOT NULL,PRIMARY KEY(candidate_id,revision));
CREATE TABLE managed_skill_versions(skill_name TEXT NOT NULL,version INTEGER NOT NULL,candidate_id TEXT NOT NULL,content TEXT NOT NULL,content_hash TEXT NOT NULL,active INTEGER NOT NULL CHECK(active IN(0,1)),created_at INTEGER NOT NULL,PRIMARY KEY(skill_name,version));
CREATE UNIQUE INDEX managed_skill_one_active ON managed_skill_versions(skill_name) WHERE active=1;");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCandidate {
    pub id: String,
    pub revision: u64,
    pub status: String,
    pub sources: Vec<ArtifactSource>,
    pub lesson: String,
    pub sensitivity: String,
    pub skill_name: Option<String>,
    pub skill_version: Option<u32>,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedSkillVersion {
    pub skill_name: String,
    pub version: u32,
    pub candidate_id: String,
    pub content: String,
    pub content_hash: String,
    pub active: bool,
    pub created_at: i64,
}

impl Database {
    pub fn initialize_skill_candidates(&self) -> Result<(), String> {
        self.apply_product_migration(&SKILL_CANDIDATES)
            .map_err(|error| error.to_string())
    }
}
