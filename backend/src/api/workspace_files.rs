//! Read-only browsing of files in a registered workspace.
use std::{
    fs,
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::app::state::AppServer;

type ApiError = (StatusCode, String);

#[derive(Default, Deserialize)]
struct FileQuery {
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
struct FileEntry {
    name: String,
    path: String,
    kind: &'static str,
    size: u64,
}

#[derive(Serialize)]
struct Directory {
    path: String,
    entries: Vec<FileEntry>,
}

#[derive(Serialize)]
struct TextFile {
    name: String,
    path: String,
    size: u64,
    content: String,
}

fn io_error(error: std::io::Error) -> ApiError {
    let status = match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
        std::io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, error.to_string())
}

fn workspace_root(server: &AppServer, id: String) -> Result<PathBuf, ApiError> {
    let id = crate::workspace::WorkspaceId::new(id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let workspace = server
        .db
        .get_workspace(&id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "工作空间不存在".into()))?;
    let root = workspace
        .root_path
        .filter(|root| !root.trim().is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "此工作空间还没有绑定本地目录".into(),
            )
        })?;
    let root = FilePath::new(&root);
    if !root.is_absolute() || !root.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            "工作空间目录不存在或不是绝对目录".into(),
        ));
    }
    root.canonicalize().map_err(io_error)
}

fn resolve(root: &FilePath, relative: &str) -> Result<PathBuf, ApiError> {
    let requested = if relative.is_empty() {
        root.to_path_buf()
    } else {
        crate::modules::code::relative_path(root, relative)
            .map_err(|error| (StatusCode::BAD_REQUEST, error))?
    };
    let resolved = requested.canonicalize().map_err(io_error)?;
    if !resolved.starts_with(root) {
        return Err((
            StatusCode::BAD_REQUEST,
            "文件路径超出当前工作空间目录".into(),
        ));
    }
    Ok(resolved)
}

fn list_directory(root: &FilePath, relative: &str) -> Result<Directory, ApiError> {
    let directory = resolve(root, relative)?;
    if !directory.is_dir() {
        return Err((StatusCode::BAD_REQUEST, "所选路径不是目录".into()));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        let name = entry.file_name().into_string().map_err(|_| {
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "此目录包含无法显示的文件名".into(),
            )
        })?;
        let path = if relative.is_empty() {
            name.clone()
        } else {
            format!(
                "{}/{}",
                relative.replace('\\', "/").trim_end_matches('/'),
                name
            )
        };
        entries.push(FileEntry {
            name,
            path,
            kind: if file_type.is_symlink() {
                "link"
            } else if file_type.is_dir() {
                "directory"
            } else if file_type.is_file() {
                "file"
            } else {
                "other"
            },
            size: entry.metadata().map_err(io_error)?.len(),
        });
    }
    entries.sort_by(|left, right| {
        (left.kind != "directory")
            .cmp(&(right.kind != "directory"))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(Directory {
        path: relative.replace('\\', "/"),
        entries,
    })
}

fn read_text(root: &FilePath, relative: &str) -> Result<TextFile, ApiError> {
    let file = resolve(root, relative)?;
    if !file.is_file() {
        return Err((StatusCode::BAD_REQUEST, "请选择一个文本文件".into()));
    }
    let bytes = fs::read(&file).map_err(io_error)?;
    let size = bytes.len() as u64;
    if bytes.contains(&0) {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "此文件是二进制内容，不能作为文本预览".into(),
        ));
    }
    let content = String::from_utf8(bytes).map_err(|_| {
        (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "此文件不是 UTF-8 文本，暂不支持预览".into(),
        )
    })?;
    Ok(TextFile {
        name: file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: relative.replace('\\', "/"),
        size,
        content,
    })
}

