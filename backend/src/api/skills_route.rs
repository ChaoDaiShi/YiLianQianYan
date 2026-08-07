// ============================================================
// Skills API — GET /api/skills, GET /api/skills/:name
// ============================================================

use axum::{
    extract::{State, Path},
    Json,
};
use std::sync::Arc;

use crate::server::AppServer;

/// GET /api/skills — list all discovered skills
pub async fn list_skills(
    State(server): State<Arc<AppServer>>,
) -> Json<Vec<serde_json::Value>> {
    let sd = server.skill_discovery.read();
    let skills: Vec<serde_json::Value> = sd.all().iter().map(|s| {
        serde_json::json!({
            "name": s.name,
            "description": s.description,
            "path": s.path.to_string_lossy(),
        })
    }).collect();
    Json(skills)
}

/// GET /api/skills/:name — load a specific skill's content
pub async fn load_skill(
    State(server): State<Arc<AppServer>>,
    Path(name): Path<String>,
) -> Json<serde_json::Value> {
    // Try SkillDiscovery first
    let mut sd = server.skill_discovery.write();
    if let Some(skill) = sd.get_mut(&name) {
        match skill.load_content() {
            Ok(content) => return Json(serde_json::json!({
                "name": skill.name, "description": skill.description,
                "content": content, "root_dir": skill.root_dir.to_string_lossy(),
            })),
            Err(e) => return Json(serde_json::json!({"error": e})),
        }
    }

    // Fallback: check filesystem directly
    for dir in &["./skills", "../skills", "skills"] {
        let path = std::path::Path::new(dir).join(&name).join("SKILL.md");
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => return Json(serde_json::json!({
                    "name": name, "description": "",
                    "content": content, "root_dir": path.parent().unwrap_or(std::path::Path::new(".")).to_string_lossy(),
                })),
                Err(e) => return Json(serde_json::json!({"error": format!("Read error: {}", e)})),
            }
        }
    }

    Json(serde_json::json!({"error": format!("Skill '{}' not found", name)}))
}
