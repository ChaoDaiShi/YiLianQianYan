use super::*;
use std::sync::{atomic::AtomicUsize, Barrier};

struct FixtureHost {
    identity: WindowIdentity,
    actions: AtomicUsize,
    launches: AtomicUsize,
    releases: AtomicUsize,
    unknown: AtomicBool,
    unknown_launch: AtomicBool,
    delay_dispatch: AtomicBool,
    lost: AtomicBool,
    hidden: AtomicBool,
    candidates: Mutex<Vec<WindowCandidate>>,
    mode: Mutex<String>,
    gates: Mutex<Option<(Arc<Barrier>, Arc<Barrier>)>>,
}
impl FixtureHost {
    fn new() -> Self {
        let identity = WindowIdentity {
            hwnd: "4242".into(),
            pid: 42,
            process_started_at: 123456,
            executable: std::env::current_exe()
                .unwrap()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            class_name: "YilianOwnedFixture".into(),
        };
        Self {
            candidates: Mutex::new(vec![WindowCandidate {
                identity: identity.clone(),
                visible: true,
                minimized: false,
            }]),
            identity,
            actions: AtomicUsize::new(0),
            launches: AtomicUsize::new(0),
            releases: AtomicUsize::new(0),
            unknown: AtomicBool::new(false),
            unknown_launch: AtomicBool::new(false),
            delay_dispatch: AtomicBool::new(false),
            lost: AtomicBool::new(false),
            hidden: AtomicBool::new(false),
            mode: Mutex::new("independent".into()),
            gates: Mutex::new(None),
        }
    }
    fn window(&self) -> WindowState {
        WindowState {
            identity: self.identity.clone(),
            mode: self.mode.lock().clone(),
            visible: !self.hidden.load(Ordering::Acquire),
            focused: false,
            minimized: false,
            embedding_supported: true,
            reason: None,
        }
    }
}
impl SoftwareHost for FixtureHost {
    fn discover(&self, _: &Registration) -> Result<Vec<WindowCandidate>, SoftwareError> {
        Ok(self.candidates.lock().clone())
    }
    fn launch(&self, _: &Registration) -> Result<WindowCandidate, SoftwareError> {
        self.launches.fetch_add(1, Ordering::AcqRel);
        if self.unknown_launch.load(Ordering::Acquire) {
            return Err(SoftwareError::unknown(
                "Launch completed without a verified window",
            ));
        }
        Ok(WindowCandidate {
            identity: self.identity.clone(),
            visible: true,
            minimized: false,
        })
    }
    fn verify(&self, target: &WindowIdentity) -> Result<WindowState, SoftwareError> {
        if *target != self.identity || self.lost.load(Ordering::Acquire) {
            Err(SoftwareError::new(
                "software_window_gone",
                "Fixture window no longer matches",
            ))
        } else {
            Ok(self.window())
        }
    }
    fn present(
        &self,
        target: &WindowIdentity,
        mode: &str,
        _: Option<Viewport>,
    ) -> Result<WindowState, SoftwareError> {
        self.verify(target)?;
        *self.mode.lock() = mode.into();
        Ok(self.window())
    }
    fn release(&self, target: &WindowIdentity) -> Result<(), SoftwareError> {
        self.verify(target)?;
        self.releases.fetch_add(1, Ordering::AcqRel);
        *self.mode.lock() = "independent".into();
        Ok(())
    }
    fn recognize_window(&self, target: &WindowIdentity) -> Result<String, SoftwareError> {
        self.verify(target)?;
        Ok("画面文字".repeat(1400))
    }
    fn wechat_frame_png(&self, target: &WindowIdentity) -> Result<Vec<u8>, SoftwareError> {
        self.verify(target)?;
        Ok(b"fixture frame".to_vec())
    }
    fn observe(
        &self,
        target: &WindowIdentity,
        observation_id: &str,
        _: bool,
    ) -> Result<Vec<SoftwareControl>, SoftwareError> {
        self.verify(target)?;
        Ok(vec![SoftwareControl {
            id: format!("{observation_id}:button"),
            name: if self.actions.load(Ordering::Acquire) > 0 {
                "Applied"
            } else {
                "Fixture action"
            }
            .into(),
            role: "Button".into(),
            automation_id: "action".into(),
            enabled: true,
            can_invoke: true,
            can_set_value: false,
            selected: None,
            toggle_state: None,
            expand_state: None,
            visible_text: None,
            is_password: false,
            fingerprint: "v1".into(),
            path: vec![0],
        }])
    }
    fn control(
        &self,
        target: &WindowIdentity,
        _: &SoftwareControl,
        _: &str,
        _: Option<&str>,
        cancellation: &AtomicBool,
    ) -> Result<(), SoftwareError> {
        self.verify(target)?;
        let delayed = self.delay_dispatch.load(Ordering::Acquire);
        if !delayed {
            self.actions.fetch_add(1, Ordering::AcqRel);
        }
        let gates = self.gates.lock().clone();
        if let Some((entered, exit)) = gates {
            entered.wait();
            exit.wait();
        }
        if delayed {
            if cancellation.load(Ordering::Acquire) {
                return Err(SoftwareError::new(
                    "cancelled",
                    "Cancelled before the fixture's final dispatch",
                ));
            }
            self.actions.fetch_add(1, Ordering::AcqRel);
        }
        if self.unknown.load(Ordering::Acquire) {
            Err(SoftwareError::unknown("Fixture action may have run"))
        } else {
            Ok(())
        }
    }
    fn shutdown(&self) -> Result<(), SoftwareError> {
        *self.mode.lock() = "independent".into();
        Ok(())
    }
}
fn op() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn setup(
    permissions: bool,
) -> (
    Database,
    Arc<SoftwareRuntime>,
    Arc<FixtureHost>,
    Registration,
) {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let runtime = Arc::new(SoftwareRuntime::new(db.clone()));
    let host = Arc::new(FixtureHost::new());
    runtime.install_host(host.clone());
    let registration = runtime
        .register(RegistrationInput {
            name: "Fixture".into(),
            adapter: "generic_fixture".into(),
            executable: host.identity.executable.clone(),
            workspace_id: None,
            allow_observe: permissions,
            allow_content: false,
            allow_window_ocr: false,
            allow_actions: permissions,
        })
        .unwrap();
    (db, runtime, host, registration)
}
fn attached(runtime: &SoftwareRuntime, host: &FixtureHost, registration: &Registration) -> Session {
    runtime
        .attach(&registration.id, host.identity.clone(), &op())
        .unwrap()
}

