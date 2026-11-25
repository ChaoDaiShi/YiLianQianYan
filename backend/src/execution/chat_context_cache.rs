//! Scalar cache for chat context assembly. Source bodies stay in the turn.
use std::{collections::VecDeque, sync::Arc};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    app::state::AppServer,
    db::Database,
    execution::context_manifest::{self, ContextManifest},
    integrations::llm::types::ChatMessage,
};

const CAPACITY: usize = 64;

#[derive(Default)]
pub(crate) struct ChatContextCache {
    estimates: Mutex<VecDeque<([u8; 32], usize)>>,
}

impl ChatContextCache {
    fn estimate(
        &self,
        key: [u8; 32],
        compute: impl FnOnce() -> Result<usize, String>,
    ) -> Result<(usize, bool), String> {
        let mut estimates = self.estimates.lock();
        if let Some(index) = estimates.iter().position(|(stored, _)| stored == &key) {
            let hit = estimates.remove(index).expect("index was just found");
            estimates.push_front(hit);
            return Ok((hit.1, true));
        }
        let estimate = compute()?;
        estimates.push_front((key, estimate));
        estimates.truncate(CAPACITY);
        Ok((estimate, false))
    }
}

/// Hash the serialized value without constructing a second copy of the context.
fn digest<T: Serialize + ?Sized>(value: &T) -> Result<[u8; 32], String> {
    let mut hasher = Sha256::new();
    serde_json::to_writer(&mut hasher, value).map_err(|error| error.to_string())?;
    Ok(hasher.finalize().into())
}

