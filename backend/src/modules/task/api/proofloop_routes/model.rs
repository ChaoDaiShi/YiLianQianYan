use crate::{
    api::chat::runtime::resolve_scoped_model_source, app::state::AppServer,
    integrations::llm::client::LlmClient, modules::task::proofloop::contracts::ProofError,
};

pub(super) fn planning_client(
    server: &AppServer,
    workspace: &str,
    correlation: &str,
    usage_kind: &str,
    max_timeout_ms: u64,
) -> Result<LlmClient, ProofError> {
    let default = server.config.read().model.clone();
    let source = resolve_scoped_model_source(&server.db, &default, None, Some(workspace))
        .map_err(unavailable)?;
    source
        .into_client(
            &server.db,
            &default,
            server.secret_resolver.clone(),
            correlation,
            usage_kind,
            Some(max_timeout_ms),
        )
        .map(LlmClient::with_one_pool_attempt)
        .map_err(unavailable)
}

fn unavailable(reason: String) -> ProofError {
    ProofError::new("proofloop_model_unavailable", reason)
}
