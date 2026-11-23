use super::*;

impl SoftwareRuntime {
    pub(crate) fn capability_evidence(
        &self,
        registrations: &[Registration],
        sessions: &[Session],
        monitors: &[monitor::MonitorStatus],
    ) -> Vec<SoftwareCapabilityEvidence> {
        let available = self.available();
        let state = self.state.lock();
        let mut receipts: HashMap<&str, Vec<&OperationRecord>> = HashMap::new();
        for record in state.operations.values().map(|receipt| &receipt.record) {
            if let Some(session_id) = record.session_id.as_deref() {
                if record.state == "verified"
                    && matches!(record.action.as_str(), "attach" | "launch" | "control")
                {
                    receipts.entry(session_id).or_default().push(record);
                }
            }
        }
        let observations = self.observations.lock();
        registrations
            .iter()
            .flat_map(|registration| {
                let matching = sessions
                    .iter()
                    .filter(|session| session.registration_id == registration.id);
                std::iter::once(None)
                    .chain(matching.map(Some))
                    .map(|session| {
                        let observation = session.and_then(|session| observations.get(&session.id));
                        let history = session
                            .and_then(|session| receipts.get(session.id.as_str()))
                            .map_or(&[][..], Vec::as_slice);
                        project(
                            registration,
                            session,
                            observation,
                            history,
                            monitors,
                            available,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

fn entry(
    dimension: &'static str,
    status: &'static str,
    source: &'static str,
) -> SoftwareCapabilityEntry {
    SoftwareCapabilityEntry {
        dimension,
        status,
        source,
        evidence_id: None,
        observed_at: None,
    }
}

fn observed(
    dimension: &'static str,
    status: &'static str,
    source: &'static str,
    id: String,
    time: i64,
) -> SoftwareCapabilityEntry {
    SoftwareCapabilityEntry {
        evidence_id: Some(id),
        observed_at: Some(time),
        ..entry(dimension, status, source)
    }
}

pub(super) fn project(
    registration: &Registration,
    session: Option<&Session>,
    observation: Option<&Observation>,
    history: &[&OperationRecord],
    monitors: &[monitor::MonitorStatus],
    host_available: bool,
) -> SoftwareCapabilityEvidence {
    let snapshot = |entries| SoftwareCapabilityEvidence {
        registration_id: registration.id.clone(),
        session_id: session.map(|item| item.id.clone()),
        generation: session.map(|item| item.generation),
        entries,
    };
    if !host_available {
        return snapshot(
            [
                "discover_launch",
                "display",
                "perceive",
                "operate",
                "background_run",
                "background_interaction",
            ]
            .into_iter()
            .map(|name| entry(name, "unavailable", "native_host_unavailable"))
            .collect(),
        );
    }

    let current_window = session
        .filter(|item| matches!(item.state.as_str(), "active" | "stopped" | "result_unknown"));
    let active = current_window.filter(|item| item.state == "active");
    let display = current_window.map(|item| WindowEvidence::capture(item.generation, &item.window));
    let discovery = current_window.and_then(|session| {
        history.iter().find(|record| {
            record.state == "verified"
                && record.session_id.as_deref() == Some(session.id.as_str())
                && matches!(record.action.as_str(), "attach" | "launch")
        })
    });
    let uia = active.zip(observation).and_then(|(session, observation)| {
        let window = observation.window_evidence.as_ref()?;
        (observation.target == session.target()
            && window.matches(session)
            && !observation.controls.is_empty())
        .then_some((observation, window))
    });
    let control = active.and_then(|session| {
        history.iter().find_map(|record| {
            let window = record.window_evidence.as_ref()?;
            (record.state == "verified"
                && record.action == "control"
                && record.session_id.as_deref() == Some(session.id.as_str())
                && window.matches(session))
            .then_some((record, window))
        })
    });
    let observed_window = |dimension, status, source, item: &Session, proof: &WindowEvidence| {
        observed(
            dimension,
            status,
            source,
            item.id.clone(),
            proof.observed_at,
        )
    };
    let display_row = current_window.zip(display.as_ref()).map_or_else(
        || entry("display", "unverified", "current_window_missing"),
        |(item, proof)| {
            observed_window(
                "display",
                if proof.is_background() {
                    "unavailable"
                } else {
                    "verified"
                },
                if proof.is_background() {
                    "window_hidden_or_minimized"
                } else {
                    "window_verified"
                },
                item,
                proof,
            )
        },
    );
    let background_window = current_window
        .zip(display.as_ref())
        .filter(|(_, proof)| proof.is_background());
    let background_process = session
        .filter(|item| item.state == "lost")
        .and_then(|session| {
            monitors
                .iter()
                .find_map(|item| item.windowless_process_evidence(session))
        });
    let background_run = match (background_window, background_process) {
        (Some((item, proof)), _) => observed_window(
            "background_run",
            "verified",
            "window_process_alive",
            item,
            proof,
        ),
        (None, Some((id, time))) if current_window.is_none() => observed(
            "background_run",
            "verified",
            "monitor_process_alive",
            id,
            time,
        ),
        _ => entry(
            "background_run",
            "unverified",
            "background_runtime_unobserved",
        ),
    };

    let can_observe = registration.config.allow_observe;
    let can_operate = can_observe && registration.config.allow_actions;
    let (background_uia, background_control) = (
        uia.filter(|(_, window)| window.is_background())
            .filter(|_| can_observe),
        control
            .filter(|(_, window)| window.is_background())
            .filter(|_| can_operate),
    );
    let background_interaction = match (background_uia, background_control) {
        (_, Some((record, proof))) => observed(
            "background_interaction",
            "verified",
            "background_control",
            record.operation_id.clone(),
            proof.observed_at,
        ),
        (Some((observation, proof)), _) => observed(
            "background_interaction",
            "partial",
            "background_uia",
            observation.id.clone(),
            proof.observed_at,
        ),
        _ => monitors
            .iter()
            .find_map(|item| item.notification_evidence(&registration.id))
            .map_or_else(
                || {
                    entry(
                        "background_interaction",
                        "unverified",
                        "background_interaction_unobserved",
                    )
                },
                |(id, time)| {
                    observed(
                        "background_interaction",
                        "partial",
                        "windows_notification_only",
                        id,
                        time,
                    )
                },
            ),
    };
    snapshot(vec![
        discovery.map_or_else(
            || {
                entry(
                    "discover_launch",
                    "unverified",
                    "matching_window_unobserved",
                )
            },
            |record| {
                observed(
                    "discover_launch",
                    "verified",
                    if record.action == "launch" {
                        "launch_receipt"
                    } else {
                        "attach_receipt"
                    },
                    record.operation_id.clone(),
                    record
                        .window_evidence
                        .as_ref()
                        .map_or(record.created_at, |x| x.observed_at),
                )
            },
        ),
        display_row,
        if !can_observe {
            entry(
                "perceive",
                "authorization_required",
                "observation_permission",
            )
        } else {
            uia.map_or_else(
                || entry("perceive", "unverified", "uia_unobserved"),
                |(item, proof)| {
                    observed(
                        "perceive",
                        "verified",
                        "uia_observation",
                        item.id.clone(),
                        proof.observed_at,
                    )
                },
            )
        },
        if !can_operate {
            entry("operate", "authorization_required", "action_permission")
        } else {
            control.map_or_else(
                || entry("operate", "unverified", "control_unobserved"),
                |(item, proof)| {
                    observed(
                        "operate",
                        "verified",
                        "control_receipt",
                        item.operation_id.clone(),
                        proof.observed_at,
                    )
                },
            )
        },
        background_run,
        background_interaction,
    ])
}
