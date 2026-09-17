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
    async fn handler_routes_registered_v1_commands() {
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
                "core.echo",
                "req-api",
                "test",
                json!({"message": "hello"}),
            )),
        )
        .await;
        assert_eq!(
            result.status,
            crate::shared::command::CommandStatus::Succeeded
        );
        assert_eq!(result.result, Some(json!({"message": "hello"})));
        let _ = std::fs::remove_file(db_path);
    }
}