fn hex(bytes: [u8; 32]) -> String {
    format!("{:x}", sha2::digest::Output::<Sha256>::from(bytes))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case")]
pub enum ContextBudget {
    EstimatedTokens {
        context_window: usize,
        output_reserve: u32,
        estimated_input: usize,
    },
    Characters {
        text_budget: usize,
        payload_chars: usize,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatContextReceipt {
    pub conversation_id: String,
    pub branch_id: Option<String>,
    pub model_id: String,
    /// The accepted user message anchors the context at its persisted position.
    pub event_cursor: String,
    pub source_revision: String,
    pub summary_revision: Option<String>,
    pub tool_schema_version: String,
    pub content_digest: String,
    pub token_budget: ContextBudget,
    pub static_budget_cache_hit: bool,
}

pub(crate) struct ChatContextObservation {
    db: Database,
    cache: Arc<ChatContextCache>,
    conversation_id: String,
    branch_id: Option<String>,
    turn_id: String,
    event_cursor: String,
    model_id: String,
    source_revision: [u8; 32],
}

pub(crate) struct StaticBudget {
    pub tokens: usize,
    tool_schema: [u8; 32],
    cache_hit: bool,
}

tokio::task_local! {
    pub(crate) static CURRENT_CHAT_CONTEXT: Option<ChatContextObservation>;
}

impl ChatContextObservation {
    pub(crate) fn new(
        server: &AppServer,
        conversation: &str,
        turn: &str,
        user_message: &str,
        model_id: String,
        config: &crate::config::types::AppConfig,
        manifest: &ContextManifest,
    ) -> Result<Self, String> {
        let lease = server.scoped_db.lease();
        let workspace = server.db.conversation_workspace(conversation)?;
        let branch = is_branch(&server.db, conversation)?.then(|| conversation.to_owned());
        let authority = current_authority(&server.db)?;
        let settings_revision = server.db.get_settings_revision()?;
        let model_pool = model_id
            .strip_prefix("pool:")
            .map(|pool| pool_revision(&server.db, pool))
            .transpose()?;
        let source_revision = digest(&(
            lease.scope_id,
            lease.generation,
            settings_revision,
            conversation,
            &branch,
            workspace,
            authority,
            &model_id,
            &config.model,
            &config.compaction,
            model_pool,
            manifest,
        ))?;
        Ok(Self {
            db: server.db.clone_connection(),
            cache: server.chat_context_cache.clone(),
            conversation_id: conversation.into(),
            branch_id: branch,
            turn_id: turn.into(),
            event_cursor: user_message.into(),
            model_id,
            source_revision,
        })
    }

    pub(crate) fn owns_conversation(&self, conversation_id: &str) -> bool {
        self.conversation_id == conversation_id
    }

    pub(crate) fn static_budget(
        &self,
        system: &ChatMessage,
        tools: &[serde_json::Value],
    ) -> Result<StaticBudget, String> {
        let tool_schema = digest(tools)?;
        let key = digest(&(self.source_revision, system, tool_schema))?;
        let (tokens, cache_hit) = self.cache.estimate(key, || {
            crate::agent::context_compaction::static_input_tokens(system, tools)
        })?;
        Ok(StaticBudget {
            tokens,
            tool_schema,
            cache_hit,
        })
    }

    pub(crate) fn record_api(
        &self,
        messages: &[ChatMessage],
        budget: &StaticBudget,
        window: usize,
        reserve: u32,
        summary_revision: Option<String>,
    ) -> Result<(), String> {
        self.record(
            digest(&(messages, budget.tool_schema))?,
            budget.tool_schema,
            summary_revision,
            ContextBudget::EstimatedTokens {
                context_window: window,
                output_reserve: reserve,
                estimated_input: crate::agent::context_compaction::estimated_input_tokens(
                    messages,
                    budget.tokens,
                ),
            },
            budget.cache_hit,
        )
    }

    pub(crate) fn record_cli(
        &self,
        content: [u8; 32],
        payload_chars: usize,
        text_budget: usize,
        summary_revision: Option<String>,
    ) -> Result<(), String> {
        self.record(
            content,
            digest(&[] as &[serde_json::Value])?,
            summary_revision,
            ContextBudget::Characters {
                text_budget,
                payload_chars,
            },
            false,
        )
    }

    fn record(
        &self,
        content: [u8; 32],
        schema: [u8; 32],
        summary_revision: Option<String>,
        token_budget: ContextBudget,
        static_budget_cache_hit: bool,
    ) -> Result<(), String> {
        let receipt = ChatContextReceipt {
            conversation_id: self.conversation_id.clone(),
            branch_id: self.branch_id.clone(),
            model_id: self.model_id.clone(),
            event_cursor: self.event_cursor.clone(),
            source_revision: hex(self.source_revision),
            summary_revision,
            tool_schema_version: hex(schema),
            content_digest: hex(content),
            token_budget,
            static_budget_cache_hit,
        };
        context_manifest::save_chat_context_receipt(&self.db, &self.turn_id, &receipt)
    }
}

fn is_branch(db: &Database, conversation: &str) -> Result<bool, String> {
    db.conn()
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM conversation_lanes WHERE conversation_id=?1
         UNION ALL SELECT 1 FROM message_canvas_branches WHERE conversation_id=?1
         UNION ALL SELECT 1 FROM connector_message_runs WHERE conversation_id=?1
            AND conversation_id<>root_conversation_id)",
            [conversation],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn pool_revision(db: &Database, pool_id: &str) -> Result<[u8; 32], String> {
    let pool = db
        .get_model_pool(pool_id)?
        .ok_or("context_model_pool_missing")?;
    let revisions = pool
        .tiers
        .iter()
        .flat_map(|tier| &tier.profile_ids)
        .map(|id| {
            db.get_llm_model(id)
                .map(|model| model.map(|model| model.updated_at))
        })
        .collect::<Result<Vec<_>, _>>()?;
    digest(&(pool, revisions))
}

fn current_authority(db: &Database) -> Result<[u8; 32], String> {
    let conn = db.conn();
    let profile_revision: i64 = conn
        .query_row(
            "SELECT revision FROM local_onboarding WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let mut grants = conn
        .prepare(
            "SELECT json_array(g.grant_id,g.subject_id,g.permission_id,g.resource_json,g.effect,
                               g.source,g.expires_at,s.scope,s.subject_id)
             FROM security_grants g LEFT JOIN security_access_rule_scopes s ON s.grant_id=g.grant_id
             WHERE g.expires_at IS NULL OR g.expires_at>?1 ORDER BY g.grant_id",
        )
        .map_err(|error| error.to_string())?;
    let mut rows = grants
        .query([chrono::Utc::now().timestamp_millis()])
        .map_err(|error| error.to_string())?;
    let mut fingerprint = Sha256::new();
    fingerprint.update(profile_revision.to_le_bytes());
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let value = row.get_ref(0).map_err(|error| error.to_string())?;
        let text = value.as_bytes().map_err(|error| error.to_string())?;
        fingerprint.update((text.len() as u64).to_le_bytes());
        fingerprint.update(text);
    }
    Ok(fingerprint.finalize().into())
}
