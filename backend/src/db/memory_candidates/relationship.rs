use serde::{Deserialize, Serialize};

use crate::db::memory_governance::normalize_memory;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RelationshipKind {
    Person,
    Kinship,
    Event,
    Project,
    Concept,
    Preference,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RelationshipCertainty {
    Confirmed,
    #[default]
    Inferred,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryRelationship {
    pub kind: RelationshipKind,
    pub subject: String,
    pub label: String,
    pub object: String,
    pub directed: bool,
    #[serde(default)]
    pub certainty: RelationshipCertainty,
    #[serde(default)]
    pub occurred_at: String,
}

impl MemoryRelationship {
    pub(crate) fn normalized(mut self) -> Option<Self> {
        fn clean(value: &str, max: usize) -> Option<String> {
            let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
            (!value.is_empty() && value.chars().count() <= max).then_some(value)
        }
        self.subject = clean(&self.subject, 60)?;
        self.label = clean(&self.label, 40)?;
        self.object = clean(&self.object, 60)?;
        self.occurred_at = self.occurred_at.trim().to_owned();
        let valid_date = self.occurred_at.is_empty()
            || (self.occurred_at.len() == 10
                && chrono::NaiveDate::parse_from_str(&self.occurred_at, "%Y-%m-%d").is_ok());
        (valid_date && normalize_memory(&self.subject) != normalize_memory(&self.object))
            .then_some(self)
    }

    pub(crate) fn content(&self) -> String {
        let mut content = format!("{} · {} · {}", self.subject, self.label, self.object);
        if !self.occurred_at.is_empty() {
            content.push_str(&format!(" · {}", self.occurred_at));
        }
        if self.certainty == RelationshipCertainty::Inferred {
            content.push_str(" · 待确认");
        }
        content
    }

    pub(crate) fn identity(&self) -> serde_json::Value {
        let mut subject = normalize_memory(&self.subject);
        let mut object = normalize_memory(&self.object);
        if !self.directed && object < subject {
            std::mem::swap(&mut subject, &mut object);
        }
        serde_json::json!([
            self.kind,
            subject,
            normalize_memory(&self.label),
            object,
            self.directed,
            self.occurred_at
        ])
    }
}

pub(in crate::db) fn read_metadata(metadata: Option<&str>) -> Option<MemoryRelationship> {
    let mut value: serde_json::Value = serde_json::from_str(metadata?).ok()?;
    serde_json::from_value(value.get_mut("relationship")?.take()).ok()
}

pub(in crate::db) fn same_memory(
    wanted: Option<&MemoryRelationship>,
    content_matches: bool,
    actual: Option<&str>,
) -> bool {
    match (wanted, read_metadata(actual)) {
        (Some(wanted), Some(actual)) => {
            wanted.identity() == actual.identity() && wanted.certainty == actual.certainty
        }
        (None, None) => content_matches,
        _ => false,
    }
}
