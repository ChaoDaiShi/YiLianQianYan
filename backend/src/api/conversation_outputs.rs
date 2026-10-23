use crate::app::state::AppServer;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

type Error = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    (status, Json(json!({"error":message.into()})))
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route("/api/conversations/:conversation_id/outputs", get(list))
        .route(
            "/api/conversations/:conversation_id/outputs/code-candidates",
            get(code_candidates),
        )
        .route(
            "/api/conversations/:conversation_id/outputs/from-message",
            post(save_code),
        )
        .route(
            "/api/conversations/:conversation_id/outputs/:id",
            get(detail),
        )
}

#[derive(Serialize)]
struct CodeCandidate {
    message_id: String,
    block_index: usize,
    language: String,
    preview: String,
    created_at: i64,
}

/// Extract only closed fenced blocks from persisted assistant messages. A code
/// snapshot is an authored snippet, never evidence that a file was run or tested.
fn code_blocks(content: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut fence: Option<(char, usize, String)> = None;
    let mut code = String::new();
    for raw_line in content.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 {
            if fence.is_some() {
                code.push_str(raw_line);
            }
            continue;
        }
        if let Some((marker, count, language)) = &fence {
            let run = trimmed
                .chars()
                .take_while(|character| character == marker)
                .count();
            if run >= *count && trimmed.chars().skip(run).all(char::is_whitespace) {
                let finished = std::mem::take(&mut code);
                if !finished.trim().is_empty() {
                    blocks.push((language.clone(), finished));
                }
                fence = None;
            } else {
                code.push_str(raw_line);
            }
        } else if let Some(marker @ ('`' | '~')) = trimmed.chars().next() {
            let count = trimmed
                .chars()
                .take_while(|character| *character == marker)
                .count();
            if count >= 3 {
                let info = trimmed.chars().skip(count).collect::<String>();
                let language = info.split_whitespace().next().unwrap_or("text").to_string();
                fence = Some((marker, count, language));
            }
        }
    }
    blocks
}

async fn code_candidates(
    State(server): State<Arc<AppServer>>,
    Path(conversation): Path<String>,
) -> Result<Json<Value>, Error> {
    let conversation = server
        .db
        .get_conversation(&conversation)
        .map_err(|_| error(StatusCode::NOT_FOUND, "对话不存在"))?;
    let candidates = conversation
        .messages
        .iter()
        .filter(|message| message.role == "assistant")
        .flat_map(|message| {
            code_blocks(&message.content)
                .into_iter()
                .enumerate()
                .map(|(index, (language, content))| CodeCandidate {
                    message_id: message.id.clone(),
                    block_index: index,
                    language,
                    preview: content.chars().take(500).collect(),
                    created_at: message.created_at,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"candidates":candidates})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveCode {
    message_id: String,
    block_index: usize,
    previous_output_id: Option<String>,
}
static CODE_SAVES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn save_code(
    State(server): State<Arc<AppServer>>,
    Path(conversation): Path<String>,
    Json(body): Json<SaveCode>,
) -> Result<Json<Value>, Error> {
    let _guard = CODE_SAVES.lock().await;
    let stored = server
        .db
        .get_conversation(&conversation)
        .map_err(|_| error(StatusCode::NOT_FOUND, "对话不存在"))?;
    let message = stored
        .messages
        .iter()
        .find(|message| message.id == body.message_id && message.role == "assistant")
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "当前对话没有该助手消息"))?;
    let (language, content) = code_blocks(&message.content)
        .into_iter()
        .nth(body.block_index)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "消息没有对应的完整代码块"))?;
    let call = format!("message:{}:block:{}", body.message_id, body.block_index);
    if let Some(output) = server
        .db
        .conversation_output_for_call(&conversation, &call)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
    {
        if output.previous_output_id != body.previous_output_id {
            return Err(error(
                StatusCode::CONFLICT,
                "该代码块已保存，不能重新指定版本关系",
            ));
        }
        return Ok(Json(json!({"output":output,"created":false})));
    }
    if let Some(previous) = &body.previous_output_id {
        if !server
            .db
            .conversation_output(&conversation, previous)
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
            .is_some_and(|output| output.kind == "code" && output.status == "ready")
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "前一版本必须是当前对话的代码成果",
            ));
        }
    }
    let extension = match language.as_str() {
        "rust" | "rs" => "rs",
        "typescript" | "ts" => "ts",
        "javascript" | "js" => "js",
        "python" | "py" => "py",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "json" => "json",
        "html" => "html",
        "css" => "css",
        "sql" => "sql",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        _ => "txt",
    };
    let name = format!(
        "code-{}-{}.{}",
        uuid::Uuid::new_v4(),
        body.block_index + 1,
        extension
    );
    let resource = server.resource_service.ingest(&name,"text/plain",content.as_bytes(),json!({
        "source":"assistant-code-block","conversation_id":conversation,"source_message_id":body.message_id,"block_index":body.block_index,"language":language,"execution_verified":false,
    })).map_err(|e|error(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()))?;
    server
        .resource_service
        .read_bytes(&resource.id)
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "代码文件保存后完整性校验失败",
            )
        })?;
    let output = server
        .db
        .register_conversation_code_output(
            &conversation,
            &body.message_id,
            body.block_index,
            &format!("{} 代码片段", language),
            &resource.id,
            body.previous_output_id.as_deref(),
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({"output":output,"created":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_candidates_require_closed_fences_and_preserve_content() {
        assert_eq!(
            code_blocks("示例\n```rust\nfn main() {}\n```\n```python\nunfinished"),
            vec![("rust".into(), "fn main() {}\n".into())]
        );
        assert_eq!(
            code_blocks("~~~~js\nconst fence = '```';\n~~~~"),
            vec![("js".into(), "const fence = '```';\n".into())]
        );
        assert!(code_blocks("    ```js\n    not a fenced block\n    ```").is_empty());
    }
}
async fn list(
    State(server): State<Arc<AppServer>>,
    Path(conversation): Path<String>,
) -> Result<Json<Value>, Error> {
    server
        .db
        .get_conversation(&conversation)
        .map_err(|_| error(StatusCode::NOT_FOUND, "对话不存在"))?;
    let outputs = server
        .db
        .conversation_outputs(&conversation)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({"outputs":outputs})))
}
async fn detail(
    State(server): State<Arc<AppServer>>,
    Path((conversation, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    let output = server
        .db
        .conversation_output(&conversation, &id)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "当前对话中不存在该成果"))?;
    Ok(Json(json!({"output":output})))
}
