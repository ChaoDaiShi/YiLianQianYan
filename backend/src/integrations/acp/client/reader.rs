use super::super::model::{
    parse_context_usage, AcpPermissionDecision, AcpPermissionRequest, AcpSessionUpdate,
};
use super::{AcpClient, AcpError, AcpInner, PendingPermission};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;
use tokio_util::sync::CancellationToken;

pub(super) async fn read_messages(inner: Arc<AcpInner>, stdout: ChildStdout) {
    let client = AcpClient {
        inner: Arc::clone(&inner),
    };
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let message: Value = match serde_json::from_str(&line) {
                    Ok(message) => message,
                    Err(_) => {
                        fail_pending(
                            &inner,
                            AcpError::Protocol(
                                "invalid JSON-RPC message from ACP agent".to_string(),
                            ),
                        )
                        .await;
                        break;
                    }
                };
                dispatch_message(&client, message).await;
            }
        }
    }
    fail_pending(&inner, AcpError::ResultUnknown("process_exit".to_string())).await;
}

async fn dispatch_message(client: &AcpClient, message: Value) {
    if let Some(id) = message.get("id").and_then(Value::as_u64) {
        if message.get("method").is_some() {
            if message.get("method").and_then(Value::as_str) == Some("session/request_permission") {
                spawn_permission_request(
                    client.clone(),
                    id,
                    message.get("params").cloned().unwrap_or(Value::Null),
                )
                .await;
            } else {
                let _ = client
                    .write_message(&json!({
                        "jsonrpc":"2.0", "id":id,
                        "error":{"code":-32601,"message":"Method not found"}
                    }))
                    .await;
            }
            return;
        }
        let pending = client.inner.pending.lock().await.remove(&id);
        if let Some(pending) = pending {
            let response = if let Some(error) = message.get("error") {
                Err(AcpError::Server {
                    code: error.get("code").and_then(Value::as_i64).unwrap_or(-32000),
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("ACP request failed")
                        .to_string(),
                })
            } else if let Some(result) = message.get("result") {
                Ok(result.clone())
            } else {
                Err(AcpError::Protocol(
                    "JSON-RPC response omitted result and error".to_string(),
                ))
            };
            let _ = pending.sender.send(response);
        }
        return;
    }

    if message.get("method").and_then(Value::as_str) == Some("session/update") {
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let session_id = params
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let update = params.get("update").cloned().unwrap_or(Value::Null);
        if let Some(usage) = parse_context_usage(&update) {
            client
                .inner
                .usage
                .lock()
                .await
                .insert(session_id.clone(), usage);
        }
        let _ = client
            .inner
            .updates
            .send(AcpSessionUpdate { session_id, update });
    }
}

async fn spawn_permission_request(client: AcpClient, id: u64, params: Value) {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let request = AcpPermissionRequest {
        session_id: session_id.clone(),
        tool_call: params.get("toolCall").cloned().unwrap_or(Value::Null),
        options: params
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    let options = request.options.clone();
    let cancel = CancellationToken::new();
    let handler = client.inner.permission_handler.clone();
    let request_cancel = cancel.clone();
    client
        .inner
        .permissions
        .lock()
        .await
        .insert(id, PendingPermission { session_id, cancel });
    tokio::spawn(async move {
        let decision = if let Some(handler) = handler {
            handler
                .request_permission(request, request_cancel)
                .await
                .unwrap_or(AcpPermissionDecision::Cancelled)
        } else {
            AcpPermissionDecision::Cancelled
        };
        let still_pending = client.inner.permissions.lock().await.remove(&id).is_some();
        if !still_pending {
            return;
        }
        let outcome = match decision {
            AcpPermissionDecision::Selected(option_id)
                if options.iter().any(|option| {
                    option.get("optionId").and_then(Value::as_str) == Some(&option_id)
                }) =>
            {
                json!({ "outcome":"selected", "optionId":option_id })
            }
            AcpPermissionDecision::Selected(_) => json!({ "outcome":"cancelled" }),
            AcpPermissionDecision::Cancelled => json!({ "outcome":"cancelled" }),
        };
        let _ = client
            .write_message(&json!({ "jsonrpc":"2.0", "id":id, "result":{"outcome":outcome} }))
            .await;
    });
}

async fn fail_pending(inner: &AcpInner, error: AcpError) {
    let pending = inner.pending.lock().await.drain().collect::<Vec<_>>();
    for (_, request) in pending {
        let failure = match &error {
            AcpError::ResultUnknown(_) => AcpError::ResultUnknown(request.method),
            error => error.clone(),
        };
        let _ = request.sender.send(Err(failure));
    }
}
