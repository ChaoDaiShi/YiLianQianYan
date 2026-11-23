use super::*;

#[test]
fn software_capability_evidence_binds_closed_window_and_preserves_application_notifications() {
    let registration = Registration {
        id: "app".into(),
        revision: 1,
        config: RegistrationInput {
            name: "fixture".into(),
            adapter: "generic_fixture".into(),
            executable: "fixture.exe".into(),
            workspace_id: None,
            allow_observe: true,
            allow_content: false,
            allow_window_ocr: false,
            allow_actions: true,
        },
    };
    let session = |id: &str, pid, creation| Session {
        id: id.into(),
        registration_id: registration.id.clone(),
        generation: 2,
        state: "lost".into(),
        launched_by_host: false,
        last_error: None,
        window: WindowState {
            identity: WindowIdentity {
                hwnd: id.into(),
                pid,
                process_started_at: creation,
                executable: "fixture.exe".into(),
                class_name: "fixture-window".into(),
            },
            mode: "independent".into(),
            visible: false,
            focused: false,
            minimized: false,
            embedding_supported: false,
            reason: None,
        },
    };
    let first = session("session-a", 17, 100);
    let second = session("session-b", 17, 100);
    let mut monitor = MonitorStatus {
        registration_id: registration.id.clone(),
        session_id: first.id.clone(),
        target: first.window.identity.clone(),
        state: "waiting_window",
        notification_state: "watching",
        content_state: "checking",
        latest: None,
        latest_lifecycle: None,
        latest_notification: None,
        latest_content: None,
    };
    for (kind, id) in [
        (Change::WindowClosed, "window-closed"),
        (Change::NotificationReceived, "app-notification"),
    ] {
        monitor.record_alert(&Alert {
            id: id.into(),
            kind,
            observed_at: 1000,
            summary: kind.summary(),
            explanation: None,
            explaining: false,
        });
    }
    assert!(monitor
        .windowless_process_evidence(&session("session-a", 17, 200))
        .is_none());
    let monitors = [monitor];
    let first_proof =
        super::super::evidence::project(&registration, Some(&first), None, &[], &monitors, true);
    let second_proof =
        super::super::evidence::project(&registration, Some(&second), None, &[], &monitors, true);
    let expected_background = first_proof
        .entries
        .iter()
        .find(|item| item.dimension == "background_run")
        .unwrap();
    assert_eq!(
        expected_background.evidence_id.as_deref(),
        Some("window-closed")
    );
    assert_eq!(
        second_proof
            .entries
            .iter()
            .find(|item| item.dimension == "background_run")
            .unwrap()
            .status,
        "unverified"
    );
    for proof in [&first_proof, &second_proof] {
        let application = proof
            .entries
            .iter()
            .find(|item| item.dimension == "background_interaction")
            .unwrap();
        assert_eq!(application.status, "partial");
        assert_eq!(application.evidence_id.as_deref(), Some("app-notification"));
    }
}

#[test]
fn notification_snapshot_ignores_existing_and_removed_ids() {
    let mut ids = None;
    assert_eq!(notification_change(&mut ids, vec![1, 2]), None);
    assert_eq!(notification_change(&mut ids, vec![2]), None);
    assert_eq!(
        notification_change(&mut ids, vec![2, 3, 4]),
        Some(Change::NotificationReceived)
    );
    assert_eq!(notification_change(&mut ids, vec![4, 3, 2]), None);
}

#[test]
fn lifecycle_transitions_are_not_repeated_on_subsequent_polls() {
    assert_eq!(
        transition(Phase::Watching, &Sample::Present),
        (Phase::Watching, None)
    );
    assert_eq!(
        transition(Phase::Watching, &Sample::Closed),
        (Phase::Closed, Some(Change::WindowClosed))
    );
    assert_eq!(
        transition(Phase::Closed, &Sample::Closed),
        (Phase::Closed, None)
    );
    assert_eq!(
        transition(Phase::Closed, &Sample::Exited),
        (Phase::Exited, Some(Change::ProcessExited))
    );
    assert_eq!(
        transition(Phase::Exited, &Sample::Exited),
        (Phase::Exited, None)
    );
}
