use crate::modules::task::TaskDecisionOption;
use crate::tools::trait_def::{Tool, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputRequest {
    pub title: String,
    pub kind: String,
    #[serde(default, deserialize_with = "read_options")]
    pub options: Vec<TaskDecisionOption>,
    #[serde(default)]
    pub allow_text: bool,
}
fn read_options<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<TaskDecisionOption>, D::Error> {
    #[derive(Deserialize)]
    struct OptionInput {
        id: String,
        label: String,
        #[serde(default)]
        description: String,
    }
    Ok(Vec::<OptionInput>::deserialize(deserializer)?
        .into_iter()
        .map(|v| TaskDecisionOption {
            id: v.id,
            label: v.label,
            description: v.description,
        })
        .collect())
}
impl UserInputRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() || self.title.chars().count() > 2000 {
            return Err("invalid_question_title".into());
        }
        match self.kind.as_str() {
            "text" if self.options.is_empty() => Ok(()),
            "single_choice" if (2..=6).contains(&self.options.len()) => {
                let mut ids = std::collections::HashSet::new();
                for option in &self.options {
                    if option.id.trim().is_empty()
                        || option.label.trim().is_empty()
                        || !ids.insert(&option.id)
                    {
                        return Err("invalid_question_options".into());
                    }
                }
                Ok(())
            }
            _ => Err("invalid_question_kind".into()),
        }
    }
}
pub struct AskUserTool;
#[async_trait::async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }
    fn description(&self) -> &str {
        "Ask for necessary missing input. Pauses this turn until the user answers. This is not permission approval. Use text or single_choice with 2-6 distinct options."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"title":{"type":"string"},"kind":{"type":"string","enum":["text","single_choice"]},"options":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"label":{"type":"string"},"description":{"type":"string"}},"required":["id","label"]}},"allow_text":{"type":"boolean"}},"required":["title","kind"]})
    }
    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::error("ask_user requires a conversation turn continuation")
    }
}
