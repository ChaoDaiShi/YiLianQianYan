use serde::{Deserialize, Serialize};

pub const SHARED_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationMetadata {
    pub simulated: bool,
    pub provider: String,
    pub reason: String,
}

impl SimulationMetadata {
    pub fn mock(reason: impl Into<String>) -> Self {
        Self {
            simulated: true,
            provider: "mock".to_string(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_metadata_never_looks_like_real_execution() {
        let metadata = SimulationMetadata::mock("desktop-foundation");
        assert!(metadata.simulated);
        assert_eq!(metadata.provider, "mock");
        assert_eq!(metadata.reason, "desktop-foundation");
    }
}