#[path = "tests/observations.rs"]
mod observations;

#[test]
fn registration_permissions_default_closed_and_no_launch_on_discovery() {
    let (_, runtime, host, registration) = setup(false);
    assert_eq!(runtime.discover(&registration.id).unwrap().len(), 1);
    assert_eq!(host.launches.load(Ordering::Acquire), 0);
    let session = attached(&runtime, &host, &registration);
    assert_eq!(
        runtime
            .ensure_ai_allowed(&session.target(), "observe")
            .unwrap_err()
            .code,
        "software_observation_not_authorized"
    );
    assert!(runtime
        .remove(&registration.id, registration.revision)
        .is_err());
    let mut expanded = registration.config.clone();
    expanded.allow_observe = true;
    assert_eq!(
        runtime
            .update(&registration.id, registration.revision, expanded)
            .unwrap_err()
            .code,
        "software_session_active"
    );
}
#[test]
fn exact_candidate_and_multinstance_choice_never_fallback_to_first() {
    let (_, runtime, host, registration) = setup(false);
    let mut fake = host.identity.clone();
    fake.process_started_at += 1;
    assert_eq!(
        runtime
            .attach(&registration.id, fake, &op())
            .unwrap_err()
            .code,
        "stale_software_candidate"
    );
    let mut second = host.candidates.lock()[0].clone();
    second.identity.hwnd = "9898".into();
    host.candidates.lock().push(second);
    assert_eq!(runtime.discover(&registration.id).unwrap().len(), 2);
    assert_eq!(
        runtime.launch(&registration.id, &op()).unwrap_err().code,
        "software_existing_windows"
    );
    assert_eq!(host.launches.load(Ordering::Acquire), 0);
}
#[test]
fn structured_action_receipt_is_idempotent_and_observation_is_single_use() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    let observation = runtime.observe(session.target()).unwrap();
    let operation = op();
    let element = &observation.controls[0].id;
    let next = runtime
        .control(
            session.target(),
            &observation.id,
            element,
            "invoke",
            None,
            &operation,
        )
        .unwrap();
    runtime
        .control(
            session.target(),
            &observation.id,
            element,
            "invoke",
            None,
            &operation,
        )
        .unwrap();
    assert_eq!(host.actions.load(Ordering::Acquire), 1);
    assert_eq!(
        runtime
            .control(
                next.target(),
                &observation.id,
                element,
                "invoke",
                None,
                &op()
            )
            .unwrap_err()
            .code,
        "stale_software_observation"
    );
    assert_eq!(
        runtime
            .control(
                session.target(),
                &observation.id,
                element,
                "focus",
                None,
                &operation
            )
            .unwrap_err()
            .code,
        "software_operation_conflict"
    );
}
#[test]
fn unknown_result_requires_inspection_and_never_replays() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    let observation = runtime.observe(session.target()).unwrap();
    host.unknown.store(true, Ordering::Release);
    let operation = op();
    assert_eq!(
        runtime
            .control(
                session.target(),
                &observation.id,
                &observation.controls[0].id,
                "invoke",
                None,
                &operation
            )
            .unwrap_err()
            .code,
        "result_unknown"
    );
    assert_eq!(
        runtime
            .control(
                session.target(),
                &observation.id,
                &observation.controls[0].id,
                "invoke",
                None,
                &operation
            )
            .unwrap_err()
            .code,
        "result_unknown"
    );
    let unknown = runtime.sessions().unwrap().pop().unwrap();
    assert_eq!(unknown.state, "result_unknown");
    assert!(runtime.observe(unknown.target()).is_err());
    assert!(runtime.resume(unknown.target()).is_err());
    let inspected = runtime.acknowledge_unknown(unknown.target()).unwrap();
    assert_eq!(inspected.state, "stopped");
    assert_eq!(host.actions.load(Ordering::Acquire), 1);
    assert_eq!(runtime.resume(inspected.target()).unwrap().state, "active");
}
#[test]
fn cancellation_before_worker_start_reserves_id_and_performs_no_action() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    let observation = runtime.observe(session.target()).unwrap();
    let operation = op();
    runtime.cancel_operation(&operation);
    assert_eq!(
        runtime
            .control(
                session.target(),
                &observation.id,
                &observation.controls[0].id,
                "invoke",
                None,
                &operation
            )
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert!(runtime
        .control(
            session.target(),
            &observation.id,
            &observation.controls[0].id,
            "invoke",
            None,
            &operation
        )
        .is_err());
    assert_eq!(host.actions.load(Ordering::Acquire), 0);
}
#[test]
fn stop_during_dispatch_preserves_unknown_and_rejects_late_success() {
    let (_, runtime, host, registration) = setup(true);
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
    let stopped = runtime.stop(session.target()).unwrap();
    assert_eq!(stopped.state, "result_unknown");
    assert!(runtime.acknowledge_unknown(stopped.target()).is_err());
    exit.wait();
    assert_eq!(worker.join().unwrap().unwrap_err().code, "result_unknown");
    let current = runtime.sessions().unwrap().pop().unwrap();
    assert_eq!(current.state, "result_unknown");
    assert_eq!(host.actions.load(Ordering::Acquire), 1);
}
#[test]
fn concurrent_inventory_writers_fail_closed_instead_of_overwriting() {
    let (db, runtime, _, _) = setup(false);
    let stale = SoftwareRuntime::new(db);
    let mut config = runtime.registrations().unwrap()[0].config.clone();
    config.name = "New entry".into();
    runtime.register(config.clone()).unwrap();
    assert_eq!(
        stale.register(config).unwrap_err().code,
        "stale_software_inventory"
    );
}

