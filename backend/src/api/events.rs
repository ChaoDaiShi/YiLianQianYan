use crate::app::state::AppServer;
use crate::shared::event::YiEvent;
use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::Stream;
use serde_json::json;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::broadcast::error::RecvError;
fn encode(event: YiEvent) -> Event {
    Event::default()
        .id(event.id.clone())
        .event(event.event_type.clone())
        .json_data(event)
        .unwrap_or_else(|_| Event::default().event("core.events.serialization_error"))
}

#[derive(serde::Deserialize)]
pub struct EventCursor {
    after: Option<String>,
}

pub async fn events_handler(
    State(server): State<Arc<AppServer>>,
    Query(cursor): Query<EventCursor>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (replay, mut receiver) = server.event_hub.resume(cursor.after.as_deref());
    let stream = async_stream::stream! {
        for event in replay { yield Ok(encode(event)); }
        loop {
            let event = match receiver.recv().await {
                Ok(event) => event,
                Err(RecvError::Lagged(skipped)) => YiEvent::new(
                    "core.events.lagged",
                    "event-hub",
                    json!({"skipped": skipped}),
                ),
                Err(RecvError::Closed) => break,
            };
            yield Ok(encode(event));
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}
