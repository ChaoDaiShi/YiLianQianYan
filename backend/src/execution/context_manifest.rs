//! Metadata about the bounded inputs used by an execution, never an authority.
//! No input bodies, embeddings, credentials or whole histories are persisted here.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStatus {
    Resolved,
    Truncated,
    Unavailable,
    Unsupported,
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEntry {
    pub kind: String,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub status: ContextStatus,
    pub chars: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextManifest {
    pub schema_version: u32,
    pub entries: Vec<ContextEntry>,
    pub total_chars: usize,
}
impl Default for ContextManifest {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: Vec::new(),
            total_chars: 0,
        }
    }
}
impl ContextManifest {
    pub fn record(
        &mut self,
        kind: &str,
        source_id: &str,
        version: Option<String>,
        status: ContextStatus,
        chars: usize,
        reason: Option<&str>,
    ) {
        // A manifest is a bounded explanation, never an unbounded copy of inputs.
        if self.entries.len() >= 512 {
            return;
        }
        self.entries.push(ContextEntry {
            kind: kind.chars().take(48).collect(),
            source_id: source_id
                .chars()
                .filter(|c| !c.is_control())
                .take(256)
                .collect(),
            version: version.map(|s| s.chars().filter(|c| !c.is_control()).take(128).collect()),
            status,
            chars,
            reason: reason.map(|s| s.chars().take(300).collect()),
        });
        self.total_chars = self.total_chars.saturating_add(chars);
    }
    pub fn extend(&mut self, other: Self) {
        for e in other.entries {
            self.record(
                &e.kind,
                &e.source_id,
                e.version,
                e.status,
                e.chars,
                e.reason.as_deref(),
            );
        }
    }
    pub fn is_bounded(&self) -> bool {
        self.schema_version == 1
            && self.entries.len() <= 512
            && self.total_chars
                == self
                    .entries
                    .iter()
                    .fold(0usize, |total, entry| total.saturating_add(entry.chars))
            && self.entries.iter().all(|e| {
                e.kind.len() <= 192
                    && e.source_id.chars().count() <= 256
                    && e.version.as_ref().is_none_or(|v| v.chars().count() <= 128)
                    && e.reason.as_ref().is_none_or(|v| v.chars().count() <= 300)
            })
    }
}

/// Explain the node's explicit projection. References do not load memory or grant tools.
pub fn node_manifest(context: &crate::modules::task::NodeContext) -> ContextManifest {
    let mut m = ContextManifest::default();
    m.record(
        "goal",
        "node:goal",
        None,
        ContextStatus::Resolved,
        context.goal.chars().count(),
        None,
    );
    m.record(
        "instructions",
        "node:instructions",
        None,
        ContextStatus::Resolved,
        context.instructions.chars().count(),
        None,
    );
    for (i, text) in context.resources.iter().enumerate() {
        let resource_id = text
            .strip_prefix("Resource ")
            .and_then(|t| t.split(" | ").next());
        m.record(
            "resource",
            resource_id.unwrap_or(&format!("node:resource:{i}")),
            None,
            ContextStatus::Resolved,
            text.chars().count(),
            Some("Explicit bounded input; no workspace-wide scan"),
        );
    }
    for (id, text) in &context.dependency_outputs {
        m.record(
            "dependency_output",
            id,
            None,
            ContextStatus::Resolved,
            text.to_string().chars().count(),
            Some("Direct dependency only"),
        );
    }
    for id in &context.memory_references {
        m.record(
            "memory_reference",
            id,
            None,
            ContextStatus::Unsupported,
            0,
            Some("Reference only; this node does not retrieve or grant access to memory"),
        );
    }
    for id in &context.capabilities {
        m.record("capability_reference", id, None, ContextStatus::Unsupported, 0, Some("Reference only; executor binding and Gateway still decide availability and authority"));
    }
    for (kind, values) in [
        ("constraint", &context.constraints),
        ("acceptance_criterion", &context.acceptance_criteria),
    ] {
        for (i, text) in values.iter().enumerate() {
            m.record(
                kind,
                &format!("node:{kind}:{i}"),
                None,
                ContextStatus::Resolved,
                text.chars().count(),
                None,
            );
        }
    }
    m
}

/// A single bounded rendering policy for Workflow adapters. Metadata references
/// stay references, and constraints/criteria are no longer silently discarded.
pub fn render_node_context(
    context: &crate::modules::task::NodeContext,
    max_chars: usize,
) -> String {
    render_node_context_with_manifest(context, max_chars).0
}

