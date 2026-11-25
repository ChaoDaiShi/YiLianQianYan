use super::*;

#[test]
fn wechat_conversation_requires_content_permission_without_ocr_or_actions() {
    let (_, runtime, host, registration) = setup(false);
    let session = attached(&runtime, &host, &registration);
    let read = || runtime.wechat_conversation(&session.target());
    assert_eq!(read().unwrap_err().code, "wechat_required");
    runtime
        .mutate(|state| {
            let config = &mut state
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config;
            config.adapter = "wechat".into();
            config.allow_observe = true;
            Ok(())
        })
        .unwrap();
    assert_eq!(read().unwrap_err().code, "software_content_not_authorized");
    runtime
        .mutate(|state| {
            state
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config
                .allow_content = true;
            Ok(())
        })
        .unwrap();
    assert_eq!(read().unwrap_err().code, "wechat_conversation_unavailable");
}

#[test]
fn window_ocr_requires_a_separate_grant_and_a_visible_window() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    let read = || runtime.recognize_window(&session.target());
    assert_eq!(read().unwrap_err().code, "window_ocr_not_authorized");
    runtime
        .mutate(|state| {
            let config = &mut state
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config;
            config.allow_window_ocr = true;
            config.allow_actions = false;
            Ok(())
        })
        .unwrap();
    let (text, truncated) = read().unwrap();
    assert_eq!(text.chars().count(), 4096);
    assert!(truncated);
    host.hidden.store(true, Ordering::Release);
    assert_eq!(read().unwrap_err().code, "window_not_visible");
    host.hidden.store(false, Ordering::Release);
    runtime
        .db
        .conn()
        .execute(
            "UPDATE local_onboarding SET profile=?1",
            [r#"{"permissions":{"screen":false}}"#],
        )
        .unwrap();
    assert_eq!(read().unwrap_err().code, "screen_capture_disabled");
}

#[test]
fn wechat_frame_requires_current_visible_observation_but_no_action_grant() {
    let (_, runtime, host, registration) = setup(false);
    let session = attached(&runtime, &host, &registration);
    let capture = || runtime.wechat_frame(session.target());
    assert_eq!(capture().unwrap_err().code, "wechat_required");
    // Inject the adapter in the controlled registry. The test binary itself is not Weixin.exe.
    runtime
        .mutate(|state| {
            state
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config
                .adapter = "wechat".into();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        capture().unwrap_err().code,
        "software_observation_not_authorized"
    );
    runtime
        .mutate(|state| {
            state
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config
                .allow_observe = true;
            Ok(())
        })
        .unwrap();
    assert_eq!(capture().unwrap(), b"fixture frame");
    host.hidden.store(true, Ordering::Release);
    assert_eq!(capture().unwrap_err().code, "wechat_not_visible");
    host.hidden.store(false, Ordering::Release);
    let mut stale = session.target();
    stale.generation += 1;
    assert!(runtime.wechat_frame(stale).is_err());
    let stopped = runtime.stop(session.target()).unwrap();
    assert!(runtime.wechat_frame(stopped.target()).is_err());
    assert_eq!(host.actions.load(Ordering::Acquire), 0);
}

#[test]
fn software_capability_evidence_uses_current_window_and_observed_operations() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    runtime
        .mutate(|state| {
            for i in 0..201 {
                let id = format!("other-session-operation-{i}");
                state.operations.insert(
                    id.clone(),
                    crate::software::store::Receipt {
                        fingerprint: id.clone(),
                        record: OperationRecord {
                            operation_id: id,
                            action: "control".into(),
                            session_id: Some("different-session".into()),
                            registration_id: registration.id.clone(),
                            state: "verified".into(),
                            reason: None,
                            created_at: i64::MAX - i,
                            window_evidence: None,
                        },
                    },
                );
            }
            Ok(())
        })
        .unwrap();
    let evidence = || {
        runtime
            .capability_evidence(
                &[registration.clone()],
                &runtime.refresh_sessions().unwrap(),
                &[],
            )
            .into_iter()
            .find(|value| value.session_id.as_deref() == Some(session.id.as_str()))
            .unwrap()
    };
    let state = |value: &SoftwareCapabilityEvidence, dimension: &str| {
        value
            .entries
            .iter()
            .find(|row| row.dimension == dimension)
            .unwrap()
            .status
    };
    let attached_proof = evidence();
    assert_eq!(runtime.history().unwrap().len(), 200);
    assert_eq!(attached_proof.entries.len(), 6);
    assert_eq!(state(&attached_proof, "discover_launch"), "verified");
    assert_eq!(state(&attached_proof, "display"), "verified");
    assert_eq!(state(&attached_proof, "perceive"), "unverified");
    assert_eq!(state(&attached_proof, "operate"), "unverified");
    assert_eq!(state(&attached_proof, "background_run"), "unverified");

    runtime.observe(session.target()).unwrap();
    assert_eq!(state(&evidence(), "perceive"), "verified");
    host.hidden.store(true, Ordering::Release);
    let hidden = evidence();
    assert_eq!(state(&hidden, "display"), "unavailable");
    assert_eq!(state(&hidden, "background_run"), "verified");
    assert_eq!(
        state(&hidden, "perceive"),
        "unverified",
        "old foreground evidence is not current"
    );
    assert_eq!(state(&hidden, "background_interaction"), "unverified");

    let observation = runtime.observe(session.target()).unwrap();
    assert_eq!(state(&evidence(), "background_interaction"), "partial");
    let controlled = runtime
        .control(
            session.target(),
            &observation.id,
            &observation.controls[0].id,
            "invoke",
            None,
            &op(),
        )
        .unwrap();
    let after = evidence();
    assert_eq!(after.generation, Some(controlled.generation));
    assert_eq!(state(&after, "operate"), "verified");
    assert_eq!(state(&after, "background_interaction"), "verified");

    host.hidden.store(false, Ordering::Release);
    let changed_window = evidence();
    assert_eq!(state(&changed_window, "operate"), "unverified");
    assert_eq!(
        state(&changed_window, "background_interaction"),
        "unverified"
    );
}
