//! File checkpoint operations are explicit UI/API actions and reuse graph checkpoint identities.
use super::*;
use crate::modules::task::{file_checkpoint, TaskGraphId};

#[derive(Deserialize)]
pub struct CaptureFilesRequest {
    pub expected_revision: u64,
    pub files: Vec<String>,
}
#[derive(Deserialize)]
pub struct RestoreFilesRequest {
    pub expected_revision: u64,
    pub confirmed: bool,
}

pub async fn capture_files_handler(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Json(request): Json<CaptureFilesRequest>,
) -> Response {
    let graph_id = match TaskGraphId::new(&id) {
        Ok(v) => v,
        Err(e) => return proof_error_response(ProofError::new("invalid_graph_id", e.to_string())),
    };
    let checkpoint = match server
        .task_world
        .checkpoint(&graph_id, request.expected_revision, now())
    {
        Ok(v) => v,
        Err(e) => {
            return proof_error_response(ProofError::new("checkpoint_unavailable", e.to_string()))
        }
    };
    match file_checkpoint::capture(
        std::path::Path::new(&server.workspace_root),
        &checkpoint,
        &request.files,
    ) {
        Ok(manifest) => (
            StatusCode::CREATED,
            Json(json!({"manifest":manifest,"external_effects_reverted":false})),
        )
            .into_response(),
        Err(e) => proof_error_response(ProofError::new("file_checkpoint_failed", e)),
    }
}
pub async fn preview_files_handler(
    State(server): State<Arc<AppServer>>,
    Path((id, checkpoint)): Path<(String, String)>,
) -> Response {
    let root = std::path::Path::new(&server.workspace_root);
    match file_checkpoint::load(root, &checkpoint).and_then(|manifest| {
        if manifest.graph_id != id {
            return Err("检查点不属于该图".into());
        }
        file_checkpoint::preview(root, &checkpoint)
    }) {
        Ok(items) => Json(json!({"items":items,"external_effects_reverted":false})).into_response(),
        Err(e) => proof_error_response(ProofError::new("file_checkpoint_failed", e)),
    }
}
pub async fn restore_files_handler(
    State(server): State<Arc<AppServer>>,
    Path((id, checkpoint)): Path<(String, String)>,
    Json(request): Json<RestoreFilesRequest>,
) -> Response {
    let root = std::path::Path::new(&server.workspace_root);
    let graph_id = match TaskGraphId::new(&id) {
        Ok(v) => v,
        Err(e) => return proof_error_response(ProofError::new("invalid_graph_id", e.to_string())),
    };
    let manifest = match file_checkpoint::load(root, &checkpoint) {
        Ok(v) => v,
        Err(e) => return proof_error_response(ProofError::new("file_checkpoint_failed", e)),
    };
    if manifest.graph_id != id {
        return proof_error_response(ProofError::new(
            "file_checkpoint_failed",
            "检查点不属于该图",
        ));
    }
    // Runtime restore owns the control lock, revision check and active execution guard.
    // Check the file preview first so a conflict cannot mutate graph state.
    let preview = match file_checkpoint::preview(root, &checkpoint) {
        Ok(v) => v,
        Err(e) => return proof_error_response(ProofError::new("file_checkpoint_failed", e)),
    };
    if !request.confirmed
        || preview
            .iter()
            .any(|(_, d)| *d == file_checkpoint::RestoreDecision::Conflict)
    {
        return proof_error_response(ProofError::new(
            "file_checkpoint_confirmation_required",
            "请确认预览并先处理文件冲突",
        ));
    }
    match server
        .task_world
        .restore(&graph_id, &checkpoint, request.expected_revision, now())
    {
        Ok(graph) => match file_checkpoint::restore(root, &checkpoint, true) {
            Ok(items) => {
                Json(json!({"graph":graph,"items":items,"external_effects_reverted":false}))
                    .into_response()
            }
            Err(e) => proof_error_response(ProofError::new(
                "file_restore_partial",
                format!("图定义已恢复，文件恢复未完成：{e}"),
            )),
        },
        Err(e) => proof_error_response(ProofError::new("checkpoint_unavailable", e.to_string())),
    }
}
