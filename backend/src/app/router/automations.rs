//! Automation and personal-delegation routes, composed under the control session.
use crate::app::state::AppServer;
use axum::{
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

pub(super) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/automations",
            get(crate::api::automations::list).post(crate::api::automations::create),
        )
        .route(
            "/api/automations/:id",
            put(crate::api::automations::update).delete(crate::api::automations::delete),
        )
        .route(
            "/api/automations/:id/enabled",
            post(crate::api::automations::enabled),
        )
        .route(
            "/api/automations/:id/run",
            post(crate::api::automations::run),
        )
        .route(
            "/api/automations/:id/history",
            get(crate::api::automations::history),
        )
        .route(
            "/api/automations/:id/observations",
            get(crate::api::automations::observations),
        )
        .route(
            "/api/automations/:id/stop",
            post(crate::api::automations::stop),
        )
        .route(
            "/api/automations/:id/personal",
            get(crate::api::automations::personal_detail),
        )
        .route(
            "/api/automations/:id/personal/history",
            get(crate::api::automations::personal_history),
        )
        .route(
            "/api/automations/:id/personal/file",
            get(crate::api::automations::personal_file),
        )
        .route(
            "/api/automations/:id/personal/start",
            post(crate::api::automations::personal_start),
        )
        .route(
            "/api/automations/:id/personal/archive",
            post(crate::api::automations::personal_archive),
        )
        .route(
            "/api/automations/:id/personal/restore",
            post(crate::api::automations::personal_restore),
        )
}
