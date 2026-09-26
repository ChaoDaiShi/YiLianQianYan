//! Sensitivity screening for a lesson before it is stored or installed.
//!
//! This delegates to the existing agent-memory write policy rather than
//! introducing a second classifier: a lesson that would be refused as a
//! long-term memory write is refused as a skill candidate too, so the two
//! paths cannot drift apart.

pub(crate) fn screen_lesson(lesson: &str) -> Result<(), String> {
    use crate::agent::memory::{
        validate_candidate, MemoryCandidate, MemoryCategory, MemoryWritePolicy,
    };
    let candidate = MemoryCandidate::new(
        MemoryCategory::Preference,
        lesson,
        None,
        "user-reviewed-candidate",
        1.0,
    );
    validate_candidate(&candidate, &MemoryWritePolicy::default(), &[])
        .map_err(|_| "经验内容为空、过长或包含敏感信息".into())
}
