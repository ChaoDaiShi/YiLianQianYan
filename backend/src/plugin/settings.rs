//! Declarative plugin-owned forms. Values are stored separately from the package.
use super::manifest::{ManifestError, PluginId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSetting {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub fields: Vec<PluginSettingField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingField {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub default: Value,
    #[serde(default)]
    pub options: Vec<String>,
}

impl PluginSettingField {
    pub fn accepts(&self, value: &Value) -> bool {
        match self.kind.as_str() {
            "text" => value.as_str().is_some_and(|s| s.len() <= 16_384),
            "number" => value.is_number(),
            "toggle" => value.is_boolean(),
            "select" => value
                .as_str()
                .is_some_and(|s| self.options.iter().any(|v| v == s)),
            _ => false,
        }
    }
}

impl PluginSetting {
    pub fn defaults(&self) -> BTreeMap<String, Value> {
        self.fields
            .iter()
            .map(|f| (f.key.clone(), f.default.clone()))
            .collect()
    }
    pub fn validate_values(&self, values: &BTreeMap<String, Value>) -> Result<(), String> {
        if values.len() != self.fields.len()
            || self
                .fields
                .iter()
                .any(|f| !values.get(&f.key).is_some_and(|v| f.accepts(v)))
        {
            return Err("插件设置字段或类型不匹配".into());
        }
        Ok(())
    }
}

pub fn validate(settings: &[PluginSetting]) -> Result<(), ManifestError> {
    let mut ids = HashSet::new();
    if settings.len() > 100 {
        return Err(ManifestError::TooManyContributions);
    }
    for setting in settings {
        PluginId::new(&setting.id)?;
        let mut keys = HashSet::new();
        if !ids.insert(&setting.id) || setting.title.trim().is_empty() || setting.fields.len() > 100
        {
            return Err(ManifestError::InvalidJson);
        }
        for field in &setting.fields {
            PluginId::new(&field.key)?;
            if !keys.insert(&field.key)
                || field.label.trim().is_empty()
                || !field.accepts(&field.default)
            {
                return Err(ManifestError::InvalidJson);
            }
        }
    }
    Ok(())
}
