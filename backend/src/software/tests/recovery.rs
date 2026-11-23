use super::*;

#[test]
fn failed_receipt_commit_blocks_new_actions_and_keeps_restoration_available() {
    let (db, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    let observation = runtime.observe(session.target()).unwrap();
    let entered = Arc::new(Barrier::new(2));
    let exit = Arc::new(Barrier::new(2));
    *host.gates.lock() = Some((entered.clone(), exit.clone()));
    let worker_runtime = runtime.clone();
    let target = session.target();
    let worker = std::thread::spawn(move || {
        worker_runtime.control(
            target,
            &observation.id,
            &observation.controls[0].id,
            "invoke",
            None,
            &op(),
        )
    });
    entered.wait();
    // The effect already occurred in the controlled host. Only completion
    // writes now fail, not admission or the original side effect.
    db.conn().execute_batch("CREATE TRIGGER fail_software_receipt BEFORE UPDATE OF value ON settings WHEN NEW.key='software_mount.inventory.v1' BEGIN SELECT RAISE(FAIL,'injected receipt commit failure'); END;").unwrap();
    exit.wait();
    assert_eq!(worker.join().unwrap().unwrap_err().code, "result_unknown");
    let current = runtime.sessions().unwrap().pop().unwrap();
    assert_eq!(current.state, "result_unknown");
    assert_eq!(
        runtime
            .ensure_ai_allowed(&current.target(), "control")
            .unwrap_err()
            .code,
        "result_unknown"
    );
    assert!(runtime
        .control(current.target(), "old", "button", "invoke", None, &op())
        .is_err());
    assert_eq!(host.actions.load(Ordering::Acquire), 1);
    // Storage failure must not trap the externally owned native window.
    let released = runtime.release(current.target(), &op()).unwrap();
    assert_eq!(released.state, "released");
    assert!(released
        .last_error
        .as_deref()
        .unwrap()
        .contains("could not be persisted"));
    assert_eq!(host.releases.load(Ordering::Acquire), 1);
    db.conn()
        .execute_batch("DROP TRIGGER fail_software_receipt;")
        .unwrap();
}

#[test]
fn archived_project_blocks_ai_but_cannot_trap_attached_windows() {
    let (db, runtime, host, _) = setup(true);
    db.conn().execute("INSERT INTO workspaces(id,name,description,root_path,status,created_at,updated_at) VALUES('software-project','Fixture','','','active',1,1)", []).unwrap();
    let registration = runtime
        .register(RegistrationInput {
            name: "Project fixture".into(),
            adapter: "generic_fixture".into(),
            executable: host.identity.executable.clone(),
            workspace_id: Some("software-project".into()),
            allow_observe: true,
            allow_content: false,
            allow_window_ocr: false,
            allow_actions: true,
        })
        .unwrap();
    let session = attached(&runtime, &host, &registration);
    let embedded = runtime
        .present(
            session.target(),
            "embedded",
            Some(Viewport {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
            }),
            &op(),
        )
        .unwrap();
    db.conn()
        .execute(
            "UPDATE workspaces SET status='archived' WHERE id='software-project'",
            [],
        )
        .unwrap();
    assert_eq!(
        runtime.observe(embedded.target()).unwrap_err().code,
        "software_workspace_unavailable"
    );
    assert!(runtime
        .present(
            embedded.target(),
            "embedded",
            Some(Viewport {
                x: 0,
                y: 0,
                width: 640,
                height: 480
            }),
            &op()
        )
        .is_err());
    let stopped = runtime.stop(embedded.target()).unwrap();
    assert_eq!(stopped.state, "stopped");
    let independent = runtime
        .present(stopped.target(), "independent", None, &op())
        .unwrap();
    let released = runtime.release(independent.target(), &op()).unwrap();
    assert_eq!(released.state, "released");
    assert_eq!(host.releases.load(Ordering::Acquire), 1);
}

#[test]
fn launch_confirmation_is_bound_to_the_displayed_revision_and_executable() {
    let (_, runtime, host, registration) = setup(false);
    host.candidates.lock().clear();
    let mut changed = registration.config.clone();
    changed.name = "Changed after the launch dialog opened".into();
    let revised = runtime
        .update(&registration.id, registration.revision, changed)
        .unwrap();
    assert_eq!(
        runtime
            .launch_confirmed(
                &registration.id,
                registration.revision,
                &registration.config.executable,
                &op()
            )
            .unwrap_err()
            .code,
        "stale_software_launch"
    );
    assert_eq!(
        runtime
            .launch_confirmed(&revised.id, revised.revision, "different.exe", &op())
            .unwrap_err()
            .code,
        "stale_software_launch"
    );
    assert_eq!(host.launches.load(Ordering::Acquire), 0);
    runtime
        .launch_confirmed(
            &revised.id,
            revised.revision,
            &revised.config.executable,
            &op(),
        )
        .unwrap();
    assert_eq!(host.launches.load(Ordering::Acquire), 1);
}

#[test]
fn unknown_launch_cannot_be_repeated_under_a_fresh_operation_id() {
    let (_, runtime, host, registration) = setup(false);
    host.candidates.lock().clear();
    host.unknown_launch.store(true, Ordering::Release);
    let original = op();
    assert_eq!(
        runtime
            .launch(&registration.id, &original)
            .unwrap_err()
            .code,
        "result_unknown"
    );
    let blocked = op();
    assert_eq!(
        runtime.launch(&registration.id, &blocked).unwrap_err().code,
        "software_launch_blocked"
    );
    assert_eq!(
        runtime
            .history()
            .unwrap()
            .iter()
            .find(|receipt| receipt.operation_id == blocked)
            .unwrap()
            .state,
        "failed"
    );
    assert_eq!(host.launches.load(Ordering::Acquire), 1);
    runtime
        .acknowledge_launch(&registration.id, &original)
        .unwrap();
    assert_eq!(host.launches.load(Ordering::Acquire), 1);
    assert_eq!(
        runtime
            .launch(&registration.id, &original)
            .unwrap_err()
            .code,
        "acknowledged_unknown"
    );
    assert_eq!(host.launches.load(Ordering::Acquire), 1);
    host.unknown_launch.store(false, Ordering::Release);
    runtime.launch(&registration.id, &op()).unwrap();
    assert_eq!(host.launches.load(Ordering::Acquire), 2);
}

#[test]
fn shrinking_permissions_does_not_clear_an_unknown_result_barrier() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    let observation = runtime.observe(session.target()).unwrap();
    host.unknown.store(true, Ordering::Release);
    assert!(runtime
        .control(
            session.target(),
            &observation.id,
            &observation.controls[0].id,
            "invoke",
            None,
            &op()
        )
        .is_err());
    let mut config = registration.config.clone();
    config.allow_actions = false;
    runtime
        .update(&registration.id, registration.revision, config)
        .unwrap();
    let current = runtime.sessions().unwrap().pop().unwrap();
    assert_eq!(current.state, "result_unknown");
    assert!(runtime.resume(current.target()).is_err());
}

