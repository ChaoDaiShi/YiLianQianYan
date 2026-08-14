use axum::{extract::State, Json};
use std::sync::Arc;

use crate::server::{AppServer, DiscoveredSubagent};

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublicSubagentMetadata {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub workdir: Option<String>,
    pub runtime_ready: bool,
}

fn public_metadata(definition: &DiscoveredSubagent) -> PublicSubagentMetadata {
    PublicSubagentMetadata {
        name: definition.name.clone(),
        description: definition.description.clone(),
        allowed_tools: definition.allowed_tools.clone(),
        model: definition.model.clone(),
        // Workdir and source paths are intentionally not exposed to the UI.
        workdir: None,
        runtime_ready: true,
    }
}

pub async fn list_subagents(
    State(server): State<Arc<AppServer>>,
) -> Json<Vec<PublicSubagentMetadata>> {
    Json(server.subagents.iter().map(public_metadata).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::DiscoveredSubagent;

    #[test]
    fn public_metadata_hides_instructions_and_absolute_paths() {
        let definition = DiscoveredSubagent {
            name: "researcher".to_string(),
            description: "Researches a bounded question".to_string(),
            path: r"C:\workspace\.agents\agents\researcher\AGENT.md".to_string(),
            allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
            model: Some("gpt-test".to_string()),
            workdir: Some(r"C:\workspace".to_string()),
            instructions: "PRIVATE_INSTRUCTIONS".to_string(),
        };

        let metadata = public_metadata(&definition);
        let json = serde_json::to_value(&metadata).unwrap();

        assert_eq!(json["name"], "researcher");
        assert_eq!(json["description"], "Researches a bounded question");
        assert_eq!(
            json["allowed_tools"],
            serde_json::json!(["read_file", "grep"])
        );
        assert_eq!(json["model"], "gpt-test");
        assert_eq!(json["workdir"], serde_json::Value::Null);
        assert_eq!(json["runtime_ready"], true);
        assert!(json.get("instructions").is_none());
        assert!(json.get("path").is_none());
        assert!(!json.to_string().contains("PRIVATE_INSTRUCTIONS"));
        assert!(!json.to_string().contains("C:\\workspace"));
    }
}
