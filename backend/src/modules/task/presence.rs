use crate::shared::context::TaskProjection;
use crate::shared::voice::{PresenceActivity, PresenceAttention, PresenceSnapshot};

/// Pure overlay from Task World facts onto the Voice-owned presence snapshot.
///
/// The adapter only elevates activity/attention.  Voice interaction remains
/// untouched so listening, speaking, and interrupted states stay owned by the
/// frozen voice contract.
pub struct TaskPresenceAdapter;

impl TaskPresenceAdapter {
    pub fn merge(voice: &PresenceSnapshot, projections: &[TaskProjection]) -> PresenceSnapshot {
        let mut combined = voice.clone();
        let task_is_working = projections
            .iter()
            .any(|projection| projection.status == "working");
        let task_needs_attention = projections
            .iter()
            .any(|projection| projection.attention_required);

        if task_is_working {
            combined.activity = PresenceActivity::Working;
        }
        if task_needs_attention {
            combined.attention = PresenceAttention::Requested;
        }
        if task_is_working || task_needs_attention {
            combined.source = "task-world-presence".to_string();
        }
        if let Some(task_updated_at) = projections.iter().map(|p| p.updated_at).max() {
            combined.updated_at = combined.updated_at.max(task_updated_at);
        }
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::context::TaskProjection;
    use crate::shared::contracts::SimulationMetadata;
    use crate::shared::voice::{
        PresenceActivity, PresenceAttention, PresenceInteraction, PresenceSnapshot,
    };

    fn projection(status: &str, attention_required: bool, updated_at: i64) -> TaskProjection {
        TaskProjection {
            id: "graph-1".to_string(),
            title: "Task graph graph-1".to_string(),
            status: status.to_string(),
            progress: None,
            current_activity: None,
            attention_required,
            updated_at,
            simulation: SimulationMetadata {
                simulated: false,
                provider: "task-supervisor-state".to_string(),
                reason: "structured task state projection only; no workflow or tool execution"
                    .to_string(),
            },
            schema_version: 1,
        }
    }

    #[test]
    fn task_presence_overlays_working_and_attention_without_clobbering_voice() {
        let voice = PresenceSnapshot {
            activity: PresenceActivity::Idle,
            interaction: PresenceInteraction::Speaking,
            attention: PresenceAttention::None,
            source: "voice-core".to_string(),
            updated_at: 10,
        };
        let projections = vec![projection("working", true, 20)];

        let combined = TaskPresenceAdapter::merge(&voice, &projections);

        assert_eq!(combined.activity, PresenceActivity::Working);
        assert_eq!(combined.attention, PresenceAttention::Requested);
        assert_eq!(combined.interaction, PresenceInteraction::Speaking);
        assert_eq!(combined.updated_at, 20);
        assert_eq!(combined.source, "task-world-presence");

        let unchanged = TaskPresenceAdapter::merge(&voice, &[]);
        assert_eq!(unchanged, voice);
    }
}
