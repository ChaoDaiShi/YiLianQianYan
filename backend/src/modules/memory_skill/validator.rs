//! Candidate revision checks and managed-skill name rules.
//!
//! Revision checking is what makes the review state machine safe for more than
//! one reviewer: an action taken against a stale revision is refused rather
//! than silently applied on top of someone else's edit. Name rules keep a
//! skill name usable as a directory name.

use crate::db::SkillCandidate;

pub(crate) fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 80
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
    {
        Err("Skill 名称须为 1-80 个小写字母、数字、横线或下划线".into())
    } else {
        Ok(())
    }
}

pub(crate) fn check_revision(candidate: &SkillCandidate, revision: u64) -> Result<(), String> {
    if candidate.revision != revision {
        return Err("候选已更新，请刷新后重试".into());
    }
    if candidate.status == "confirmed" || candidate.status == "rejected" {
        return Err("此候选已结束审核，请创建新候选".into());
    }
    Ok(())
}
