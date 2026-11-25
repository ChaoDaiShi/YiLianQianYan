//! Durable, body-free change feed. The source row and its event share a SQLite transaction.
use crate::{app::state::AppServer, db::Database};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc, time::Duration};

#[derive(Deserialize, Default, Clone)]
struct Cursor {
    after: Option<i64>,
    limit: Option<usize>,
    stream_id: Option<String>,
}
#[derive(Serialize)]
struct ChangeEvent {
    event_id: String,
    schema_version: u32,
    sequence: i64,
    stream_id: String,
    event_type: String,
    subject_id: String,
    payload: Value,
    created_at: i64,
}
fn page(db: &Database, q: &Cursor) -> rusqlite::Result<Vec<ChangeEvent>> {
    let conn = db.conn();
    let common="SELECT event_id,sequence,stream_id,event_type,subject_id,payload,created_at FROM activity_events WHERE sequence>?1";
    let sql = if q.stream_id.is_some() {
        format!("{common} AND stream_id=?3 ORDER BY sequence LIMIT ?2")
    } else {
        format!("{common} ORDER BY sequence LIMIT ?2")
    };
    let mut stmt = conn.prepare(&sql)?;
    stmt.raw_bind_parameter(1, q.after.unwrap_or(0))?;
    stmt.raw_bind_parameter(2, q.limit.unwrap_or(100).clamp(1, 500) as i64)?;
    if let Some(stream) = &q.stream_id {
        stmt.raw_bind_parameter(3, stream)?;
    }
    let mut rows = stmt.raw_query();
    let mut events = Vec::new();
    while let Some(r) = rows.next()? {
        let text: String = r.get(5)?;
        let payload = serde_json::from_str(&text).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;
        events.push(ChangeEvent {
            event_id: r.get(0)?,
            schema_version: 1,
            sequence: r.get(1)?,
            stream_id: r.get(2)?,
            event_type: r.get(3)?,
            subject_id: r.get(4)?,
            payload,
            created_at: r.get(6)?,
        });
    }
    Ok(events)
}
async fn history(
    State(s): State<Arc<AppServer>>,
    Query(q): Query<Cursor>,
) -> Result<Json<Value>, StatusCode> {
    let events = page(&s.db, &q).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(
        json!({"next_cursor":events.last().map(|e|e.sequence).or(q.after).unwrap_or(0),"events":events}),
    ))
}
async fn conversation(
    State(s): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(mut q): Query<Cursor>,
) -> Result<Json<Value>, StatusCode> {
    let exists: bool =
        s.db.conn()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
                [&id],
                |r| r.get(0),
            )
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    q.stream_id = Some(format!("conversation:{id}"));
    history(State(s), Query(q)).await
}
async fn stream(
    State(s): State<Arc<AppServer>>,
    Query(mut q): Query<Cursor>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    if let Some(last) = headers.get("last-event-id") {
        let after = last
            .to_str()
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n >= 0)
            .ok_or(StatusCode::BAD_REQUEST)?;
        q.after = Some(after);
    }
    let output = async_stream::stream! {
        loop {
            let batch=match page(&s.db,&q) {
                Ok(batch)=>batch,Err(_)=>{yield Ok(Event::default().event("error").data("event_store_unavailable"));break;}
            };
            if batch.is_empty(){tokio::time::sleep(Duration::from_millis(750)).await;continue;}
            for item in batch {
                q.after=Some(item.sequence);
                let frame=Event::default().id(item.sequence.to_string()).event(item.event_type.clone()).json_data(&item);
                match frame { Ok(frame)=>yield Ok(frame),Err(_)=>{yield Ok(Event::default().event("error").data("serialization_failed"));return;} }
            }
        }
    };
    Ok(Sse::new(output).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}
pub(crate) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/v1/events", get(history))
        .route("/api/v1/events/stream", get(stream))
        .route("/api/v1/conversations/:id/events", get(conversation))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rollback_is_silent_and_replay_excludes_payload_bodies() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let seed="INSERT INTO conversations(id,title,created_at,updated_at) VALUES('c','title',1,1);
            INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('m','c','user','private body',1);";
        db.conn()
            .execute_batch(&format!("BEGIN;{seed}ROLLBACK;"))
            .unwrap();
        let mut q = Cursor {
            stream_id: Some("conversation:c".into()),
            ..Default::default()
        };
        assert!(page(&db, &q).unwrap().is_empty());
        db.conn().execute_batch(seed).unwrap();
        let event = page(&db, &q).unwrap().pop().unwrap();
        assert_eq!(event.subject_id, "m");
        assert!(!serde_json::to_string(&event)
            .unwrap()
            .contains("private body"));
        q.after = Some(event.sequence);
        assert!(page(&db, &q).unwrap().is_empty());
    }
}
