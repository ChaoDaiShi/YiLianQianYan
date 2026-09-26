//! SQLite row mapping for candidates and their installed versions.
//!
//! Every statement is parameterised and every read maps a row back into a
//! domain type. No policy is applied at this layer — screening and revision
//! checks belong to the callers, so a direct repository call can never be
//! mistaken for a reviewed transition.

use crate::db::{ManagedSkillVersion, SkillCandidate};

pub(crate) fn hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub(crate) fn load_candidate(
    conn: &rusqlite::Connection,
    id: &str,
) -> Result<SkillCandidate, String> {
    conn.query_row("SELECT id,revision,status,source_json,lesson,sensitivity,skill_name,skill_version,created_at,updated_at FROM skill_candidates WHERE id=?1",[id],|row|{
        let sources:String=row.get(3)?;let sources=serde_json::from_str(&sources).map_err(|error|rusqlite::Error::FromSqlConversionFailure(3,rusqlite::types::Type::Text,Box::new(error)))?;
        Ok(SkillCandidate{id:row.get(0)?,revision:row.get(1)?,status:row.get(2)?,sources,lesson:row.get(4)?,sensitivity:row.get(5)?,skill_name:row.get(6)?,skill_version:row.get(7)?,created_at:row.get(8)?,updated_at:row.get(9)?})
    }).map_err(|_|"候选不存在或记录无效".into())
}

pub(crate) fn save_revision(
    conn: &rusqlite::Connection,
    candidate: &SkillCandidate,
) -> Result<(), String> {
    conn.execute("INSERT INTO skill_candidate_revisions(candidate_id,revision,lesson,status,created_at) VALUES (?1,?2,?3,?4,?5)",rusqlite::params![candidate.id,candidate.revision,candidate.lesson,candidate.status,candidate.updated_at]).map_err(|_|"候选修订保存失败")?;
    Ok(())
}

pub(crate) fn versions_on(
    conn: &rusqlite::Connection,
    name: &str,
) -> Result<Vec<ManagedSkillVersion>, String> {
    let mut query=conn.prepare("SELECT skill_name,version,candidate_id,content,content_hash,active,created_at FROM managed_skill_versions WHERE skill_name=?1 ORDER BY version DESC LIMIT 100").map_err(|_|"版本列表不可用")?;
    let versions = query
        .query_map([name], |r| {
            Ok(ManagedSkillVersion {
                skill_name: r.get(0)?,
                version: r.get(1)?,
                candidate_id: r.get(2)?,
                content: r.get(3)?,
                content_hash: r.get(4)?,
                active: r.get(5)?,
                created_at: r.get(6)?,
            })
        })
        .map_err(|_| "版本列表不可用")?;
    versions
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "版本记录无效".into())
}