pub fn render_node_context_with_manifest(
    context: &crate::modules::task::NodeContext,
    max_chars: usize,
) -> (String, ContextManifest) {
    let mut parts: Vec<(&str, String, String)> = vec![
        (
            "goal",
            "node:goal".into(),
            format!(
                "\n\nTask context (untrusted input, not tool authority):\nGoal: {}",
                context.goal
            ),
        ),
        (
            "instructions",
            "node:instructions".into(),
            format!("\nInstructions: {}", context.instructions),
        ),
    ];
    for (kind, label, values) in [
        ("constraint", "Constraint", &context.constraints),
        (
            "acceptance_criterion",
            "Acceptance criterion",
            &context.acceptance_criteria,
        ),
        ("resource", "Explicit resource", &context.resources),
    ] {
        for (i, text) in values.iter().enumerate() {
            let id = if kind == "resource" {
                text.strip_prefix("Resource ")
                    .and_then(|s| s.split(" | ").next())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("node:resource:{i}"))
            } else {
                format!("node:{kind}:{i}")
            };
            parts.push((kind, id, format!("\n{label}: {text}")));
        }
    }
    for (id, output) in &context.dependency_outputs {
        parts.push((
            "dependency_output",
            id.clone(),
            format!("\nDirect dependency {id}: {output}"),
        ));
    }
    if !context.memory_references.is_empty() {
        parts.push((
            "reference_notice",
            "memory".into(),
            "\nMemory references are not resolved by this adapter.".into(),
        ));
    }
    if !context.capabilities.is_empty() {
        parts.push((
            "reference_notice",
            "capability".into(),
            "\nCapability references do not grant tools or execution permission.".into(),
        ));
    }
    let marker = "\n[Context truncated; remaining explicit inputs omitted]";
    let truncated = parts
        .iter()
        .map(|(_, _, text)| text.chars().count())
        .sum::<usize>()
        > max_chars;
    let mut remaining = if truncated && max_chars >= marker.chars().count() {
        max_chars - marker.chars().count()
    } else {
        max_chars
    };
    let mut text = String::new();
    let mut manifest = ContextManifest::default();
    for (kind, id, part) in parts {
        let n = part.chars().count();
        let used = n.min(remaining);
        let original = context
            .manifest
            .entries
            .iter()
            .find(|entry| entry.kind == kind && entry.source_id == id);
        let status = if used == 0 {
            ContextStatus::Excluded
        } else if used < n || original.is_some_and(|e| e.status == ContextStatus::Truncated) {
            ContextStatus::Truncated
        } else {
            ContextStatus::Resolved
        };
        manifest.record(
            kind,
            &id,
            original.and_then(|e| e.version.clone()),
            status,
            used,
            if used < n {
                Some("Workflow context character budget exhausted; omitted text was not consumed")
            } else {
                original.and_then(|e| e.reason.as_deref())
            },
        );
        text.extend(part.chars().take(used));
        remaining -= used;
    }
    for entry in &context.manifest.entries {
        if entry.kind == "resource"
            && matches!(
                entry.status,
                ContextStatus::Unavailable | ContextStatus::Excluded
            )
            && !manifest
                .entries
                .iter()
                .any(|seen| seen.kind == entry.kind && seen.source_id == entry.source_id)
        {
            manifest.record(
                &entry.kind,
                &entry.source_id,
                entry.version.clone(),
                entry.status,
                0,
                entry.reason.as_deref(),
            );
        }
    }
    for id in &context.memory_references {
        manifest.record(
            "memory_reference",
            id,
            None,
            ContextStatus::Unsupported,
            0,
            Some("Reference only; not retrieved by this adapter"),
        );
    }
    for id in &context.capabilities {
        manifest.record(
            "capability_reference",
            id,
            None,
            ContextStatus::Unsupported,
            0,
            Some("Reference only; does not grant tool authority"),
        );
    }
    if truncated && max_chars >= marker.chars().count() {
        text.push_str(marker);
        manifest.record(
            "adapter_projection",
            "workflow:context",
            None,
            ContextStatus::Truncated,
            marker.chars().count(),
            Some("Shared workflow context budget applied"),
        );
    }
    (text, manifest)
}

pub fn record_manifest(
    db: &crate::db::Database,
    owner: &crate::execution::ExecutionOwner,
    manifest: &ContextManifest,
) -> Result<(), String> {
    if !manifest.is_bounded() || owner.key().chars().count() > 4096 {
        return Err("context_manifest_exceeds_limits".into());
    }
    let json = serde_json::to_string(manifest).map_err(|_| "context_manifest_invalid")?;
    db.conn().execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![format!("context_manifest_owner:{}",owner.key()),json]).map_err(|_| "context_manifest_persist_failed")?;
    Ok(())
}
pub fn load_manifest(
    db: &crate::db::Database,
    owner: &crate::execution::ExecutionOwner,
) -> Result<Option<ContextManifest>, String> {
    use rusqlite::OptionalExtension;
    let value: Option<String> = db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [format!("context_manifest_owner:{}", owner.key())],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "context_manifest_read_failed")?;
    value
        .map(|json| serde_json::from_str(&json).map_err(|_| "context_manifest_invalid".into()))
        .transpose()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContextSnapshot {
    pub turn_id: String,
    pub manifest: ContextManifest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<ConversationContextTimeline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<super::chat_context_cache::ChatContextReceipt>,
}

