use super::*;

#[test]
fn permission_revocation_invalidates_binding_and_stops_session() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    let old_binding = runtime.binding(&session.target()).unwrap();
    let old_epoch = runtime.authorization_epoch();
    let mut config = registration.config.clone();
    config.allow_actions = false;
    let revised = runtime
        .update(&registration.id, registration.revision, config)
        .unwrap();
    assert!(revised.revision > registration.revision);
    assert!(runtime.authorization_epoch() > old_epoch);
    assert!(runtime.binding(&session.target()).is_err());
    let stopped = runtime.sessions().unwrap().pop().unwrap();
    assert_eq!(stopped.state, "stopped");
    assert_ne!(runtime.binding(&stopped.target()).unwrap(), old_binding);
    let active = runtime.resume(stopped.target()).unwrap();
    assert_eq!(
        runtime
            .ensure_ai_allowed(&active.target(), "control")
            .unwrap_err()
            .code,
        "software_actions_not_authorized"
    );
}
#[test]
fn restart_marks_sessions_interrupted_without_any_native_replay() {
    let (db, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    runtime
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
    let restored = SoftwareRuntime::new(db);
    restored.install_host(host.clone());
    assert_eq!(restored.registrations().unwrap().len(), 1);
    assert_eq!(restored.sessions().unwrap()[0].state, "interrupted");
    assert_eq!(host.launches.load(Ordering::Acquire), 0);
    assert_eq!(host.actions.load(Ordering::Acquire), 0);
    assert!(restored
        .resume(restored.sessions().unwrap()[0].target())
        .is_err());
}
#[test]
fn independent_release_restores_without_terminating_process_and_blocks_stale_targets() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    assert!(runtime
        .present(
            session.target(),
            "embedded",
            Some(Viewport {
                x: i32::MIN,
                y: 0,
                width: 640,
                height: 480
            }),
            &op()
        )
        .is_err());
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
    assert!(runtime.observe(session.target()).is_err());
    let released = runtime.release(embedded.target(), &op()).unwrap();
    assert_eq!(released.state, "released");
    assert_eq!(released.window.mode, "independent");
    assert_eq!(host.releases.load(Ordering::Acquire), 1);
    runtime
        .remove(&registration.id, registration.revision)
        .unwrap();
}
