use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    LocalUser,
    WindowsAccount,
    OrganizationUser,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInRole {
    Owner,
    Standard,
    Restricted,
}

impl BuiltInRole {
    pub const ALL: [Self; 3] = [Self::Owner, Self::Standard, Self::Restricted];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Standard => "standard",
            Self::Restricted => "restricted",
        }
    }
}

impl std::fmt::Display for BuiltInRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecuritySubject {
    pub subject_id: String,
    pub subject_type: SubjectType,
    pub provider: String,
    pub external_ref: Option<String>,
}

impl SecuritySubject {
    pub fn local_user() -> Self {
        Self {
            subject_id: "local-user".to_string(),
            subject_type: SubjectType::LocalUser,
            provider: "local".to_string(),
            external_ref: None,
        }
    }
}
