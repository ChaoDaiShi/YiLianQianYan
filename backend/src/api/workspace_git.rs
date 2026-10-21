//! Read-only Git inspection for an explicitly selected registered local workspace.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{app::state::AppServer, workspace::WorkspaceId};

mod read;
#[cfg(test)]
mod tests;

type ApiError = (StatusCode, String);
#[derive(Default, Deserialize)]
pub struct WorkspaceGitQuery {
    pub workspace_id: Option<String>,
}
#[derive(Deserialize)]
pub struct WorkspaceGitDiffQuery {
    pub workspace_id: Option<String>,
    pub path: String,
    #[serde(default)]
    pub staged: bool,
}
#[derive(Debug, Serialize)]
pub struct GitFileChange {
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
}
#[derive(Debug, Serialize)]
pub struct WorkspaceGitStatus {
    pub is_repository: bool,
    pub root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub files: Vec<GitFileChange>,
}
#[derive(Debug, Serialize)]
pub struct WorkspaceGitDiff {
    pub path: String,
    pub staged: bool,
    pub untracked: bool,
    pub content: String,
    pub binary: bool,
    pub truncated: bool,
}

fn workspace_root(server: &AppServer, raw: Option<String>) -> Result<PathBuf, ApiError> {
    let root = match raw.filter(|id| !id.trim().is_empty()) {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                "请先选择已设置本地目录的工作空间".into(),
            ))
        }
        Some(raw) => {
            let id = WorkspaceId::new(raw).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            server
                .db
                .get_workspace(&id)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
                .ok_or_else(|| (StatusCode::NOT_FOUND, "工作空间不存在".into()))?
                .root_path
                .filter(|root| !root.trim().is_empty())
                .ok_or_else(|| (StatusCode::BAD_REQUEST, "该工作空间尚未设置本地目录".into()))?
        }
    };
    let root = Path::new(&root);
    if !root.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "请为工作空间设置完整的本地目录路径".into(),
        ));
    }
    if !root.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            "工作空间目录不存在或不是目录".into(),
        ));
    }
    Ok(root.to_path_buf())
}

pub async fn get_workspace_git(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<WorkspaceGitQuery>,
) -> Result<Json<WorkspaceGitStatus>, ApiError> {
    let root = workspace_root(&server, query.workspace_id)?;
    tokio::task::spawn_blocking(move || read::status(&root))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

pub async fn get_workspace_git_diff(
    State(server): State<Arc<AppServer>>,
    Query(query): Query<WorkspaceGitDiffQuery>,
) -> Result<Json<WorkspaceGitDiff>, ApiError> {
    let root = workspace_root(&server, query.workspace_id)?;
    tokio::task::spawn_blocking(move || read::diff(&root, &query.path, query.staged))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

pub fn router() -> axum::Router<Arc<AppServer>> {
    axum::Router::new()
        .route("/api/workspace-git", axum::routing::get(get_workspace_git))
        .route(
            "/api/workspace-git/diff",
            axum::routing::get(get_workspace_git_diff),
        )
}
