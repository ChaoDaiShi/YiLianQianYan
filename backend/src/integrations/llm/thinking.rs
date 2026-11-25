//! Explicit provider thinking controls. Unknown protocols are rejected rather than ignored.
use crate::config::types::ModelConfig;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    #[default]
    Standard,
    Light,
    Balanced,
    Thinking,
    Maximum,
}
impl ThinkingMode {
    pub fn validate(self, config: &ModelConfig) -> Result<(), String> {
        if !available_modes(config).contains(&self) {
            return Err(format!(
                "模型 {} 不支持所选思考深度，请调整深度或选择其他模型",
                config.name
            ));
        }
        Ok(())
    }
    pub fn apply(self, config: &ModelConfig, body: &mut serde_json::Value) -> Result<(), String> {
        self.validate(config)?;
        if config.provider != "deepseek" {
            if let Some(messages) = body["messages"].as_array_mut() {
                for message in messages {
                    message.as_object_mut().unwrap().remove("reasoning_content");
                }
            }
        }
        let effort = match self {
            Self::Standard => None,
            Self::Light => Some("low"),
            Self::Balanced => Some("medium"),
            Self::Thinking => Some("high"),
            Self::Maximum => Some("max"),
        };
        if config.provider == "deepseek" {
            let mode = if effort.is_some() {
                "enabled"
            } else {
                "disabled"
            };
            body["thinking"] = serde_json::json!({ "type": mode });
            if let Some(effort) = effort.filter(|_| supports_deepseek_effort(&config.name)) {
                body["reasoning_effort"] = serde_json::json!(effort);
            }
            body.as_object_mut().unwrap().remove("temperature");
        } else if supports_openai_effort(config) {
            if let Some(effort) = effort {
                body["reasoning_effort"] = serde_json::json!(effort);
            }
            body.as_object_mut().unwrap().remove("temperature");
            if let Some(tokens) = body.as_object_mut().unwrap().remove("max_tokens") {
                body["max_completion_tokens"] = tokens;
            }
        }
        Ok(())
    }
}
pub fn available_modes(config: &ModelConfig) -> &'static [ThinkingMode] {
    use ThinkingMode::*;
    if config.provider == "deepseek" {
        if supports_deepseek_effort(&config.name) {
            &[Standard, Light, Thinking, Maximum]
        } else {
            &[Standard, Thinking]
        }
    } else if supports_openai_effort(config) {
        &[Standard, Light, Balanced, Thinking]
    } else {
        &[Standard]
    }
}

fn supports_deepseek_effort(model: &str) -> bool {
    matches!(
        model,
        "deepseek-flash" | "deepseek-v4-flash" | "deepseek-v4-pro"
    )
}

pub(super) fn supports_openai_effort(config: &ModelConfig) -> bool {
    let name = config.name.as_str();
    config.provider == "openai"
        && !name.ends_with("-pro")
        && !name.contains("-pro-")
        && !name.contains("-search-")
        && !name.starts_with("o1-mini")
        && !name.starts_with("o1-preview")
        && (name.starts_with("gpt-5")
            || ["o1", "o3", "o4"].iter().any(|family| {
                name == *family
                    || name
                        .strip_prefix(family)
                        .is_some_and(|suffix| suffix.starts_with('-'))
            }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slider_values_use_provider_effort_without_reinterpreting_legacy_high() {
        let mut config = ModelConfig::default();
        config.provider = "openai".into();
        config.name = "gpt-5.4".into();
        let mut body = serde_json::json!({"max_tokens":128});
        ThinkingMode::Light.apply(&config, &mut body).unwrap();
        assert_eq!(body["reasoning_effort"], "low");
        ThinkingMode::Balanced.apply(&config, &mut body).unwrap();
        assert_eq!(body["reasoning_effort"], "medium");
        ThinkingMode::Thinking.apply(&config, &mut body).unwrap();
        assert_eq!(body["reasoning_effort"], "high");
        config.provider = "deepseek".into();
        config.name = "deepseek-v4-pro".into();
        let mut body = serde_json::json!({"max_tokens":128});
        ThinkingMode::Maximum.apply(&config, &mut body).unwrap();
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["reasoning_effort"], "max");
        assert!(ThinkingMode::Balanced.validate(&config).is_err());
    }

    #[test]
    fn actual_provider_payloads() {
        let mut config = ModelConfig::default();
        config.provider = "deepseek".into();
        let mut body = serde_json::json!({"max_tokens":128,"temperature":0.7});
        ThinkingMode::Thinking.apply(&config, &mut body).unwrap();
        assert_eq!(body["thinking"]["type"], "enabled");
        ThinkingMode::Standard.apply(&config, &mut body).unwrap();
        assert_eq!(body["thinking"]["type"], "disabled");
        config.provider = "openai".into();
        config.name = "gpt-5.4".into();
        ThinkingMode::Thinking.apply(&config, &mut body).unwrap();
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["max_completion_tokens"], 128);
        config.name = "gpt-4o".into();
        assert!(ThinkingMode::Thinking.validate(&config).is_err());
    }
}
