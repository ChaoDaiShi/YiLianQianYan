use thiserror::Error;

pub const CONTROL_SESSION_HEADER: &str = "x-yilian-control-session";
pub const CONTROL_SESSION_ENV: &str = "YILIAN_CONTROL_SESSION_TOKEN";
const MIN_TOKEN_LENGTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlSession {
    token: String,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ControlSessionError {
    #[error("control session token is missing")]
    Missing,
    #[error("control session token is invalid")]
    Invalid,
    #[error("control session token must contain at least 32 non-whitespace characters")]
    TooShort,
}

impl ControlSession {
    pub fn new(token: impl Into<String>) -> Result<Self, ControlSessionError> {
        let token = token.into();
        if token.trim().chars().count() < MIN_TOKEN_LENGTH {
            return Err(ControlSessionError::TooShort);
        }
        Ok(Self { token })
    }

    pub fn generate() -> Self {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        Self { token }
    }

    pub fn from_environment_or_generate() -> Result<Self, ControlSessionError> {
        match std::env::var(CONTROL_SESSION_ENV) {
            Ok(token) => Self::new(token),
            Err(std::env::VarError::NotPresent) => Ok(Self::generate()),
            Err(std::env::VarError::NotUnicode(_)) => Err(ControlSessionError::Invalid),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn verify(&self, candidate: Option<&str>) -> Result<(), ControlSessionError> {
        let candidate = candidate.ok_or(ControlSessionError::Missing)?;
        let expected = self.token.as_bytes();
        let provided = candidate.as_bytes();

        if expected.len() != provided.len() {
            return Err(ControlSessionError::Invalid);
        }

        let difference = expected
            .iter()
            .zip(provided)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            });
        if difference == 0 {
            Ok(())
        } else {
            Err(ControlSessionError::Invalid)
        }
    }
}
