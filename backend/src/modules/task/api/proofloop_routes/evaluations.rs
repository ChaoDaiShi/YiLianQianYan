//! Goal evaluation HTTP handlers.
use super::*;

fn acceptance_report_save_error(error: crate::db::ProofLoopDbError) -> Response {
    match error {
        crate::db::ProofLoopDbError::Integrity(message) => {
            proof_error_response(ProofError::new("proofloop_evidence_stale", message))
        }
        error => proof_error_response(ProofError::new("db_error", error.to_string())),
    }
}

pub async fn list_goal_evaluations_handler(
    State(server): State<Arc<AppServer>>,
    Path(goal_id): Path<String>,
) -> Response {
    match server.db.list_acceptance_reports(&goal_id) {
        Ok(reports) => (StatusCode::OK, Json(json!({ "reports": reports }))).into_response(),
        Err(err) => proof_error_response(ProofError::new("db_error", err.to_string())),
    }
}

pub async fn evaluate_goal_handler(
    State(server): State<Arc<AppServer>>,
    Path(goal_id): Path<String>,
    Json(request): Json<EvaluateGoalRequest>,
) -> Response {
    let now_ms = now();
    let contract = match server.db.load_goal_version(&goal_id, request.goal_version) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return proof_error_response(ProofError::new(
                "goal_not_found",
                format!("未找到目标 '{}' 版本 {}", goal_id, request.goal_version),
            ));
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };

    let draft = match server
        .db
        .confirmed_proof_draft_for_goal(&goal_id, request.goal_version)
    {
        Ok(Some(draft)) => draft,
        Ok(None) => {
            return proof_error_response(ProofError::new(
                "goal_graph_not_found",
                "目标尚未确认保存为任务图",
            ))
        }
        Err(error) => return proof_error_response(ProofError::new("db_error", error.to_string())),
    };
    let allowed_root = server.workspace_root.clone();
    let artifact_service =
        crate::modules::task::artifact::ArtifactService::new(server.db.clone_connection());
    let (snapshot, evidence_content) = match collect_evidence(
        &server.db,
        &artifact_service,
        &contract,
        &draft.graph_id,
        request.graph_revision,
        &request.execution_ids,
        &allowed_root,
        now_ms,
    ) {
        Ok(res) => res,
        Err(err) => return proof_error_response(err),
    };

    let existing_reviews = server
        .db
        .list_acceptance_reports(&goal_id)
        .map_err(|error| ProofError::new("db_error", error.to_string()));
    let existing_reviews = match existing_reviews {
        Ok(reports) => reports
            .into_iter()
            .find_map(|report| {
                if report.goal_version != contract.version
                    || report.graph_revision != snapshot.graph_revision
                {
                    return None;
                }
                server
                    .db
                    .get_acceptance_report(&report.report_id)
                    .ok()
                    .flatten()
                    .and_then(|(report, old)| {
                        if old.executions == snapshot.executions
                            && old.deliverables == snapshot.deliverables
                            && old.evidence == snapshot.evidence
                        {
                            Some(report.human_reviews)
                        } else {
                            None
                        }
                    })
            })
            .unwrap_or_default(),
        Err(error) => return proof_error_response(error),
    };
    let report = match evaluate_goal(
        &contract,
        &snapshot,
        &evidence_content,
        &existing_reviews,
        now_ms,
    ) {
        Ok(r) => r,
        Err(err) => return proof_error_response(err),
    };

    if let Err(err) = server
        .db
        .save_acceptance_report_if_current(&report, &snapshot, now_ms)
    {
        return acceptance_report_save_error(err);
    }

    (StatusCode::OK, Json(report)).into_response()
}

pub async fn submit_human_review_handler(
    State(server): State<Arc<AppServer>>,
    Path(evaluation_id): Path<String>,
    Json(request): Json<SubmitHumanReviewRequest>,
) -> Response {
    let now_ms = now();
    let (mut report, snapshot) = match server.db.get_acceptance_report(&evaluation_id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return proof_error_response(ProofError::new(
                "report_not_found",
                format!("未找到验收报告 '{}'", evaluation_id),
            ));
        }
        Err(err) => return proof_error_response(ProofError::new("db_error", err.to_string())),
    };

    let contract = match server
        .db
        .load_goal_version(&report.goal_id, report.goal_version)
    {
        Ok(Some(c)) => c,
        _ => {
            return proof_error_response(ProofError::new("goal_not_found", "未找到对应的目标契约"));
        }
    };

    if !matches!(request.decision.as_str(), "pass" | "fail")
        || !contract.criteria.iter().any(|criterion| {
            criterion.id == request.criterion_id
                && matches!(
                    criterion.check,
                    crate::modules::task::proofloop::CriterionCheck::HumanReview { .. }
                )
        })
    {
        return proof_error_response(ProofError::new(
            "invalid_human_review",
            "只能复核契约中的人工条件，并选择通过或不通过",
        ));
    }
    let artifact_service =
        crate::modules::task::artifact::ArtifactService::new(server.db.clone_connection());
    let content = match reload_snapshot_contents(
        &server.db,
        &artifact_service,
        &snapshot,
        &server.workspace_root,
    ) {
        Ok(content) => content,
        Err(error) => return proof_error_response(error),
    };
    let human_review = HumanReview {
        criterion_id: request.criterion_id.clone(),
        decision: request.decision,
        reason: request.reason,
        reviewer_subject: crate::safety::SecuritySubject::local_user().subject_id,
        reviewed_at_ms: now_ms,
    };

    report
        .human_reviews
        .retain(|r| r.criterion_id != request.criterion_id);
    report.human_reviews.push(human_review);

    let updated_report = match evaluate_goal(
        &contract,
        &snapshot,
        &content,
        &report.human_reviews,
        now_ms,
    ) {
        Ok(r) => r,
        Err(err) => return proof_error_response(err),
    };

    if let Err(err) =
        server
            .db
            .save_acceptance_report_if_current(&updated_report, &snapshot, now_ms)
    {
        return acceptance_report_save_error(err);
    }

    (StatusCode::OK, Json(updated_report)).into_response()
}
