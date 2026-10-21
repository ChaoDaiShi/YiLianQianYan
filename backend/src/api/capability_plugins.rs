//! Install declarative plugin contributions once; feature pages only reuse IDs.
use crate::app::state::AppServer;
use crate::modules::capability::{
    import_owner,
    import_store::{InstalledPackage, Package},
};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::BTreeMap;

fn contribution_id(plugin: &str, path: &str) -> String {
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{plugin}:{path}").as_bytes())
    );
    format!("plugin-{}", &digest[..24])
}
fn manifest(package: &Package) -> Result<crate::plugin::PluginManifest, String> {
    crate::plugin::parse_manifest(
        package
            .files
            .get("plugin.json")
            .ok_or("plugin_manifest_missing")?,
    )
    .map_err(|e| e.to_string())
}
pub fn enabled_plugin_resources(
    db: &crate::db::Database,
) -> Result<Vec<serde_json::Value>, String> {
    let mut result = Vec::new();
    for record in crate::modules::capability::import_store::list(db)?
        .into_iter()
        .filter(|r| r.installed && r.enabled && r.package.kind == "plugin")
    {
        let manifest = manifest(&record.package)?;
        for (kind, paths) in [
            ("skill", manifest.contributes.skills),
            ("workflow", manifest.contributes.workflows),
            ("agent", manifest.contributes.agents),
        ] {
            for path in paths {
                if let Some(id) = record.resources.get(&path) {
                    result.push(serde_json::json!({"plugin_id":record.package.id,"plugin_name":record.package.name,"kind":kind,"id":id,"path":path,"name":format!("{} / {}",record.package.name,path)}));
                }
            }
        }
    }
    Ok(result)
}
#[derive(serde::Deserialize)]
struct AgentContribution {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default = "default_iterations")]
    max_iterations: u32,
}
fn default_iterations() -> u32 {
    10
}

