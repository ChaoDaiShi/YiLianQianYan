//! Trusted application management only. UIA actions remain Gateway tools.
use crate::{app::state::AppServer, software::*};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
type Error = (StatusCode, Json<Value>);
fn error(e: SoftwareError) -> Error {
    let status = if e.code.ends_with("not_found") {
        StatusCode::NOT_FOUND
    } else if e.code.starts_with("stale_")
        || matches!(
            e.code.as_str(),
            "result_unknown"
                | "software_operation_conflict"
                | "software_session_active"
                | "software_operation_pending"
                | "software_launch_blocked"
        )
    {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({"error":e.code,"message":e.message})))
}
async fn worker<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, SoftwareError> + Send + 'static,
) -> Result<Json<T>, Error> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| {
            error(SoftwareError::unknown(format!(
                "Software worker did not finish: {e}"
            )))
        })?
        .map(Json)
        .map_err(error)
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/software", get(list))
        .route("/api/software/registrations", post(register))
        .route(
            "/api/software/registrations/:id",
            axum::routing::put(update).delete(remove),
        )
        .route("/api/software/registrations/:id/discover", post(discover))
        .route("/api/software/registrations/:id/attach", post(attach))
        .route(
            "/api/software/registrations/:id/monitor",
            axum::routing::put(set_monitor),
        )
        .route("/api/software/registrations/:id/launch", post(launch))
        .route(
            "/api/software/registrations/:id/acknowledge",
            post(acknowledge_launch),
        )
        .route("/api/software/sessions/:id/present", post(present))
        .route("/api/software/sessions/:id/release", post(release))
        .route(
            "/api/software/sessions/:id/wechat-frame",
            post(wechat_frame),
        )
        .route(
            "/api/software/sessions/:id/wechat-conversation",
            post(wechat_conversation),
        )
        .route("/api/software/sessions/:id/stop", post(stop))
        .route("/api/software/sessions/:id/resume", post(resume))
        .route("/api/software/sessions/:id/acknowledge", post(acknowledge))
}
async fn list(State(server): State<Arc<AppServer>>) -> Result<Json<Value>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        let registrations = runtime.registrations()?;
        let sessions = runtime.refresh_sessions()?;
        let history = runtime.history()?;
        let monitors = runtime.monitor_statuses();
        let capability_evidence = runtime.capability_evidence(&registrations, &sessions, &monitors);
        Ok(json!({"available":runtime.available(),"registrations":registrations,"sessions":sessions,
            "history":history,"monitors":monitors,"capability_evidence":capability_evidence}))
    }).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetMonitor {
    enabled: bool,
    session_id: Option<String>,
}

async fn set_monitor(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<SetMonitor>,
) -> Result<Json<Vec<crate::software::monitor::MonitorStatus>>, Error> {
    worker(move || {
        crate::software::monitor::set_enabled(server.clone(), id, input.enabled, input.session_id)?;
        Ok(server.software_runtime.monitor_statuses())
    })
    .await
}
async fn register(
    State(server): State<Arc<AppServer>>,
    Json(input): Json<RegistrationInput>,
) -> Result<Json<Registration>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || runtime.register(input)).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    revision: u64,
    config: RegistrationInput,
}
async fn update(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Update>,
) -> Result<Json<Registration>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || runtime.update(&id, input.revision, input.config)).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: u64,
}
async fn remove(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Revision>,
) -> Result<Json<Value>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.remove(&id, input.revision)?;
        Ok(json!({"removed":true}))
    })
    .await
}
async fn discover(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || Ok(json!({"candidates":runtime.discover(&id)?}))).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attach {
    operation_id: String,
    identity: WindowIdentity,
}
async fn attach(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Attach>,
) -> Result<Json<Session>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || runtime.attach(&id, input.identity, &input.operation_id)).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Launch {
    operation_id: String,
    confirmed: bool,
    revision: u64,
    expected_executable: String,
}
async fn launch(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Launch>,
) -> Result<Json<Session>, Error> {
    if !input.confirmed {
        return Err(error(SoftwareError::new(
            "software_launch_confirmation_required",
            "Confirm the exact registered executable before launching",
        )));
    }
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.launch_confirmed(
            &id,
            input.revision,
            &input.expected_executable,
            &input.operation_id,
        )
    })
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchAcknowledgement {
    operation_id: String,
    confirmed: bool,
}
async fn acknowledge_launch(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<LaunchAcknowledgement>,
) -> Result<Json<Value>, Error> {
    if !input.confirmed {
        return Err(error(SoftwareError::new(
            "software_inspection_required",
            "Inspect the original launch result before acknowledging",
        )));
    }
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.acknowledge_launch(&id, &input.operation_id)?;
        Ok(json!({"acknowledged":true,"replayed":false}))
    })
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Present {
    generation: u64,
    operation_id: String,
    mode: String,
    viewport: Option<Viewport>,
}
async fn present(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Present>,
) -> Result<Json<Session>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.present(
            SessionTarget {
                session_id: id,
                generation: input.generation,
            },
            &input.mode,
            input.viewport,
            &input.operation_id,
        )
    })
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    generation: u64,
    operation_id: String,
}
async fn release(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Operation>,
) -> Result<Json<Session>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.release(
            SessionTarget {
                session_id: id,
                generation: input.generation,
            },
            &input.operation_id,
        )
    })
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Generation {
    generation: u64,
}
async fn wechat_conversation(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Generation>,
) -> Result<impl axum::response::IntoResponse, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    let reply = worker(move || {
        runtime.wechat_conversation(&SessionTarget {
            session_id: id,
            generation: input.generation,
        })
    })
    .await?;
    Ok(([(axum::http::header::CACHE_CONTROL, "no-store")], reply))
}

async fn wechat_frame(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Generation>,
) -> Result<impl axum::response::IntoResponse, Error> {
    use base64::Engine;
    let runtime = Arc::clone(&server.software_runtime);
    let reply = worker(move || {
        let png = runtime.wechat_frame(SessionTarget {
            session_id: id,
            generation: input.generation,
        })?;
        Ok(json!({"png": base64::engine::general_purpose::STANDARD.encode(png)}))
    })
    .await?;
    Ok(([(axum::http::header::CACHE_CONTROL, "no-store")], reply))
}

async fn stop(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Generation>,
) -> Result<Json<Session>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.stop(SessionTarget {
            session_id: id,
            generation: input.generation,
        })
    })
    .await
}
async fn resume(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Generation>,
) -> Result<Json<Session>, Error> {
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.resume(SessionTarget {
            session_id: id,
            generation: input.generation,
        })
    })
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Acknowledge {
    generation: u64,
    confirmed: bool,
}
async fn acknowledge(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(input): Json<Acknowledge>,
) -> Result<Json<Session>, Error> {
    if !input.confirmed {
        return Err(error(SoftwareError::new(
            "software_inspection_required",
            "Inspect the original application before acknowledging an unknown result",
        )));
    }
    let runtime = Arc::clone(&server.software_runtime);
    worker(move || {
        runtime.acknowledge_unknown(SessionTarget {
            session_id: id,
            generation: input.generation,
        })
    })
    .await
}
