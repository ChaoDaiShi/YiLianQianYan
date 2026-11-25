use serde::{Deserialize, Serialize};

use super::thinking::{supports_openai_effort, ThinkingMode};
use crate::config::types::ModelConfig;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GenerationOverrides {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_output_tokens: Option<u32>,
}

impl GenerationOverrides {
    pub fn validate(self) -> Result<(), String> {
        if self
            .temperature
            .is_some_and(|value| !(0.0..=2.0).contains(&value))
        {
            return Err("温度须在 0 到 2 之间".into());
        }
        if self
            .top_p
            .is_some_and(|value| !(0.0..=1.0).contains(&value) || value == 0.0)
        {
            return Err("采样范围须大于 0 且不超过 1".into());
        }
        if self.temperature.is_some() && self.top_p.is_some() {
            return Err("温度与采样范围请选择一项覆盖".into());
        }
        if self
            .max_output_tokens
            .is_some_and(|value| !(1..=1_000_000).contains(&value))
        {
            return Err("单次输出上限须在 1 到 1000000 之间".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sampling_capabilities_follow_the_model_and_thinking_mode() {
        let mut model = ModelConfig {
            provider: "deepseek".into(),
            name: "deepseek-flash".into(),
            ..ModelConfig::default()
        };
        let standard = GenerationOptions::for_model(&model, ThinkingMode::Standard);
        assert!(standard.temperature);
        assert!(!standard.top_p);
        model.name = "deepseek-v4-pro".into();
        assert!(GenerationOptions::for_model(&model, ThinkingMode::Standard).top_p);
        let reasoning = GenerationOptions::for_model(&model, ThinkingMode::Thinking);
        assert!(!reasoning.temperature);
        assert!(!reasoning.top_p);

        let mut body = json!({ "temperature": 0.25 });
        let override_p = GenerationOverrides {
            top_p: Some(0.9),
            max_output_tokens: Some(100),
            ..Default::default()
        };
        override_p.apply_sampling(
            GenerationOptions::for_model(&model, ThinkingMode::Standard),
            &mut body,
        );
        assert_eq!(body, json!({ "top_p": 0.9 }));
        assert_eq!(override_p.output_limit(reasoning, 80), 80);

        let unknown = GenerationOptions::for_model(
            &ModelConfig {
                provider: "custom".into(),
                ..model
            },
            ThinkingMode::Standard,
        );
        override_p.apply_sampling(unknown, &mut body);
        assert!(!unknown.temperature && !unknown.top_p && unknown.max_output_tokens.is_none());
        assert_eq!(override_p.output_limit(unknown, 200), 200);
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct GenerationOptions {
    pub temperature: bool,
    pub top_p: bool,
    pub max_output_tokens: Option<u32>,
    pub default_temperature: Option<f64>,
}

impl GenerationOptions {
    pub fn for_model(model: &ModelConfig, mode: ThinkingMode) -> Self {
        let known_openai_chat = model.provider == "openai"
            && [
                "gpt-4o",
                "gpt-4o-mini",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
            ]
            .iter()
            .any(|name| {
                model.name == *name
                    || model.name.strip_prefix(*name).is_some_and(|suffix| {
                        suffix.len() == 11
                            && suffix.starts_with('-')
                            && suffix[1..].chars().all(|c| c.is_ascii_digit() || c == '-')
                    })
            });
        let known_deepseek_chat = model.provider == "deepseek"
            && matches!(
                model.name.as_str(),
                "deepseek-chat" | "deepseek-flash" | "deepseek-v4-flash" | "deepseek-v4-pro"
            );
        let deepseek_standard = known_deepseek_chat && mode == ThinkingMode::Standard;
        let temperature = known_openai_chat || deepseek_standard;
        let top_p = known_openai_chat
            || (deepseek_standard
                && matches!(model.name.as_str(), "deepseek-chat" | "deepseek-v4-pro"));
        Self {
            temperature,
            top_p,
            max_output_tokens: (known_openai_chat
                || known_deepseek_chat
                || supports_openai_effort(model)
                || (model.provider == "anthropic" && model.name.starts_with("claude-")))
            .then_some(model.max_tokens),
            default_temperature: known_openai_chat.then_some(model.temperature),
        }
    }

    pub fn intersect(&mut self, other: Self) {
        self.temperature &= other.temperature;
        self.top_p &= other.top_p;
        self.max_output_tokens = self
            .max_output_tokens
            .zip(other.max_output_tokens)
            .map(|(left, right)| left.min(right));
        if self.default_temperature != other.default_temperature {
            self.default_temperature = None;
        }
    }
}

impl GenerationOverrides {
    pub fn output_limit(self, options: GenerationOptions, current: u32) -> u32 {
        if options.max_output_tokens.is_some() {
            self.max_output_tokens
                .map_or(current, |requested| current.min(requested))
        } else {
            current
        }
    }

    pub fn apply_sampling(self, options: GenerationOptions, body: &mut serde_json::Value) {
        if let Some(temperature) = self.temperature.filter(|_| options.temperature) {
            body["temperature"] = serde_json::json!(temperature);
        }
        if let Some(top_p) = self.top_p.filter(|_| options.top_p) {
            body.as_object_mut().unwrap().remove("temperature");
            body["top_p"] = serde_json::json!(top_p);
        }
    }
}
