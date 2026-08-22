// ============================================================
// Skills API — GET /api/skills, GET /api/skills/:name
// ============================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppServer;

/// GET /api/skills — list all discovered skills
pub async fn list_skills(State(server): State<Arc<AppServer>>) -> Json<Vec<serde_json::Value>> {
    let managed = server.managed_skill_store();
    let sd = server.skill_discovery.read();
    let skills: Vec<serde_json::Value> = sd
        .all()
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "description": s.description,
                "path": s.path.to_string_lossy(),
                "editable": managed.as_ref().is_some_and(|store| store.is_editable(&s.path)),
            })
        })
        .collect();
    Json(skills)
}

/// GET /api/skills/:name — load a specific skill's content
pub async fn load_skill(
    State(server): State<Arc<AppServer>>,
    Path(name): Path<String>,
) -> Json<serde_json::Value> {
    let managed = server.managed_skill_store();
    // Try SkillDiscovery first
    let mut sd = server.skill_discovery.write();
    if let Some(skill) = sd.get_mut(&name) {
        match skill.load_content() {
            Ok(content) => {
                return Json(serde_json::json!({
                    "name": skill.name, "description": skill.description,
                    "content": content, "root_dir": skill.root_dir.to_string_lossy(),
                    "editable": managed.as_ref().is_some_and(|store| store.is_editable(&skill.path)),
                }))
            }
            Err(e) => return Json(serde_json::json!({"error": e})),
        }
    }

    // Fallback: check filesystem directly
    for dir in &["./skills", "../skills", "skills"] {
        let path = std::path::Path::new(dir).join(&name).join("SKILL.md");
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    return Json(serde_json::json!({
                        "name": name, "description": "",
                        "content": content, "root_dir": path.parent().unwrap_or(std::path::Path::new(".")).to_string_lossy(),
                        "editable": managed.as_ref().is_some_and(|store| store.is_editable(&path)),
                    }))
                }
                Err(e) => return Json(serde_json::json!({"error": format!("Read error: {}", e)})),
            }
        }
    }

    Json(serde_json::json!({"error": format!("Skill '{}' not found", name)}))
}

#[derive(Debug, Deserialize)]
pub struct CreateSkillRequest {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSkillRequest {
    pub name: String,
    pub content: String,
}

fn management_error(error: crate::skill_management::SkillManagementError) -> (StatusCode, String) {
    use crate::skill_management::SkillManagementError;
    let status = match error {
        SkillManagementError::EmptyName
        | SkillManagementError::UnsafeName
        | SkillManagementError::NameTooLong
        | SkillManagementError::EmptyContent
        | SkillManagementError::ContentTooLarge => StatusCode::BAD_REQUEST,
        SkillManagementError::AlreadyExists => StatusCode::CONFLICT,
        SkillManagementError::NotFound => StatusCode::NOT_FOUND,
        SkillManagementError::OutsideManagedRoot => StatusCode::FORBIDDEN,
        SkillManagementError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}

fn managed_store(
    server: &AppServer,
) -> Result<crate::skill_management::ManagedSkillStore, (StatusCode, String)> {
    server.managed_skill_store().ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            "没有可写的工作区技能目录；外部技能目录保持只读".to_string(),
        )
    })
}

pub async fn create_skill(
    State(server): State<Arc<AppServer>>,
    Json(body): Json<CreateSkillRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let store = managed_store(&server)?;
    let path = store
        .create(&body.name, &body.content)
        .map_err(management_error)?;
    server.refresh_skill_discovery();
    Ok(Json(serde_json::json!({
        "name": body.name,
        "path": path.to_string_lossy(),
        "editable": true,
    })))
}

pub async fn update_skill(
    State(server): State<Arc<AppServer>>,
    Path(current_name): Path<String>,
    Json(body): Json<UpdateSkillRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let store = managed_store(&server)?;
    let path = store
        .update(&current_name, &body.name, &body.content)
        .map_err(management_error)?;
    server.refresh_skill_discovery();
    Ok(Json(serde_json::json!({
        "name": body.name,
        "path": path.to_string_lossy(),
        "editable": true,
    })))
}

pub async fn delete_skill(
    State(server): State<Arc<AppServer>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let store = managed_store(&server)?;
    store.delete(&name).map_err(management_error)?;
    server.refresh_skill_discovery();
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, State};

    struct TempSkillServer {
        root: std::path::PathBuf,
        db_path: std::path::PathBuf,
    }

    impl TempSkillServer {
        fn new(label: &str) -> (Self, Arc<AppServer>) {
            let root = std::env::temp_dir()
                .join(format!("yilian-skill-api-{label}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let db_path = root.join("test.db");
            let server =
                Arc::new(AppServer::new(&db_path, root.to_string_lossy().as_ref()).unwrap());
            (Self { root, db_path }, server)
        }
    }

    impl Drop for TempSkillServer {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.db_path);
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn managed_skill_api_creates_updates_and_deletes_discovered_skill() {
        let (_temp, server) = TempSkillServer::new("crud");

        let _ = create_skill(
            State(server.clone()),
            Json(CreateSkillRequest {
                name: "demo".to_string(),
                content: "# Demo\n\nFirst".to_string(),
            }),
        )
        .await
        .unwrap();

        let Json(listed) = list_skills(State(server.clone())).await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["name"], "demo");
        assert_eq!(listed[0]["editable"], true);

        let _ = update_skill(
            State(server.clone()),
            Path("demo".to_string()),
            Json(UpdateSkillRequest {
                name: "demo-renamed".to_string(),
                content: "# Demo\n\nSecond".to_string(),
            }),
        )
        .await
        .unwrap();
        assert!(server.skill_discovery.read().get("demo-renamed").is_some());
        assert!(server.skill_discovery.read().get("demo").is_none());

        let _ = delete_skill(State(server.clone()), Path("demo-renamed".to_string()))
            .await
            .unwrap();
        assert!(server.skill_discovery.read().get("demo-renamed").is_none());
    }
}
