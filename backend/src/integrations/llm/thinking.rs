//! Explicit provider thinking controls. Unknown protocols are rejected rather than ignored.
use crate::config::types::ModelConfig;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    #[default]
    Standard,
    Thinking,
}
impl ThinkingMode {
    pub fn validate(self, config: &ModelConfig) -> Result<(), String> {
        if self == Self::Thinking && !supports_thinking(config) {
            return Err(format!(
                "模型 {} 尚未支持深度思考协议，请选择标准模式或支持的模型",
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
        if config.provider == "deepseek" {
            body["thinking"] =
                serde_json::json!({"type":if self == Self::Thinking {"enabled"} else {"disabled"}});
            body.as_object_mut().unwrap().remove("temperature");
        } else if supports_thinking(config) {
            if self == Self::Thinking {
                body["reasoning_effort"] = serde_json::json!("high");
            }
            body.as_object_mut().unwrap().remove("temperature");
            if let Some(tokens) = body.as_object_mut().unwrap().remove("max_tokens") {
                body["max_completion_tokens"] = tokens;
            }
        }
        Ok(())
    }
}
pub fn supports_thinking(config: &ModelConfig) -> bool {
    config.provider == "deepseek"
        || (config.provider == "openai"
            && (config.name.starts_with("gpt-5")
                || ["o1", "o3", "o4"].iter().any(|name| {
                    config.name == *name || config.name.starts_with(&format!("{name}-"))
                })))
}
#[cfg(test)]
mod tests {
    use super::*;
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
