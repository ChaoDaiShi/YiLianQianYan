use super::*;
use crate::modules::{
    automation::model::Schedule,
    task::proofloop::contracts::{AttentionGoalSource, ProofDraft},
};

pub(super) fn existing_draft(
    server: &AppServer,
    source: &AttentionGoalSource,
    workspace: Option<&str>,
) -> Result<Option<ProofDraft>, ProofError> {
    let id = source.draft_id()?;
    let invalid = || ProofError::new("invalid_attention_source", "关注提议与观察或工作空间不匹配");
    let unavailable = |reason| ProofError::new("proofloop_observation_unavailable", reason);
    let watch = server
        .db
        .get_automation(&source.automation_id)
        .map_err(unavailable)?
        .filter(|item| matches!(item.schedule, Schedule::Watch { .. }))
        .ok_or_else(invalid)?;
    if workspace.is_some_and(|workspace| workspace != watch.workspace_id) {
        return Err(invalid());
    }
    server
        .db
        .attention_observation(&watch.id, &source.observation_id)
        .map_err(unavailable)?
        .filter(|item| item.workspace_id == watch.workspace_id)
        .ok_or_else(invalid)?;
    if let Some(draft) = DraftService::new(server.db.clone()).get_draft(&id)? {
        return if draft.source.as_ref() == Some(source)
            && draft.contract.workspace_id == watch.workspace_id
        {
            Ok(Some(draft))
        } else {
            Err(invalid())
        };
    }
    if watch
        .attention
        .and_then(|state| state.proposal)
        .is_some_and(|proposal| proposal.observation_id == source.observation_id)
    {
        Ok(None)
    } else {
        Err(invalid())
    }
}

pub(super) async fn lookup(
    State(server): State<Arc<AppServer>>,
    Path((automation_id, observation_id)): Path<(String, String)>,
) -> Response {
    match existing_draft(
        &server,
        &AttentionGoalSource {
            automation_id,
            observation_id,
        },
        None,
    ) {
        Ok(draft) => Json(json!({"draft":draft})).into_response(),
        Err(error) => proof_error_response(error),
    }
}