#[test]
fn filetime_identity_preserves_every_bit_in_json_and_rejects_numeric_input() {
    let host = FixtureHost::new();
    let mut identity = host.identity;
    identity.process_started_at = 134_064_475_637_013_431;
    let mut json = serde_json::to_value(&identity).unwrap();
    assert_eq!(json["process_started_at"], "134064475637013431");
    assert_eq!(
        serde_json::from_value::<WindowIdentity>(json.clone()).unwrap(),
        identity
    );
    json["process_started_at"] = serde_json::json!(134_064_475_637_013_431u64);
    assert!(serde_json::from_value::<WindowIdentity>(json).is_err());
}

#[path = "tests/recovery.rs"]
mod recovery;

#[path = "tests/sessions.rs"]
mod sessions;

#[test]
fn wechat_draft_tool_pins_one_writable_window_before_approval() {
    let (_, runtime, host, registration) = setup(false);
    let tool = crate::tools::software::software_tools(runtime.clone())
        .into_iter()
        .find(|tool| tool.name() == "software_wechat_draft")
        .unwrap();
    let session = attached(&runtime, &host, &registration);
    let request = || serde_json::json!({"group":"cs一群","value":"cs2","replace":true});
    assert!(tool.prepare_call_arguments("chat", &mut request()).is_err());
    runtime
        .mutate(|inventory| {
            let config = &mut inventory
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config;
            config.adapter = "wechat".into();
            config.allow_observe = true;
            Ok(())
        })
        .unwrap();
    assert!(tool
        .prepare_call_arguments("chat", &mut request())
        .unwrap_err()
        .contains("许可"));
    runtime
        .mutate(|inventory| {
            inventory
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config
                .allow_actions = true;
            Ok(())
        })
        .unwrap();
    let mut approved = request();
    tool.prepare_call_arguments("chat", &mut approved).unwrap();
    assert_eq!(approved["target"], serde_json::json!(session.target()));
    assert!(tool.security_descriptor(&approved).is_ok());
    runtime
        .mutate(|inventory| {
            let mut second = session.clone();
            second.id = "other".into();
            inventory.sessions.insert(second.id.clone(), second);
            Ok(())
        })
        .unwrap();
    assert!(tool
        .prepare_call_arguments("chat", &mut request())
        .unwrap_err()
        .contains("多个"));
    runtime.stop(session.target()).unwrap();
    assert!(tool.security_descriptor(&approved).is_err());
    let mut fresh = request();
    tool.prepare_call_arguments("chat", &mut fresh).unwrap();
    assert_eq!(fresh["target"]["session_id"], "other");
    runtime
        .mutate(|inventory| {
            inventory
                .registrations
                .get_mut(&registration.id)
                .unwrap()
                .config
                .workspace_id = Some("other-project".into());
            Ok(())
        })
        .unwrap();
    assert!(tool.prepare_call_arguments("chat", &mut request()).is_err());
    assert_eq!(host.actions.load(Ordering::Acquire), 0);
}

