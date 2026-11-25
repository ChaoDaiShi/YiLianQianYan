// ============================================================
// Memory context builder — query generation + retrieval + bounded
// context injection for the Agent Runtime.
//
// The builder is deterministic and safety-preserving: it derives a retrieval
// query from the task title / description / step instruction, embeds it when an
// embedding model is configured (falling back to lexical retrieval otherwise),
// retrieves and ranks memories via the existing hybrid retriever, and emits a
// char-safe-truncated context string. Raw embedding vectors are never surfaced.
// ============================================================

use serde::Serialize;

use crate::config::types::ModelConfig;
use crate::db::{Database, MemoryRecallScope, RetrieveQuery, ScoredMemory};
use crate::execution::context_manifest::{ContextManifest, ContextStatus};
use crate::integrations::llm::client::LlmClient;
use crate::integrations::llm::types::ChatMessage;
use crate::integrations::secret::SecretResolver;
use crate::utils::text::truncate_chars;
use std::sync::Arc;

/// Maximum characters of a generated memory retrieval query.
pub const MAX_MEMORY_QUERY_CHARS: usize = 500;

/// Maximum characters of the injected memory context block.
pub const MAX_MEMORY_CONTEXT_CHARS: usize = 4000;

/// Number of top-ranked memories to retrieve and inject.
pub const DEFAULT_TOP_K: usize = 5;

/// Which retrieval mode produced the results (for observability / tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRetrievalMode {
    Lexical,
    Hybrid,
    LexicalFallback,
}

/// The outcome of a memory retrieval for one task step.
#[derive(Debug, Clone)]
pub struct MemoryContext {
    pub query: String,
    pub mode: MemoryRetrievalMode,
    pub memories: Vec<ScoredMemory>,
    /// Bounded, UI-safe text to inject into the agent's context.
    pub injected_text: String,
    pub manifest: ContextManifest,
}

