use super::*;

#[tokio::test]
async fn personal_assistant_management_start_is_idempotent_and_archive_restore_stays_paused() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let (service, calls) = model(&f, 5).await;
    let request = management_request(&a);
    let (first, duplicate) = tokio::join!(
        super::super::control::start(&f.server, &a.id, request.clone()),
        super::super::control::start(&f.server, &a.id, request.clone())
    );
    assert_eq!(first.unwrap().id, duplicate.unwrap().id);
    wait_status(&f, &a, "completed").await;
    let replay = super::super::control::start(&f.server, &a.id, request.clone())
        .await
        .unwrap();
    assert_eq!(replay.status, "completed");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut changed = request;
    changed.acknowledged = true;
    assert!(super::super::control::start(&f.server, &a.id, changed)
        .await
        .is_err());
    let archived = super::super::control::archive(&f.server, &a.id)
        .await
        .unwrap();
    assert!(archived.archived);
    assert!(!archived.enabled);
    assert_eq!(
        super::super::control::archive(&f.server, &a.id)
            .await
            .unwrap()
            .revision,
        archived.revision
    );
    assert!(super::super::super::run_now(&f.server, &a.id)
        .await
        .is_err());
    let restored = super::super::control::restore(&f.server, &a.id, archived.revision).unwrap();
    assert!(!restored.archived);
    assert!(!restored.enabled);
    assert!(restored.next_run_at.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(attempts(&f.server, &a.id).unwrap().len(), 1);
    service.abort();
}

#[tokio::test]
async fn personal_assistant_management_recovery_requires_exact_review_and_no_automatic_model_call()
{
    let f = Fixture::new();
    let a = f.save(false, 5);
    let mut interrupted = claim(&f, &a);
    interrupted.status = "interrupted".into();
    f.server
        .db
        .update_automation_occurrence(&interrupted)
        .unwrap();
    let (service, calls) = model(&f, 5).await;
    let mut request = management_request(&a);
    assert!(
        super::super::control::start(&f.server, &a.id, request.clone())
            .await
            .is_err()
    );
    request.acknowledged = true;
    request.reviewed_occurrence_id = Some("different-old-round".into());
    assert!(
        super::super::control::start(&f.server, &a.id, request.clone())
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    request.reviewed_occurrence_id = Some(interrupted.id.clone());
    super::super::control::start(&f.server, &a.id, request)
        .await
        .unwrap();
    wait_status(&f, &a, "completed").await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(attempts(&f.server, &a.id).unwrap().len(), 2);
    assert_eq!(
        attempts(&f.server, &a.id)
            .unwrap()
            .iter()
            .find(|v| v.id == interrupted.id)
            .unwrap()
            .status,
        "interrupted"
    );
    service.abort();
}

#[tokio::test]
async fn personal_assistant_same_workspace_root_change_does_not_expand_saved_authority() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let mut workspace = super::super::super::workspace(&f.server, &a.workspace_id).unwrap();
    let new_root = f.root.join("different-directory");
    std::fs::create_dir_all(&new_root).unwrap();
    workspace.root_path = Some(new_root.to_string_lossy().into_owned());
    f.server.db.update_workspace(&workspace).unwrap();
    let paused = super::super::super::set_enabled(&f.server, &a.id, false, a.revision).unwrap();
    let resumed =
        super::super::super::set_enabled(&f.server, &a.id, true, paused.revision).unwrap();
    match (a.target.as_ref().unwrap(), resumed.target.as_ref().unwrap()) {
        (
            AutomationTarget::PersonalAssistant { root_path: old, .. },
            AutomationTarget::PersonalAssistant { root_path: new, .. },
        ) => assert_eq!(old, new),
        _ => panic!("wrong target"),
    }
}

#[tokio::test]
async fn personal_assistant_management_http_rejects_legacy_resume_forks_goal_override_and_bound_delete(
) {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let item = claim(&f, &a);
    mutate(&f.server, &item.id, |v| v.status = "interrupted".into()).unwrap();
    let (service, calls) = model(&f, 5).await;
    let (status, _) = management_http(
        &f,
        Method::POST,
        format!("/api/automations/{}/run", a.id),
        json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    for (path, body) in [
        (
            format!("/api/conversations/{}/branches", conversation(&a)),
            json!({"parent_node_id":"any-answer"}),
        ),
        (
            "/api/message-canvas/branches".into(),
            json!({"conversation_id":conversation(&a),"parent_node_id":"any-answer","message":"try a branch"}),
        ),
    ] {
        let (status, _) = management_http(&f, Method::POST, path, body).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }
    let (status, _) = management_http(
        &f,
        Method::PUT,
        format!("/api/conversations/{}/goal", conversation(&a)),
        json!({"graph_id":"another-goal"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    let (status,_) = management_http(&f,Method::POST,"/api/chat".into(),json!({"conversation_id":conversation(&a),"message":"hello","goal_ref":{"goal_id":"other-goal","goal_version":1}})).await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    let (status, _) = management_http(
        &f,
        Method::DELETE,
        format!("/api/conversations/{}", conversation(&a)),
        json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(f.server.db.get_conversation(conversation(&a)).is_ok());
    assert_eq!(
        f.server
            .db
            .conn()
            .query_row("SELECT count(*) FROM conversations", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    service.abort();
}

#[tokio::test]
async fn personal_assistant_management_http_distinguishes_rejected_request_from_accepted_uuid() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let (service, calls) = model(&f, 5).await;
    let mut request = management_request(&a);
    request.expected_revision += 1;
    let (status, body) = management_http(
        &f,
        Method::POST,
        format!("/api/automations/{}/personal/start", a.id),
        serde_json::to_value(&request).unwrap(),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(body["definitive_rejected"], true);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(attempts(&f.server, &a.id).unwrap().is_empty());
    request.expected_revision = a.revision;
    super::super::control::start(&f.server, &a.id, request.clone())
        .await
        .unwrap();
    wait_status(&f, &a, "completed").await;
    request.acknowledged = true;
    let (status, body) = management_http(
        &f,
        Method::POST,
        format!("/api/automations/{}/personal/start", a.id),
        serde_json::to_value(&request).unwrap(),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["definitive_rejected"], false);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    service.abort();
}

#[test]
fn personal_assistant_configuration_save_cannot_race_an_already_claimed_turn() {
    let f = Fixture::new();
    let a = f.save(false, 5);
    let _item = claim(&f, &a);
    let mut changed = a.clone();
    changed.name = "changed while dispatching".into();
    changed.revision += 1;
    assert!(f.server.db.save_automation(&changed, false).is_err());
    let paused = super::super::super::set_enabled(&f.server, &a.id, false, a.revision).unwrap();
    assert!(!paused.enabled);
    assert_eq!(
        f.server.db.get_automation(&a.id).unwrap().unwrap().name,
        a.name
    );
}