#[test]
fn generic_control_binds_scope_and_verifies_new_label() {
    let (_, runtime, host, registration) = setup(true);
    let session = attached(&runtime, &host, &registration);
    assert_eq!(
        runtime.chat_target("chat", Some("Fixture"), None).unwrap(),
        session.target()
    );
    assert!(runtime
        .chat_target("chat", Some("different app"), None)
        .is_err());
    let observed = runtime.observe(session.target()).unwrap();
    assert_eq!(
        runtime.observation_target(&observed.id).unwrap(),
        session.target()
    );
    assert!(runtime.observation_target("expired").is_err());
    let button = &observed.controls[0];
    let invoke = |target, label| {
        runtime.control_with_outcome(
            target,
            &observed.id,
            &button.id,
            "invoke",
            None,
            Some(label),
            &op(),
        )
    };
    assert_eq!(
        invoke(session.target(), "Fixture action").unwrap_err().code,
        "postcondition_already_visible"
    );
    assert_eq!(host.actions.load(Ordering::Acquire), 0);
    let updated = invoke(session.target(), "Applied").unwrap();
    assert_eq!(host.actions.load(Ordering::Acquire), 1);
    assert_eq!(
        invoke(session.target(), "Applied").unwrap_err().code,
        "stale_software_target"
    );
    assert_eq!(
        invoke(updated.target(), "Applied").unwrap_err().code,
        "stale_software_observation"
    );
}