/// Callers select sources and bounds explicitly. A shared retriever does not
/// make conversation-local, workspace-local or profile memory interchangeable.
pub enum MemoryContextScope<'a> {
    Conversation(&'a str),
    Workspace(&'a str),
    Profile,
}
#[derive(Clone, Copy)]
pub struct MemoryContextPolicy {
    pub top_k: usize,
    pub max_chars: usize,
    pub max_item_chars: usize,
    pub allow_embedding: bool,
    pub restrict_memory: bool,
    pub restrict_knowledge: bool,
    pub include_references: bool,
}
impl Default for MemoryContextPolicy {
    fn default() -> Self {
        Self {
            top_k: DEFAULT_TOP_K,
            max_chars: MAX_MEMORY_CONTEXT_CHARS,
            max_item_chars: MAX_MEMORY_CONTEXT_CHARS,
            allow_embedding: true,
            restrict_memory: false,
            restrict_knowledge: false,
            include_references: false,
        }
    }
}

/// Build a deterministic retrieval query from task + step fields.
///
/// No LLM is involved: the query is a char-safe concatenation of the fields.
pub fn build_memory_query(title: &str, description: &str, instruction: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let title = title.trim();
    let description = description.trim();
    let instruction = instruction.trim();
    if !title.is_empty() {
        parts.push(title);
    }
    if !description.is_empty() {
        parts.push(description);
    }
    if !instruction.is_empty() {
        parts.push(instruction);
    }
    let joined = parts.join(" ");
    truncate_chars(&joined, MAX_MEMORY_QUERY_CHARS)
}

enum ChatQueryKind {
    Topic,
    RecallPrevious,
    NoTopic,
}

fn chat_query_kind(message: &str) -> ChatQueryKind {
    match message.trim().trim_end_matches(|character: char| {
        character.is_whitespace() || matches!(character, '？' | '?' | '。' | '！' | '!')
    }) {
        "还记得吗"
        | "你还记得吗"
        | "记得吗"
        | "你记得吗"
        | "还记得么"
        | "你还记得么"
        | "之前那个呢"
        | "上次那个呢"
        | "撤回刚才那条"
        | "忘掉刚才那条"
        | "前者"
        | "后者"
        | "第一条"
        | "第二条"
        | "继续"
        | "接着来"
        | "按刚才的来"
        | "还是按之前的来" => ChatQueryKind::RecallPrevious,
        "" | "你好" | "您好" | "谢谢" | "谢谢你" | "好的" | "好" | "收到" | "可以" | "嗯" => {
            ChatQueryKind::NoTopic
        }
        _ => ChatQueryKind::Topic,
    }
}

pub(crate) fn chat_memory_query<'a>(message: &'a str, history: &'a [ChatMessage]) -> &'a str {
    match chat_query_kind(message) {
        ChatQueryKind::Topic => message,
        ChatQueryKind::NoTopic => "",
        ChatQueryKind::RecallPrevious => history
            .iter()
            .rev()
            .filter(|message| message.role == "user")
            .filter_map(|message| message.content.as_deref())
            .find(|text| matches!(chat_query_kind(text), ChatQueryKind::Topic))
            .unwrap_or(""),
    }
}

/// Render ranked memories into a bounded, human-readable context block.
///
/// Empty input (or no relevant memories) yields an empty string.
pub fn build_context_text(memories: &[ScoredMemory]) -> String {
    if memories.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = memories
        .iter()
        .map(|scored| {
            let category = scored.memory.category.as_str();
            let content = scored.memory.content.trim();
            if category.is_empty() {
                format!("- {content}")
            } else {
                format!("- [{category}] {content}")
            }
        })
        .collect();
    truncate_chars(&lines.join("\n"), MAX_MEMORY_CONTEXT_CHARS)
}

/// Builds and injects memory context for an agent execution.
pub struct MemoryContextBuilder {
    db: Database,
    llm: LlmClient,
    has_embedding: bool,
    embedding_model: String,
}

impl MemoryContextBuilder {
    pub fn new(db: Database, config: &ModelConfig, resolver: Arc<SecretResolver>) -> Self {
        let has_embedding = config.has_embedding()
            && (config.has_local_embedding()
                || crate::modules::onboarding::allowed(&db, "external_model"));
        Self {
            llm: LlmClient::new(config, resolver).with_profile_permissions(db.clone_connection()),
            db,
            has_embedding,
            embedding_model: crate::db::memory_governance::embedding_identity(config),
        }
    }

    /// Retrieve and build the memory context for a task step.
    ///
    /// Retrieval failures are returned as `Err` so the caller can decide to
    /// degrade gracefully (e.g. continue without memory) rather than failing
    /// the whole task.
    pub async fn build(
        &self,
        title: &str,
        description: &str,
        instruction: &str,
    ) -> Result<MemoryContext, String> {
        self.build_for_workspace(title, description, instruction, None)
            .await
    }
    pub async fn build_for_workspace(
        &self,
        title: &str,
        description: &str,
        instruction: &str,
        workspace_id: Option<&str>,
    ) -> Result<MemoryContext, String> {
        let query = build_memory_query(title, description, instruction);
        let scope = workspace_id
            .map(MemoryContextScope::Workspace)
            .unwrap_or(MemoryContextScope::Profile);
        self.build_with_policy(&query, scope, MemoryContextPolicy::default())
            .await
    }

    /// Common retrieval/governance/filtering path for chat and legacy task steps.
    /// Policies only narrow the caller's existing scope; they never mint grants.
    pub async fn build_with_policy(
        &self,
        query: &str,
        scope: MemoryContextScope<'_>,
        policy: MemoryContextPolicy,
    ) -> Result<MemoryContext, String> {
        let query = truncate_chars(query, MAX_MEMORY_QUERY_CHARS);
        let mut manifest = ContextManifest::default();
        if !crate::modules::onboarding::allowed(&self.db, "memory") {
            manifest.record(
                "memory_retrieval",
                "profile_preference",
                None,
                ContextStatus::Excluded,
                0,
                Some("Memory retrieval disabled by the local profile"),
            );
            return Ok(MemoryContext {
                query,
                mode: MemoryRetrievalMode::Lexical,
                memories: Vec::new(),
                injected_text: String::new(),
                manifest,
            });
        }
        if policy.restrict_memory && policy.restrict_knowledge {
            manifest.record(
                "memory_retrieval",
                "policy",
                None,
                ContextStatus::Excluded,
                0,
                Some("Memory and knowledge are excluded by this execution's bindings"),
            );
            return Ok(MemoryContext {
                query,
                mode: MemoryRetrievalMode::Lexical,
                memories: Vec::new(),
                injected_text: String::new(),
                manifest,
            });
        }

        let mut query_embedding: Option<Vec<f32>> = None;
        let mut mode = MemoryRetrievalMode::Lexical;

        if policy.allow_embedding && self.has_embedding && !query.is_empty() {
            match tokio::time::timeout(
                std::time::Duration::from_secs(2),
                self.llm.embed_query(&query),
            )
            .await
            {
                Ok(Ok(vector)) => {
                    query_embedding = Some(vector);
                    mode = MemoryRetrievalMode::Hybrid;
                }
                _ => {
                    tracing::debug!("memory query embedding unavailable; lexical fallback");
                    mode = MemoryRetrievalMode::LexicalFallback;
                }
            }
        }

        let retrieval = RetrieveQuery {
            q: query.clone(),
            top_k: policy.top_k.clamp(1, 8),
            category: policy.restrict_memory.then(|| "knowledge".into()),
        };
        let mut recall_scope = MemoryRecallScope {
            exclude_documents: true,
            exclude_knowledge: policy.restrict_knowledge,
            ..Default::default()
        };
        match scope {
            MemoryContextScope::Workspace(workspace) => recall_scope.workspace_id = Some(workspace),
            MemoryContextScope::Conversation(conversation) => {
                recall_scope.conversation_id = Some(conversation);
                recall_scope.include_conversation_pins = true;
            }
            MemoryContextScope::Profile => {}
        }
        let memories = self.db.retrieve_memory_candidates(
            &retrieval,
            query_embedding.as_deref(),
            Some(&self.embedding_model),
            recall_scope,
        )?;
        let max_chars = policy.max_chars.min(MAX_MEMORY_CONTEXT_CHARS);
        let mut injected_text = String::new();
        for m in &memories {
            let remaining = max_chars.saturating_sub(injected_text.chars().count());
            if remaining == 0 {
                manifest.record(
                    "memory",
                    &m.memory.id,
                    Some(m.revision.to_string()),
                    ContextStatus::Excluded,
                    0,
                    Some("Context character budget exhausted"),
                );
                continue;
            }
            let content = m.memory.content.trim();
            let prefix = if policy.include_references {
                format!(
                    "- [{} · {}] (memory_id={}; revision={}) ",
                    m.memory.category,
                    m.scope.label(),
                    m.memory.id,
                    m.revision
                )
            } else {
                format!("- [{} · {}] ", m.memory.category, m.scope.label())
            };
            let item_limit = policy
                .max_item_chars
                .min(remaining.saturating_sub(prefix.chars().count() + 1));
            let truncated = content.chars().count() > item_limit;
            let excerpt = if truncated && policy.include_references {
                "[摘录] "
            } else {
                ""
            };
            let content =
                truncate_chars(content, item_limit.saturating_sub(excerpt.chars().count()));
            let bounded: String = if policy.include_references && content.is_empty() {
                String::new()
            } else {
                format!("{prefix}{excerpt}{content}\n")
                    .chars()
                    .take(remaining)
                    .collect()
            };
            manifest.record(
                "memory",
                &m.memory.id,
                Some(m.revision.to_string()),
                if bounded.is_empty() {
                    ContextStatus::Excluded
                } else if truncated {
                    ContextStatus::Truncated
                } else {
                    ContextStatus::Resolved
                },
                bounded.chars().count(),
                truncated.then_some("Bounded memory excerpt; remaining content omitted"),
            );
            injected_text.push_str(&bounded);
        }
        manifest.record(
            "memory_retrieval",
            "scoped_retriever",
            Some(
                match mode {
                    MemoryRetrievalMode::Lexical => "lexical",
                    MemoryRetrievalMode::Hybrid => "hybrid",
                    MemoryRetrievalMode::LexicalFallback => "lexical_fallback",
                }
                .into(),
            ),
            ContextStatus::Resolved,
            0,
            None,
        );
        Ok(MemoryContext {
            query,
            mode,
            memories,
            injected_text,
            manifest,
        })
    }
}

#[cfg(test)]
mod context_policy_tests {
    use super::*;
    use crate::db::{memory_governance::WithdrawMemoryRequest, CreateMemoryRequest};
    use crate::integrations::secret::InMemorySecretStore;

    #[tokio::test]
    async fn shared_context_policy_keeps_bounds_withdrawal_and_delegation_exclusion() {
        let path = std::env::temp_dir().join(format!("context-policy-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(&path).unwrap();
        let memory = db
            .create_memory(&CreateMemoryRequest {
                content: "integration sample note about the active local project".into(),
                category: "note".into(),
                source: "manual".into(),
                source_conversation_id: None,
                metadata: None,
            })
            .unwrap();
        let resolver = Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new())));
        let builder = MemoryContextBuilder::new(db.clone(), &ModelConfig::default(), resolver);
        let policy = MemoryContextPolicy {
            max_chars: 24,
            max_item_chars: 8,
            ..Default::default()
        };
        let result = builder
            .build_with_policy("integration sample", MemoryContextScope::Profile, policy)
            .await
            .unwrap();
        assert_eq!(result.memories.len(), 1);
        assert!(result.injected_text.chars().count() <= 24);
        assert!(result
            .manifest
            .entries
            .iter()
            .any(|entry| entry.source_id == memory.id && entry.status == ContextStatus::Truncated));
        let blocked = builder
            .build_with_policy(
                "integration sample",
                MemoryContextScope::Profile,
                MemoryContextPolicy {
                    restrict_memory: true,
                    restrict_knowledge: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(blocked.memories.is_empty() && blocked.injected_text.is_empty());
        assert_eq!(blocked.manifest.entries[0].status, ContextStatus::Excluded);
        let revision = db.memory_governance(&memory.id).unwrap().revision;
        db.withdraw_memory(
            &memory.id,
            &WithdrawMemoryRequest {
                expected_revision: revision,
                reason: "fixture withdrawal".into(),
            },
        )
        .unwrap();
        let result = builder
            .build_with_policy("integration sample", MemoryContextScope::Profile, policy)
            .await
            .unwrap();
        assert!(result.memories.is_empty());
        drop(builder);
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
