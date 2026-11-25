//! User-owned persona profiles, kept separate from private execution templates.
use crate::{db::Database, integrations::llm::generation::GenerationOverrides};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyVariation {
    #[default]
    Default,
    Consistent,
    Flexible,
    Playful,
}

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
    pub scenario: String,
    #[serde(default)]
    pub reply_examples: String,
    #[serde(default)]
    pub reply_variation: ReplyVariation,
    #[serde(default)]
    pub generation: GenerationOverrides,
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
impl AgentProfile {
    pub fn generation(&self) -> GenerationOverrides {
        if self.persona_enabled {
            self.persona.generation
        } else {
            GenerationOverrides::default()
        }
    }
}
fn key(id: &str) -> String {
    format!("agent.user_profile.v1:{id}")
}
pub fn reset_profile(db: &Database, id: &str) -> Result<AgentProfile, String> {
    profile_for_agent(db, id)?;
    let profile = AgentProfile::default();
    save_profile(db, id, &profile)?;
    Ok(profile)
}
pub fn save_profile(db: &Database, id: &str, profile: &AgentProfile) -> Result<(), String> {
    profile.persona.generation.validate()?;
    if profile.user_prompt.chars().count() > 32000
        || profile.persona.description.chars().count() > 4000
        || profile.persona.display_name.chars().count() > 120
        || profile.persona.tone.chars().count() > 2000
        || profile.persona.language.chars().count() > 120
        || profile.persona.scenario.chars().count() > 4000
        || profile.persona.reply_examples.chars().count() > 6000
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
            ("扮演场景", persona.scenario.as_str()),
            ("回复示例", persona.reply_examples.as_str()),
        ] {
            if !value.trim().is_empty() {
                fields.push(format!("{label}：{}", value.trim()));
            }
        }
        let variation = match persona.reply_variation {
            ReplyVariation::Default => None,
            ReplyVariation::Consistent => Some("保持称呼、语气与回答结构稳定，优先沿用符合角色的惯用表达。"),
            ReplyVariation::Flexible => Some("保持角色的身份与语气，根据话题自然调整表达和示例。"),
            ReplyVariation::Playful => Some("主动变换表达方式、举例角度和叙述节奏；在适合创作的语境中尝试不同写法，同时保持角色设定与事实准确。"),
        };
        if let Some(variation) = variation {
            fields.push(format!("回复变化：{variation}"));
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
    fn role_scenario_examples_and_variation_roundtrip_in_the_user_prompt_layer() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let profile = AgentProfile {
            persona_enabled: true,
            persona: Persona {
                scenario: "你在为博物馆访客介绍展品".into(),
                reply_examples: "先讲一件展品的来历".into(),
                reply_variation: ReplyVariation::Playful,
                ..Default::default()
            },
            ..Default::default()
        };
        save_profile(&db, "default", &profile).unwrap();
        let mut saved = profile_for_agent(&db, "default").unwrap();
        let prompt = compose_user_persona(&saved);
        assert!(prompt.contains(&saved.persona.scenario));
        assert!(prompt.contains(&saved.persona.reply_examples));
        assert!(prompt.contains("变换表达方式"));
        saved.persona_enabled = false;
        assert!(compose_user_persona(&saved).is_empty());
        let legacy: Persona = serde_json::from_str(r#"{"tone":"沉稳"}"#).unwrap();
        assert_eq!(legacy.reply_variation, ReplyVariation::Default);
    }

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