#[test]
fn workspace_revocation_reaches_the_native_final_dispatch_check() {
    let (db, runtime, host, _) = setup(true);
    db.conn().execute("INSERT INTO workspaces(id,name,description,root_path,status,created_at,updated_at) VALUES('revoked-project','Fixture','','','active',1,1)",[]).unwrap();
    let registration = runtime
        .register(RegistrationInput {
            name: "Revocation fixture".into(),
            adapter: "generic_fixture".into(),
            executable: host.identity.executable.clone(),
            workspace_id: Some("revoked-project".into()),
            allow_observe: true,
            allow_content: false,
            allow_window_ocr: false,
            allow_actions: true,
        })
        .unwrap();
    let session = attached(&runtime, &host, &registration);
    let observation = runtime.observe(session.target()).unwrap();
    let entered = Arc::new(Barrier::new(2));
    let exit = Arc::new(Barrier::new(2));
    *host.gates.lock() = Some((entered.clone(), exit.clone()));
    host.delay_dispatch.store(true, Ordering::Release);
    let worker_runtime = runtime.clone();
    let worker = std::thread::spawn(move || {
        worker_runtime.control(
            session.target(),
            &observation.id,
            &observation.controls[0].id,
            "invoke",
            None,
            &op(),
        )
    });
    entered.wait();
    runtime.revoke_workspace("revoked-project").unwrap();
    db.conn()
        .execute(
            "UPDATE workspaces SET status='archived' WHERE id='revoked-project'",
            [],
        )
        .unwrap();
    exit.wait();
    assert_eq!(worker.join().unwrap().unwrap_err().code, "result_unknown");
    assert_eq!(host.actions.load(Ordering::Acquire), 0);
}

#[test]
fn full_receipt_capacity_blocks_actions_but_not_window_restoration() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    runtime
        .mutate(|state| {
            while state.operations.len() < 4096 {
                let id = op();
                state.operations.insert(
                    id.clone(),
                    crate::software::store::Receipt {
                        fingerprint: "capacity-fixture".into(),
                        record: OperationRecord {
                            operation_id: id,
                            action: "fixture".into(),
                            session_id: None,
                            registration_id: registration.id.clone(),
                            state: "verified".into(),
                            reason: None,
                            created_at: 1,
                            window_evidence: None,
                        },
                    },
                );
            }
            Ok(())
        })
        .unwrap();
    let observation = runtime.observe(session.target()).unwrap();
    assert_eq!(
        runtime
            .control(
                session.target(),
                &observation.id,
                &observation.controls[0].id,
                "invoke",
                None,
                &op()
            )
            .unwrap_err()
            .code,
        "software_history_limit"
    );
    let released = runtime.release(session.target(), &op()).unwrap();
    assert_eq!(released.state, "released");
    assert_eq!(runtime.state.lock().operations.len(), 4096);
    assert_eq!(host.releases.load(Ordering::Acquire), 1);
    assert_eq!(host.actions.load(Ordering::Acquire), 0);
}

#[test]
fn disappeared_window_can_be_released_without_becoming_actionable_again() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    host.lost.store(true, Ordering::Release);
    runtime.refresh_sessions().unwrap();
    let lost = runtime.sessions().unwrap().pop().unwrap();
    assert_eq!(lost.state, "lost");
    assert!(runtime.observe(lost.target()).is_err());
    // A native host reports its removed original bookmark without touching any
    // replacement window. Our fixture uses the same explicit closed-window code.
    host.lost.store(false, Ordering::Release);
    let released = runtime.release(lost.target(), &op()).unwrap();
    assert_eq!(released.state, "released");
    assert!(runtime.resume(session.target()).is_err());
}
