//! Task World graph routes, composed under the control-session middleware.
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

use crate::{api::task_world, app::state::AppServer};

pub(super) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        // v1 Task World graph state (all routes remain under the control
        // session middleware applied to this protected router).
        .route("/api/task-world/graphs", get(task_world::list_graphs))
        .route("/api/task-world/graphs", post(task_world::create_graph))
        .route(
            "/api/task-world/graphs/:graph_id",
            get(task_world::get_graph).delete(task_world::delete_graph),
        )
        .route(
            "/api/task-world/graphs/:graph_id/detail",
            get(task_world::get_graph_detail),
        )
        .route(
            "/api/task-world/graphs/:graph_id/review",
            post(task_world::review_graph),
        )
        .route(
            "/api/task-world/graphs/:graph_id/canvas-view",
            get(task_world::get_canvas_view),
        )
        .route(
            "/api/task-world/graphs/:graph_id/canvas-view",
            put(task_world::put_canvas_view),
        )
        .route(
            "/api/task-world/graphs/:graph_id/nodes",
            post(task_world::add_node),
        )
        .route(
            "/api/task-world/graphs/:graph_id/nodes/:node_id",
            put(task_world::update_node),
        )
        .route(
            "/api/task-world/graphs/:graph_id/nodes/:node_id",
            delete(task_world::delete_node),
        )
        .route(
            "/api/task-world/graphs/:graph_id/nodes/:node_id/start",
            post(task_world::start_node),
        )
        .route(
            "/api/task-world/graphs/:graph_id/nodes/:node_id/executions",
            post(task_world::start_execution).get(task_world::list_executions),
        )
        .route(
            "/api/task-world/graphs/:graph_id/executions/:execution_id/cancel",
            post(task_world::cancel_execution),
        )
        .route(
            "/api/task-world/graphs/:graph_id/executions/:execution_id/review",
            post(task_world::review_execution_outcome),
        )
        .route(
            "/api/task-world/graphs/:graph_id/rerun",
            post(task_world::rerun),
        )
        .route(
            "/api/task-world/graphs/:graph_id/nodes/:node_id/execute-command",
            post(task_world::execute_command),
        )
        .route(
            "/api/task-world/graphs/:graph_id/nodes/:node_id/cancel-command",
            post(task_world::cancel_command),
        )
        .route(
            "/api/task-world/graphs/:graph_id/edges",
            post(task_world::add_edge),
        )
        .route(
            "/api/task-world/graphs/:graph_id/edges/:from/:to",
            delete(task_world::delete_edge),
        )
        .route(
            "/api/task-world/graphs/:graph_id/checkpoint",
            post(task_world::checkpoint),
        )
        .route(
            "/api/task-world/graphs/:graph_id/restore",
            post(task_world::restore),
        )
        .merge(task_world::proofloop_router())
}
