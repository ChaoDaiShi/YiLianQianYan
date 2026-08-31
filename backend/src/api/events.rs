use crate::server::AppServer;
use crate::shared::event::YiEvent;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::Stream;
use serde_json::json;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::broadcast::error::RecvError;

pub async fn events_handler(
    State(server): State<Arc<AppServer>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut receiver = server.event_hub.subscribe();
    let stream = async_stream::stream! {
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
            let sse = Event::default()
                .id(event.id.clone())
                .event(event.event_type.clone())
                .json_data(event)
                .unwrap_or_else(|_| Event::default().event("core.events.serialization_error"));
            yield Ok(sse);
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}
