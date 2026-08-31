use crate::server::AppServer;
use crate::shared::command::{CommandRequest, CommandResult};
use axum::{extract::State, Json};
use std::sync::Arc;

pub async fn execute_handler(
    State(server): State<Arc<AppServer>>,
    Json(request): Json<CommandRequest>,
) -> Json<CommandResult> {
    Json(server.command_router.execute(request))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::ControlSession;
    use serde_json::json;

    #[tokio::test]
    async fn handler_returns_explicit_mock_result() {
        let db_path =
            std::env::temp_dir().join(format!("yilian-command-{}.db", uuid::Uuid::new_v4()));
        let server = Arc::new(
            AppServer::new_with_control_session(
                &db_path,
                ".",
                ControlSession::new("c".repeat(64)).unwrap(),
            )
            .unwrap(),
        );
        let Json(result) = execute_handler(
            State(server),
            Json(CommandRequest::new(
                "desktop.space.switch",
                "req-api",
                "test",
                json!({"space": "mock-space"}),
            )),
        )
        .await;
        assert_eq!(result.result.unwrap()["simulated"], true);
        let _ = std::fs::remove_file(db_path);
    }
}
