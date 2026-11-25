//! Commit notifications for existing goal records, carried by the shared EventHub.
use super::Database;
use crate::shared::event::{EventHub, YiEvent};
use serde_json::{json, Value};

impl Database {
    pub(crate) fn attach_goal_event_hub(&self, hub: EventHub) {
        *self.goal_events.lock().unwrap() = Some(hub);
    }

    pub(crate) fn publish_goal_change(&self, graph_id: &str, change: &str, fields: Value) {
        // Source transactions release the database lock before this hook.
        // Delivery is durable even when no EventHub consumer is mounted.
        if matches!(
            change,
            "artifact_created" | "conversation_status" | "conversation_bound"
        ) {
            if let Err(error) = self.queue_goal_evaluation(graph_id) {
                tracing::error!(%graph_id,%error,"basic acceptance queue unavailable; startup will rescan");
            }
        }
        let Some(hub) = self.goal_events.lock().unwrap().clone() else {
            return;
        };
        let draft = match self.confirmed_proof_draft_for_graph(graph_id) {
            Ok(Some(draft)) => draft,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%graph_id, %error, "committed goal notification identity unavailable");
                return;
            }
        };
        let mut payload = json!({"graph_id":graph_id,"goal_id":draft.contract.goal_id,"goal_version":draft.contract.version,"workspace_id":draft.contract.workspace_id,"change":change});
        if let Some(fields) = fields.as_object() {
            payload.as_object_mut().unwrap().extend(fields.clone());
        }
        // Broadcast delivery is optional; reconnects reload committed records.
        let _ = hub.publish(YiEvent::new(
            "task.goal.changed",
            "goal-persistence",
            payload,
        ));
    }

    pub(crate) fn publish_goal_conversation_change(
        &self,
        conversation_id: &str,
        change: &str,
        status: Option<&str>,
    ) {
        if self.goal_events.lock().unwrap().is_none() {
            return;
        }
        match self.goal_event_graph_for_conversation(conversation_id) {
            Ok(Some(graph_id)) => {
                let mut fields = json!({"conversation_id":conversation_id});
                if let Some(status) = status {
                    fields["status"] = json!(status);
                }
                self.publish_goal_change(&graph_id, change, fields);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%conversation_id, %error, "committed goal conversation notification unavailable")
            }
        }
    }

    pub(crate) fn goal_event_graph_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<String>, String> {
        if self.goal_events.lock().unwrap().is_none() {
            return Ok(None);
        }
        let identity = self
            .load_conversation_goal_binding(conversation_id)
            .and_then(|binding| {
                binding
                    .map(|binding| {
                        self.confirmed_proof_draft_for_goal(&binding.goal_id, binding.goal_version)
                            .map_err(|error| error.to_string())
                    })
                    .transpose()
            });
        Ok(identity?.flatten().map(|draft| draft.graph_id))
    }
}
