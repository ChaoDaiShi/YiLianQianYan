use super::LlmClient;

impl LlmClient {
    /// Explicit caller contract, not a retry or provider fallback. Providers
    /// that reject JSON mode return their ordinary error to the caller.
    pub fn with_json_object_output(mut self) -> Self {
        self.set_json_object_output();
        self
    }

    pub(super) fn set_json_object_output(&mut self) {
        self.json_object_output = true;
        if let Some(pool) = &mut self.pool {
            for candidate in &mut pool.candidates {
                candidate.client.set_json_object_output();
            }
        }
    }

    pub(super) fn apply_response_format(&self, request: &mut serde_json::Value) {
        if self.json_object_output {
            request["response_format"] = serde_json::json!({"type":"json_object"});
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::types::ModelConfig,
        integrations::secret::{InMemorySecretStore, SecretResolver},
    };
    use std::sync::Arc;

    #[test]
    fn json_output_is_explicit_and_does_not_change_ordinary_client_requests() {
        let resolver = Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new())));
        let ordinary = LlmClient::new(&ModelConfig::default(), resolver.clone());
        let mut request = serde_json::json!({"model":"fixture"});
        ordinary.apply_response_format(&mut request);
        assert!(request.get("response_format").is_none());
        let json = LlmClient::new(&ModelConfig::default(), resolver).with_json_object_output();
        json.apply_response_format(&mut request);
        assert_eq!(
            request["response_format"],
            serde_json::json!({"type":"json_object"})
        );
    }
}