async fn files(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<Json<Directory>, ApiError> {
    let root = workspace_root(&server, id)?;
    tokio::task::spawn_blocking(move || list_directory(&root, &query.path))
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(Json)
}

async fn file(
    State(server): State<Arc<AppServer>>,
    Path(id): Path<String>,
    Query(query): Query<FileQuery>,
) -> Result<Json<TextFile>, ApiError> {
    let root = workspace_root(&server, id)?;
    tokio::task::spawn_blocking(move || read_text(&root, &query.path))
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(Json)
}

pub fn router() -> axum::Router<Arc<AppServer>> {
    axum::Router::new()
        .route("/api/workspaces/:id/files", axum::routing::get(files))
        .route("/api/workspaces/:id/file", axum::routing::get(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::{fs, path::PathBuf};
    use tower::ServiceExt;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("yilian-workspace-files-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(root.join("project/docs")).unwrap();
            fs::write(root.join("project/README.md"), "# 项目\n真实文件内容\n").unwrap();
            fs::write(root.join("project/binary.dat"), [0, 159, 146, 150]).unwrap();
            fs::write(root.join("outside.txt"), "outside").unwrap();
            Self(root)
        }

        fn server(&self) -> (Arc<AppServer>, String) {
            let server = AppServer::new_with_control_session(
                &self.0.join("test.db"),
                self.0.to_str().unwrap(),
                crate::safety::ControlSession::generate(),
            )
            .unwrap();
            let workspace = crate::workspace::Workspace::new(
                crate::workspace::WorkspaceId::generate(),
                "Registered project".into(),
                String::new(),
                Some(self.0.join("project").to_string_lossy().into_owned()),
                1,
            )
            .unwrap();
            server.db.create_workspace(&workspace).unwrap();
            (Arc::new(server), workspace.id.to_string())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    async fn call(server: Arc<AppServer>, path: &str) -> (StatusCode, Vec<u8>) {
        let response = router()
            .with_state(server)
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        (
            status,
            to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
    }

    #[tokio::test]
    async fn workspace_files_lists_registered_root_and_reads_utf8_text() {
        let fixture = Fixture::new();
        let (server, id) = fixture.server();
        let (status, bytes) =
            call(server.clone(), &format!("/api/workspaces/{id}/files?path=")).await;
        assert_eq!(status, StatusCode::OK);
        let listing: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(listing["entries"][0]["path"], "docs");
        assert_eq!(listing["entries"][0]["kind"], "directory");
        assert!(listing["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "README.md"));
        let (status, bytes) =
            call(server, &format!("/api/workspaces/{id}/file?path=README.md")).await;
        assert_eq!(status, StatusCode::OK);
        let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(document["content"], "# 项目\n真实文件内容\n");
        assert_eq!(document["path"], "README.md");
    }

    #[tokio::test]
    async fn workspace_files_rejects_traversal_absolute_paths_missing_paths_and_binary() {
        let fixture = Fixture::new();
        let (server, id) = fixture.server();
        for (suffix, expected) in [
            ("file?path=../outside.txt", StatusCode::BAD_REQUEST),
            ("files?path=/", StatusCode::BAD_REQUEST),
            ("files?path=missing", StatusCode::NOT_FOUND),
            ("file?path=binary.dat", StatusCode::UNSUPPORTED_MEDIA_TYPE),
            ("file?path=docs", StatusCode::BAD_REQUEST),
        ] {
            let (status, _) = call(server.clone(), &format!("/api/workspaces/{id}/{suffix}")).await;
            assert_eq!(status, expected, "{suffix}");
        }
    }

    #[tokio::test]
    async fn workspace_files_requires_a_registered_existing_root() {
        let fixture = Fixture::new();
        let (server, id) = fixture.server();
        let (status, _) = call(server.clone(), "/api/workspaces/unknown/files").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        fs::remove_dir_all(fixture.0.join("project")).unwrap();
        let (status, _) = call(server, &format!("/api/workspaces/{id}/files")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn workspace_files_rejects_symlinks_that_leave_the_registered_root() {
        let fixture = Fixture::new();
        let (server, id) = fixture.server();
        std::os::unix::fs::symlink(
            fixture.0.join("outside.txt"),
            fixture.0.join("project/link.txt"),
        )
        .unwrap();
        let (status, _) = call(server, &format!("/api/workspaces/{id}/file?path=link.txt")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
