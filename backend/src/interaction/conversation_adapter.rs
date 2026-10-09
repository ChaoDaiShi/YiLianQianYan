//! Conversation persistence adapter for the v1 Voice/Text interaction path.
//!
//! The adapter owns only target resolution and the final anchor validation
//! immediately before a turn is written.  It does not own a conversation
//! runtime and it never infers a replacement conversation after an anchor is
//! stale.

use thiserror::Error;

use crate::db::{Database, MessageRow};
use crate::shared::interaction::{
    ConversationalAnchor, InteractionTarget, TargetResolution, MAX_INTERACTION_IDENTIFIER_CHARS,
};

const CONVERSATION_TARGET_PREFIX: &str = "conversation://";

#[derive(Debug, Error)]
pub enum ConversationVoiceAdapterError {
    #[error("conversation target is invalid: {0}")]
    InvalidTarget(String),
    #[error("conversation anchor is missing: {0}")]
    Missing(String),
    #[error("conversation anchor is ambiguous: {0}")]
    Ambiguous(String),
    #[error("conversation message targets {message_conversation_id}, not anchor {anchor_conversation_id}")]
    MessageTargetMismatch {
        anchor_conversation_id: String,
        message_conversation_id: String,
    },
    #[error("conversation persistence failed: {0}")]
    Persistence(String),
}

/// Resolves stable conversation anchors against the existing SQLite store.
#[derive(Clone)]
pub struct ConversationVoiceAdapter {
    database: Database,
}

impl ConversationVoiceAdapter {
    pub fn new(database: &Database) -> Self {
        Self {
            database: database.clone(),
        }
    }

    pub fn database(&self) -> Database {
        self.database.clone()
    }

    /// Resolve an explicit `conversation://<id>` target or one normalized
    /// title.  No title is treated as an implicit current conversation: the
    /// caller must provide a shared anchor for continuation turns.
    pub fn resolve_anchor(
        &self,
        explicit_target: Option<&str>,
        title: Option<&str>,
    ) -> TargetResolution {
        if let Some(target) = explicit_target {
            return self.resolve_explicit_target(target);
        }

        let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) else {
            return TargetResolution::Missing {
                reason: "conversation title or explicit target is missing".to_string(),
            };
        };
        let wanted = normalize(title);
        if wanted.is_empty() {
            return TargetResolution::Missing {
                reason: "conversation title is empty after normalization".to_string(),
            };
        }

        let conversations = match self.database.list_conversations() {
            Ok(conversations) => conversations,
            Err(error) => {
                return TargetResolution::Missing {
                    reason: format!("conversation store unavailable: {error}"),
                }
            }
        };
        let candidates = conversations
            .into_iter()
            .filter(|conversation| normalize(&conversation.title) == wanted)
            .map(|conversation| InteractionTarget::Conversation {
                conversation_id: conversation.id,
            })
            .collect::<Vec<_>>();

        match candidates.as_slice() {
            [target] => TargetResolution::Resolved {
                target: target.clone(),
            },
            [] => TargetResolution::Missing {
                reason: "no conversation matches the normalized title".to_string(),
            },
            _ => TargetResolution::Ambiguous {
                candidates: candidates.into_iter().take(16).collect(),
            },
        }
    }

    /// Validate the persisted identity of an anchor.  The title is retained
    /// for narration/UI context but is deliberately not used as an identity
    /// key, so a renamed conversation remains the same conversation.
    pub fn validate_anchor(&self, anchor: &ConversationalAnchor) -> TargetResolution {
        if !valid_identifier(&anchor.conversation_id) {
            return TargetResolution::Missing {
                reason: "conversation anchor id is invalid".to_string(),
            };
        }
        match self.database.get_conversation(&anchor.conversation_id) {
            Ok(_) => TargetResolution::Resolved {
                target: InteractionTarget::Conversation {
                    conversation_id: anchor.conversation_id.clone(),
                },
            },
            Err(_) => TargetResolution::Missing {
                reason: "conversation anchor no longer exists".to_string(),
            },
        }
    }

    /// Revalidate an anchor and write exactly one message to that anchored
    /// conversation. The final existence check is repeated inside the same
    /// SQLite transaction as the insert, so a concurrent deletion cannot
    /// create an orphan message between validation and commit.
    pub fn commit_turn(
        &self,
        anchor: &ConversationalAnchor,
        message: &MessageRow,
    ) -> Result<(), ConversationVoiceAdapterError> {
        if message.conversation_id != anchor.conversation_id {
            return Err(ConversationVoiceAdapterError::MessageTargetMismatch {
                anchor_conversation_id: anchor.conversation_id.clone(),
                message_conversation_id: message.conversation_id.clone(),
            });
        }
        match self.validate_anchor(anchor) {
            TargetResolution::Resolved { .. } => self
                .database
                .add_message_if_conversation_exists(message)
                .map_err(ConversationVoiceAdapterError::Persistence),
            TargetResolution::Missing { reason } => {
                Err(ConversationVoiceAdapterError::Missing(reason))
            }
            TargetResolution::Ambiguous { candidates } => {
                Err(ConversationVoiceAdapterError::Ambiguous(format!(
                    "{} candidates remain for anchored conversation",
                    candidates.len()
                )))
            }
        }
    }

    fn resolve_explicit_target(&self, target: &str) -> TargetResolution {
        let Some(conversation_id) = target.strip_prefix(CONVERSATION_TARGET_PREFIX) else {
            return TargetResolution::Missing {
                reason: "explicit conversation target must use conversation://<id>".to_string(),
            };
        };
        if !valid_identifier(conversation_id) {
            return TargetResolution::Missing {
                reason: "explicit conversation target id is invalid".to_string(),
            };
        }
        match self.database.get_conversation(conversation_id) {
            Ok(_) => TargetResolution::Resolved {
                target: InteractionTarget::Conversation {
                    conversation_id: conversation_id.to_string(),
                },
            },
            Err(_) => TargetResolution::Missing {
                reason: "explicit conversation target does not exist".to_string(),
            },
        }
    }
}

fn valid_identifier(value: &str) -> bool {
    let length = value.chars().count();
    !value.trim().is_empty() && length <= MAX_INTERACTION_IDENTIFIER_CHARS
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_whitespace()
                && !matches!(
                    character,
                    '。' | '！' | '？' | '?' | '!' | ',' | '，' | '、' | ':' | '：' | ';' | '；'
                )
        })
        .flat_map(char::to_lowercase)
        .collect()
}
