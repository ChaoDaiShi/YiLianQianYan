//! Task World HTTP surface tests.
//!
//! Shared fixtures live here; the suites are split so a change to the
//! planner/review path does not require reading the execution-lifecycle tests.

mod execution;
mod planning;

use super::*;

use crate::modules::task::application::execution_service::{dispatch_execution, executor_resolver};
use crate::modules::task::{GraphRevision, TaskGraph, TaskWorldRuntime};
use axum::body::to_bytes;

fn server() -> Arc<AppServer> {
    let server = AppServer::new_with_control_session(
        std::path::Path::new(":memory:"),
        ".",
        crate::safety::ControlSession::new(uuid::Uuid::new_v4().to_string().repeat(2)).unwrap(),
    )
    .unwrap();
    {
        let mut config = server.config.write();
        config.model.api_key.clear();
        config.model.api_key_env.clear();
        config.model.api_key_ref = None;
    }
    Arc::new(server)
}

async fn body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap()
}
mod mcp_canvas;
