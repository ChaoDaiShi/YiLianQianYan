use super::*;

#[tokio::test]
async fn attention_deadline_blocks_late_http_and_drops_late_proposal() {
    for delay_source in [true, false] {
        let f = Fixture::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let now = chrono::Utc::now().timestamp_millis();
        let expires = now + 60_000;
        let clock = Arc::new(std::sync::atomic::AtomicI64::new(now));
        let source_clock = clock.clone();
        let model_clock = clock.clone();
        let app = Router::new()
            .route("/status", get(move || {
                let clock = source_clock.clone();
                async move {
                    if delay_source { clock.store(expires, Ordering::SeqCst); }
                    "clock fixture"
                }
            }))
            .route("/chat/completions", post(move || {
                let observed = observed.clone();
                let clock = model_clock.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    clock.store(expires, Ordering::SeqCst);
                    let decision = json!({"action":"propose","reason":"changed","recheck_in_secs":30,"proposal":"Verify late evidence"});
                    Json(json!({"id":"late","choices":[{"index":0,"message":{"role":"assistant","content":decision.to_string()},"finish_reason":"stop"}]}))
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let http = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        f.use_loopback_model(address).await;
        let mut watch = f
            .create(json!({"kind":"http","url":format!("http://{address}/status")}))
            .await;
        watch.next_run_at = Some(now);
        if let crate::modules::automation::model::Schedule::Watch { expires_at, .. } =
            &mut watch.schedule
        {
            *expires_at = expires;
        }
        watch.revision += 1;
        f.server.db.save_automation(&watch, false).unwrap();
        super::super::attention::check_with_clock(&f.server, &watch, now, || {
            clock.load(Ordering::SeqCst)
        })
        .await
        .unwrap();
        let stopped = f.get(&watch.id);
        assert!(!stopped.enabled);
        let stopped = stopped.attention.unwrap();
        assert!(stopped.proposal.is_none(), "{stopped:?}");
        assert!(stopped.decision.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(!delay_source));
        http.abort();
    }
}

#[tokio::test]
async fn attention_checks_changed_service_facts_and_http_failure_status() {
    use crate::plugin::service::{ServiceConfiguration, ServiceDeployment, ServiceModelMode};
    let f = Fixture::new();
    let status = Arc::new(AtomicUsize::new(200));
    let seen = Arc::new(Mutex::new(vec![]));
    let app = Router::new().route("/status", get({
        let status = status.clone();
        move || {
            let status = StatusCode::from_u16(status.load(Ordering::SeqCst) as u16).unwrap();
            async move { (status, "unchanged service response") }
        }
    })).route("/chat/completions", post({
        let seen = seen.clone();
        move |Json(body): Json<Value>| {
            let context: Value = serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            seen.lock().unwrap().push(context);
            async {
                let decision = json!({"action":"defer","reason":"check again","recheck_in_secs":2,"proposal":null});
                Json(json!({"id":"facts","choices":[{"index":0,"message":{"role":"assistant","content":decision.to_string()},"finish_reason":"stop"}]}))
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let source = format!("http://{address}/status");
    let http = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    f.use_loopback_model(address).await;
    let runtime = &f.server.plugin_services;
    let instance = runtime
        .start(
            "fixture-plugin",
            "fixture-service",
            ServiceConfiguration {
                package_revision: "test".into(),
                service_id: "fixture-service".into(),
                deployment: ServiceDeployment::Cloud,
                launch: None,
                web_url: None,
                health_url: Some(source.clone()),
                mcp_url: None,
                model_mode: ServiceModelMode::Independent,
                cloud_auth: Default::default(),
            },
            false,
            &[],
        )
        .unwrap();
    let initial = f
        .create(json!({"kind":"http","url":source,"send_preview":true}))
        .await;
    let first = f.check(&initial).await;
    let same = f.check(&first).await;
    assert_eq!(seen.lock().unwrap().len(), 1);
    runtime.health("fixture-plugin", "fixture-service", instance, Ok(()));
    let ready = f.check(&same).await;
    assert_eq!(
        seen.lock().unwrap()[1]["current_facts"]["plugin_service"]["health"],
        "ready"
    );
    let Some(crate::modules::automation::model::AutomationTarget::PersonalAssistant {
        conversation_id,
        ..
    }) = &initial.target
    else {
        panic!("fixture has a task conversation")
    };
    f.server
        .db
        .set_conversation_run_status(
            conversation_id,
            crate::db::ConversationRunStatus::Completed,
            None,
        )
        .unwrap();
    let completed = f.check(&ready).await;
    assert_eq!(
        seen.lock().unwrap()[2]["current_facts"]["conversation_status"],
        "completed"
    );
    status.store(503, Ordering::SeqCst);
    let unavailable = f.check(&completed).await;
    assert_eq!(unavailable.attention.unwrap().retries_used, 1);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[3]["observation"]["http_status"], 503);
    assert!(seen[3]["permitted_source_excerpt"].is_null());
    assert_eq!(
        seen[3]["current_facts"]["plugin_service"]["health"],
        "ready"
    );
    http.abort();
}

#[test]
fn attention_service_fact_references_the_matching_runtime_and_current_health() {
    use crate::plugin::service::{ServiceConfiguration, ServiceDeployment, ServiceModelMode};

    let f = Fixture::new();
    let source = "https://service-fixture.invalid/exact-health";
    let runtime = &f.server.plugin_services;
    let instance = runtime
        .start(
            "fixture-plugin",
            "fixture-service",
            ServiceConfiguration {
                package_revision: "test".into(),
                service_id: "fixture-service".into(),
                deployment: ServiceDeployment::Cloud,
                launch: None,
                web_url: None,
                health_url: Some(source.into()),
                mcp_url: None,
                model_mode: ServiceModelMode::Independent,
                cloud_auth: Default::default(),
            },
            false,
            &[],
        )
        .unwrap();
    let snapshot = |url| runtime.attention_service_for_url(url).unwrap();
    assert_eq!(
        snapshot(source),
        Some((
            "fixture-plugin/fixture-service".into(),
            Some(instance),
            "starting"
        ))
    );
    assert_eq!(snapshot("https://service-fixture.invalid/wrong-path"), None);
    runtime.health("fixture-plugin", "fixture-service", instance, Ok(()));
    assert_eq!(snapshot(source).unwrap().2, "ready");
    runtime.stop("fixture-plugin", "fixture-service").unwrap();
    assert_eq!(
        snapshot(source),
        Some(("fixture-plugin/fixture-service".into(), None, "stopped"))
    );
}