pub fn change_plugin_runtime(
    server: &AppServer,
    record: &InstalledPackage,
    action: &str,
) -> Result<(), String> {
    if !matches!(action, "enable" | "disable" | "uninstall" | "rollback") {
        return Ok(());
    }
    let manifest = manifest(&record.package)?;
    let enabling = action == "enable";
    let now = chrono::Utc::now().timestamp_millis();
    let mut workflows = Vec::new();
    let mut agents = Vec::new();
    // Parse and validate all structured contributions before writing anything.
    for path in &manifest.contributes.workflows {
        let value: serde_json::Value = serde_json::from_str(
            record
                .package
                .files
                .get(path)
                .ok_or("plugin_contribution_missing")?,
        )
        .map_err(|_| "invalid_plugin_workflow")?;
        let definition: crate::modules::workflow::WorkflowGraphDefinition =
            serde_json::from_value(value.get("definition").cloned().unwrap_or(value.clone()))
                .map_err(|_| "invalid_plugin_workflow")?;
        definition.validate().map_err(|e| e.to_string())?;
        workflows.push((
            contribution_id(&record.package.id, path),
            value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(path)
                .to_string(),
            value
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            definition,
        ));
    }
    for path in &manifest.contributes.agents {
        let agent: AgentContribution = serde_json::from_str(
            record
                .package
                .files
                .get(path)
                .ok_or("plugin_contribution_missing")?,
        )
        .map_err(|_| "invalid_plugin_agent")?;
        if agent.name.trim().is_empty() || agent.max_iterations == 0 || agent.max_iterations > 60 {
            return Err("invalid_plugin_agent".into());
        }
        agents.push((contribution_id(&record.package.id, path), agent));
    }
    let mut saved = Vec::new();
    for path in &manifest.contributes.skills {
        let store = server
            .managed_skill_store()
            .ok_or("managed_skill_directory_unavailable")?;
        let id = contribution_id(&record.package.id, path);
        let snapshot = if enabling {
            let mut package = crate::modules::capability::import_store::markdown_package(
                &id,
                path,
                &record.package.version,
                record
                    .package
                    .files
                    .get(path)
                    .ok_or("plugin_contribution_missing")?,
                &record.package.source,
            )?;
            let prefix = std::path::Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if !prefix.is_empty() {
                for (resource, content) in &record.package.files {
                    if let Some(relative) = resource.strip_prefix(&format!("{prefix}/")) {
                        package.files.insert(relative.into(), content.clone());
                        if let Some(encoding) = record.package.file_encodings.get(resource) {
                            package
                                .file_encodings
                                .insert(relative.into(), encoding.clone());
                        }
                    }
                }
            }
            crate::modules::capability::import_store::validate_package(&mut package)?;
            import_owner::activate(&store, &package)
        } else {
            import_owner::deactivate(&store, &id)
        };
        match snapshot {
            Ok(snapshot) => saved.push(snapshot),
            Err(error) => {
                for snapshot in saved.into_iter().rev() {
                    import_owner::restore(snapshot)?;
                }
                return Err(error);
            }
        }
    }
    let write = (|| {
        let mut conn = server.db.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        if enabling {
            for (id, name, description, definition) in workflows {
                let definition_json =
                    serde_json::to_string(&definition).map_err(|e| e.to_string())?;
                tx.execute("INSERT INTO workflow_graphs(id,name,description,schema_version,definition_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,schema_version=excluded.schema_version,definition_json=excluded.definition_json,updated_at=excluded.updated_at",rusqlite::params![id,name,description,definition.schema_version,definition_json,now]).map_err(|e|e.to_string())?;
            }
        }
        for (id, agent) in agents {
            if enabling {
                tx.execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,model,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,'database',?9,?9) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,instructions=excluded.instructions,allowed_tools=excluded.allowed_tools,model=excluded.model,capabilities=excluded.capabilities,max_iterations=excluded.max_iterations,enabled=1,updated_at=excluded.updated_at",rusqlite::params![id,agent.name,agent.description,agent.instructions,serde_json::to_string(&agent.allowed_tools).map_err(|e|e.to_string())?,agent.model,serde_json::to_string(&agent.capabilities).map_err(|e|e.to_string())?,agent.max_iterations,now]).map_err(|e|e.to_string())?;
            } else {
                tx.execute(
                    "UPDATE agent_definitions SET enabled=0,updated_at=?2 WHERE id=?1",
                    rusqlite::params![id, now],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    })();
    if let Err(error) = write {
        for snapshot in saved.into_iter().rev() {
            import_owner::restore(snapshot)?;
        }
        return Err(error);
    }
    server.refresh_skill_discovery();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::capabilities::{change_import_owned, list_plugin_features};
    use axum::extract::State;
    #[tokio::test]
    async fn plugin_contributions_install_once_and_features_follow_enable_state() {
        let root =
            std::env::temp_dir().join(format!("yilian-plugin-install-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let server = std::sync::Arc::new(
            AppServer::new(&root.join("test.db"), &root.to_string_lossy()).unwrap(),
        );
        let manifest = serde_json::json!({"schema_version":1,"id":"demo.plugin","name":"Demo","version":"1.0.0","contributes":{"skills":["skills/SKILL.md"],"agents":["agents/demo.json"],"workflows":["workflows/demo.json"],"features":[{"id":"flow","title":"Run Demo","target":{"kind":"workflow","contribution":"workflows/demo.json"}}]}});
        let mut package = Package { id:"demo.plugin".into(),name:"Demo".into(),version:"1.0.0".into(),kind:"plugin".into(),source:"test".into(),permissions:vec![],content_hash:String::new(),file_encodings:BTreeMap::new(),files:BTreeMap::from([
            ("plugin.json".into(),manifest.to_string()),
            ("skills/SKILL.md".into(),"# Installed plugin skill".into()),
            ("skills/references/guide.txt".into(),"A real bundled reference".into()),
            ("agents/demo.json".into(),serde_json::json!({"name":"Demo agent","instructions":"Help the user."}).to_string()),
            ("workflows/demo.json".into(),serde_json::json!({"name":"Demo flow","definition":{"schema_version":1,"entry_node_id":"out","nodes":[{"id":"out","kind":"output","config":{"type":"output","template":"Complete"}}],"edges":[]}}).to_string())
        ])};
        crate::modules::capability::import_store::validate_package(&mut package).unwrap();
        let preview =
            crate::modules::capability::import_store::inspect(&server.db, package, 1).unwrap();
        let record =
            crate::modules::capability::import_store::confirm(&server.db, &preview.preview_id, 2)
                .unwrap();
        assert_eq!(record.resources.len(), 3);
        assert!(enabled_plugin_resources(&server.db).unwrap().is_empty());
        let enabled =
            change_import_owned(&server, "demo.plugin", record.revision, "enable").unwrap();
        let first = enabled_plugin_resources(&server.db).unwrap();
        assert_eq!(first.len(), 3);
        assert_eq!(server.db.list_workflow_graphs().unwrap().len(), 1);
        assert_eq!(server.db.list_agent_definitions().unwrap().len(), 1);
        let skill_id = enabled.resources.get("skills/SKILL.md").unwrap();
        let skill = server.skill_discovery.read().get(skill_id).unwrap().clone();
        assert_eq!(
            std::fs::read_to_string(skill.root_dir.join("references/guide.txt")).unwrap(),
            "A real bundled reference"
        );
        let axum::Json(features) = list_plugin_features(State(server.clone())).await.unwrap();
        assert_eq!(
            features["features"][0]["resource_id"],
            enabled.resources["workflows/demo.json"]
        );
        let disabled =
            change_import_owned(&server, "demo.plugin", enabled.revision, "disable").unwrap();
        assert!(enabled_plugin_resources(&server.db).unwrap().is_empty());
        assert!(!server.db.list_agent_definitions().unwrap()[0].enabled);
        assert!(server.skill_discovery.read().get(skill_id).is_none());
        let workflow_id = &enabled.resources["workflows/demo.json"];
        assert!(
            crate::modules::workflow::application::run_service::start_workflow_run(
                &server,
                workflow_id
            )
            .await
            .is_err()
        );
        let again =
            change_import_owned(&server, "demo.plugin", disabled.revision, "enable").unwrap();
        assert_eq!(again.resources, record.resources);
        assert_eq!(server.db.list_workflow_graphs().unwrap().len(), 1);
        assert_eq!(server.db.list_agent_definitions().unwrap().len(), 1);
        change_import_owned(&server, "demo.plugin", again.revision, "uninstall").unwrap();
        assert!(
            crate::modules::workflow::application::run_service::start_workflow_run(
                &server,
                workflow_id
            )
            .await
            .is_err()
        );
        let axum::Json(features) = list_plugin_features(State(server.clone())).await.unwrap();
        assert_eq!(features["features"].as_array().unwrap().len(), 0);
        let current = crate::modules::capability::import_store::get(&server.db, "demo.plugin")
            .unwrap()
            .unwrap();
        let restored =
            change_import_owned(&server, "demo.plugin", current.revision, "restore").unwrap();
        change_import_owned(&server, "demo.plugin", restored.revision, "enable").unwrap();
        let mut replacement = enabled.package.clone();
        replacement.version = "2.0.0".into();
        replacement.files.insert("plugin.json".into(), serde_json::json!({"schema_version":1,"id":"demo.plugin","name":"Demo","version":"2.0.0","contributes":{}}).to_string());
        let preview =
            crate::modules::capability::import_store::inspect(&server.db, replacement, 10).unwrap();
        let replacement =
            crate::modules::capability::import_store::confirm(&server.db, &preview.preview_id, 11)
                .unwrap();
        change_import_owned(&server, "demo.plugin", replacement.revision, "enable").unwrap();
        assert!(inactive_plugin_resource_ids(&server.db)
            .unwrap()
            .contains(workflow_id));
        assert!(
            crate::modules::workflow::application::run_service::start_workflow_run(
                &server,
                workflow_id
            )
            .await
            .is_err()
        );
        // Ownership must outlive the two-version rollback history.
        for version in 3..=5 {
            let mut next = replacement.package.clone();
            next.version = format!("{version}.0.0");
            next.files.insert("plugin.json".into(), serde_json::json!({"schema_version":1,"id":"demo.plugin","name":"Demo","version":next.version,"contributes":{}}).to_string());
            let preview =
                crate::modules::capability::import_store::inspect(&server.db, next, 20).unwrap();
            let next = crate::modules::capability::import_store::confirm(
                &server.db,
                &preview.preview_id,
                21,
            )
            .unwrap();
            change_import_owned(&server, "demo.plugin", next.revision, "enable").unwrap();
        }
        assert!(
            crate::modules::capability::import_store::require_active_plugin_resource(
                &server.db,
                workflow_id
            )
            .is_err()
        );
        drop(server);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn feature_targets_must_reference_declared_contributions() {
        let invalid = r#"{"schema_version":1,"id":"demo","name":"Demo","version":"1","contributes":{"features":[{"id":"feature","title":"Feature","target":{"kind":"workflow","contribution":"missing.json"}}]}}"#;
        assert!(crate::plugin::parse_manifest(invalid).is_err());
        let tool_only = r#"{"schema_version":1,"id":"demo","name":"Demo","version":"1"}"#;
        assert!(crate::plugin::parse_manifest(tool_only)
            .unwrap()
            .contributes
            .features
            .is_empty());
    }
}

pub fn inactive_plugin_resource_ids(
    db: &crate::db::Database,
) -> Result<std::collections::HashSet<String>, String> {
    crate::modules::capability::import_store::inactive_plugin_resource_ids(db)
}