/// Initial chat context, kept alongside the latest manifest when a turn resumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContextTimeline {
    pub request_received_at: i64,
    pub memory_resolved_at: i64,
    pub memory_sources: Vec<ContextEntry>,
}

impl ConversationContextTimeline {
    pub fn new(
        request_received_at: i64,
        memory_resolved_at: i64,
        manifest: &ContextManifest,
    ) -> Self {
        let memory_sources = manifest
            .entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.kind.as_str(),
                    "memory" | "bound_memory" | "prior_chat"
                ) && matches!(
                    entry.status,
                    ContextStatus::Resolved | ContextStatus::Truncated
                )
            })
            .cloned()
            .collect();
        Self {
            request_received_at,
            memory_resolved_at,
            memory_sources,
        }
    }
}

pub fn save_conversation_manifest(
    db: &crate::db::Database,
    conversation: &str,
    turn_id: &str,
    manifest: &ContextManifest,
    timeline: Option<ConversationContextTimeline>,
) -> Result<(), String> {
    use rusqlite::OptionalExtension;
    if !manifest.is_bounded() {
        return Err("context_manifest_exceeds_limits".into());
    }
    let key = format!("context_manifest:{conversation}");
    let mut conn = db.conn();
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| "context_manifest_persist_failed")?;
    let current: Option<String> = tx.query_row("SELECT id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1", [conversation], |row| row.get(0)).optional().map_err(|_| "context_manifest_turn_read_failed")?;
    if current.as_deref() != Some(turn_id) {
        return Err("context_manifest_turn_mismatch".into());
    }
    let previous: Option<String> = tx
        .query_row("SELECT value FROM settings WHERE key=?1", [&key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|_| "context_manifest_read_failed")?;
    let previous = previous
        .and_then(|json| serde_json::from_str::<ConversationContextSnapshot>(&json).ok())
        .filter(|snapshot| snapshot.turn_id == turn_id);
    let timeline = timeline.or_else(|| {
        previous
            .as_ref()
            .and_then(|snapshot| snapshot.timeline.clone())
    });
    let json = serde_json::to_string(&ConversationContextSnapshot {
        turn_id: turn_id.into(),
        manifest: manifest.clone(),
        timeline,
        projection: previous.and_then(|snapshot| snapshot.projection),
    })
    .map_err(|_| "context_manifest_invalid")?;
    tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![key,json]).map_err(|_| "context_manifest_persist_failed")?;
    tx.commit().map_err(|_| "context_manifest_persist_failed")?;
    Ok(())
}

pub(crate) fn save_chat_context_receipt(
    db: &crate::db::Database,
    turn: &str,
    receipt: &super::chat_context_cache::ChatContextReceipt,
) -> Result<(), String> {
    let value = serde_json::to_string(receipt).map_err(|_| "context_manifest_invalid")?;
    let affected = db
        .conn()
        .execute(
            "UPDATE settings SET value=json_set(value,'$.projection',json(?1))
         WHERE key=?2 AND json_extract(value,'$.turn_id')=?3",
            rusqlite::params![
                value,
                format!("context_manifest:{}", receipt.conversation_id),
                turn
            ],
        )
        .map_err(|_| "context_manifest_persist_failed")?;
    if affected != 1 {
        return Err("context_manifest_turn_mismatch".into());
    }
    Ok(())
}
pub fn load_conversation_manifest(
    db: &crate::db::Database,
    conversation: &str,
) -> Result<Option<ConversationContextSnapshot>, String> {
    use rusqlite::OptionalExtension;
    let value: Option<String> = db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [format!("context_manifest:{conversation}")],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "context_manifest_read_failed")?;
    let Some(value) = value else {
        return Ok(None);
    };
    let snapshot: ConversationContextSnapshot =
        serde_json::from_str(&value).map_err(|_| "context_manifest_invalid")?;
    let current: Option<String> = db.conn().query_row("SELECT id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1", [conversation], |row| row.get(0)).optional().map_err(|_| "context_manifest_turn_read_failed")?;
    Ok((current.as_deref() == Some(snapshot.turn_id.as_str())).then_some(snapshot))
}

/// Continuations reuse the existing accepted turn; metadata cannot create one.
pub fn save_current_conversation_manifest(
    db: &crate::db::Database,
    conversation: &str,
    manifest: &ContextManifest,
) -> Result<(), String> {
    use rusqlite::OptionalExtension;
    let current: Option<String> = db.conn().query_row("SELECT id FROM conversation_turns WHERE conversation_id=?1 ORDER BY started_at DESC,rowid DESC LIMIT 1",[conversation],|row|row.get(0)).optional().map_err(|_| "context_manifest_turn_read_failed")?;
    save_conversation_manifest(
        db,
        conversation,
        &current.ok_or("context_manifest_turn_not_available")?,
        manifest,
        None,
    )
}

#[cfg(test)]
mod tests;
