use crate::shared::contracts::SHARED_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct YiEvent {
    pub id: String,
    pub namespace: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub timestamp: i64,
    pub schema_version: u32,
    pub payload: Value,
}

impl YiEvent {
    pub fn new(event_type: &str, source: impl Into<String>, payload: Value) -> Self {
        let namespace = event_type
            .split('.')
            .next()
            .unwrap_or("invalid")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            namespace,
            event_type: event_type.to_string(),
            source: source.into(),
            scope: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            schema_version: SHARED_SCHEMA_VERSION,
            payload,
        }
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_event_type(&self.event_type)?;
        if self.namespace != self.event_type.split('.').next().unwrap_or_default() {
            return Err("event namespace must match the type prefix".to_string());
        }
        if self.source.trim().is_empty() || self.source.len() > 128 {
            return Err("event source must be 1-128 characters".to_string());
        }
        Ok(())
    }
}

pub fn validate_event_type(value: &str) -> Result<(), String> {
    let segments = value.split('.').collect::<Vec<_>>();
    if segments.len() < 2 {
        return Err("event type must contain a namespace".to_string());
    }
    if segments.iter().any(|segment| !valid_segment(segment)) || value.len() > 128 {
        return Err("event type must be namespace.name using lowercase identifiers".to_string());
    }
    Ok(())
}

fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

#[derive(Clone)]
pub struct EventHub {
    sender: broadcast::Sender<YiEvent>,
}

impl EventHub {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub fn publish(&self, event: YiEvent) -> Result<usize, String> {
        event.validate()?;
        self.sender.send(event).map_err(|error| error.to_string())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<YiEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_serialization_roundtrips_and_ignores_additive_fields() {
        let event = YiEvent::new("resource.created", "resource-core", json!({"id": "res-1"}))
            .with_scope("workspace:one");
        let mut value = serde_json::to_value(&event).unwrap();
        value["future_field"] = json!({"consumer": "must-ignore"});

        let decoded: YiEvent = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.event_type, "resource.created");
        assert_eq!(decoded.namespace, "resource");
        assert_eq!(decoded.scope.as_deref(), Some("workspace:one"));
        assert_eq!(decoded.schema_version, 1);
    }

    #[tokio::test]
    async fn event_hub_broadcasts_product_events() {
        let hub = EventHub::new(4);
        let mut subscriber = hub.subscribe();
        let event = YiEvent::new("core.health.changed", "health", json!({"healthy": true}));

        hub.publish(event.clone()).unwrap();

        assert_eq!(subscriber.recv().await.unwrap(), event);
    }

    #[test]
    fn invalid_product_event_names_fail_closed() {
        assert!(validate_event_type("resource.created").is_ok());
        assert!(validate_event_type("mouse_move").is_err());
        assert!(validate_event_type("Desktop.App.Open").is_err());
    }
}
