//! User-owned persona profiles, kept separate from private execution templates.
use crate::db::Database;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Persona {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tone: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub avatar_resource_id: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProfile {
    #[serde(default)]
    pub user_prompt: String,
    #[serde(default)]
    pub persona_enabled: bool,
    #[serde(default)]
    pub persona: Persona,
}
fn key(id: &str) -> String {
    format!("agent.user_profile.v1:{id}")
}
pub fn save_profile(db: &Database, id: &str, profile: &AgentProfile) -> Result<(), String> {
    if profile.user_prompt.chars().count() > 32000
        || profile.persona.description.chars().count() > 4000
        || profile.persona.display_name.chars().count() > 120
        || profile.persona.tone.chars().count() > 2000
        || profile.persona.language.chars().count() > 120
        || profile
            .persona
            .avatar_resource_id
            .as_ref()
            .is_some_and(|v| v.len() > 256)
    {
        return Err("角色字段超过长度限制".into());
    }
    let value = serde_json::to_string(profile).map_err(|e| e.to_string())?;
    db.conn().execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![key(id),value]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn profile_for_agent(db: &Database, id: &str) -> Result<AgentProfile, String> {
    let existing: Option<String> = db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [key(id)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(value) = existing {
        return serde_json::from_str(&value).map_err(|e| e.to_string());
    }
    let mut profile = AgentProfile::default();
    if id == "default" {
        let config = db.get_settings()?;
        let default = crate::config::types::AppConfig::default();
        if config.agent.system_prompt != default.agent.system_prompt {
            profile.user_prompt = config.agent.system_prompt;
        }
    } else {
        let agent_id = crate::modules::task::AgentId::new(id).map_err(|e| e.to_string())?;
        let agent = db.get_agent_definition(&agent_id)?.ok_or("Agent 不存在")?;
        if agent.source != crate::modules::task::AgentSource::Builtin {
            profile.user_prompt = agent.instructions;
        }
    }
    save_profile(db, id, &profile)?;
    Ok(profile)
}
/// Compose the user layer only. Official execution rules and permission policy
/// stay in their existing trusted layers; callers insert this before bindings.
pub fn compose_user_persona(profile: &AgentProfile) -> String {
    let mut sections = Vec::new();
    if profile.persona_enabled {
        let persona = &profile.persona;
        let mut fields = Vec::new();
        for (label, value) in [
            ("名称", persona.display_name.as_str()),
            ("角色简介", persona.description.as_str()),
            ("语气", persona.tone.as_str()),
            ("语言", persona.language.as_str()),
        ] {
            if !value.trim().is_empty() {
                fields.push(format!("{label}：{}", value.trim()));
            }
        }
        if !fields.is_empty() {
            sections.push(format!("用户启用的角色设定：\n{}", fields.join("\n")));
        }
    }
    if !profile.user_prompt.trim().is_empty() {
        sections.push(format!("用户提示词：\n{}", profile.user_prompt.trim()));
    }
    sections.join("\n\n")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_profiles_persist_and_role_toggle_preserves_user_prompt() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let mut profile = AgentProfile {
            user_prompt: "Keep my custom instructions".into(),
            persona_enabled: true,
            persona: Persona {
                display_name: "沉稳研究员".into(),
                tone: "严谨简洁".into(),
                language: "中文".into(),
                ..Default::default()
            },
        };
        save_profile(&db, "default", &profile).unwrap();
        let first = profile_for_agent(&db, "default").unwrap();
        assert!(compose_user_persona(&first).contains("严谨简洁"));
        profile.persona_enabled = false;
        save_profile(&db, "default", &profile).unwrap();
        let next = compose_user_persona(&profile_for_agent(&db, "default").unwrap());
        assert!(!next.contains("严谨简洁"));
        assert!(next.contains("Keep my custom instructions"));
    }
    #[test]
    fn legacy_user_instructions_migrate_but_official_templates_do_not() {
        use crate::modules::task::{AgentDefinition, AgentId, AgentSource};
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        for (id, source, text) in [
            (
                "official",
                AgentSource::Builtin,
                "PRIVATE OFFICIAL TEMPLATE",
            ),
            ("user", AgentSource::Database, "USER OWNED TEXT"),
            ("imported", AgentSource::LocalFile, "IMPORTED OWNED TEXT"),
        ] {
            db.create_agent_definition(&AgentDefinition {
                id: AgentId::new(id).unwrap(),
                name: id.into(),
                description: String::new(),
                instructions: text.into(),
                allowed_tools: vec![],
                model: None,
                capabilities: vec![],
                max_iterations: 10,
                enabled: true,
                source,
            })
            .unwrap();
        }
        assert!(profile_for_agent(&db, "official")
            .unwrap()
            .user_prompt
            .is_empty());
        assert_eq!(
            profile_for_agent(&db, "user").unwrap().user_prompt,
            "USER OWNED TEXT"
        );
        assert_eq!(
            profile_for_agent(&db, "imported").unwrap().user_prompt,
            "IMPORTED OWNED TEXT"
        );
    }
}
