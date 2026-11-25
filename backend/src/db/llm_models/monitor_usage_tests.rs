use super::*;
#[test]
fn basic_pool_tokens_join_usage_without_counting_image_units_as_tokens() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-20T16:30:00Z")
        .unwrap()
        .timestamp_millis();
    db.save_provider_account(&crate::db::model_pools::ProviderAccount {
        id: "vision-account".into(),
        provider_id: "vision-provider".into(),
        label: "Vision".into(),
        base_url: "https://example.invalid".into(),
        credential_id: None,
        profile_ids: vec![],
    })
    .unwrap();
    for (id, kind) in [("vision-profile", "vision"), ("image-profile", "image")] {
        db.save_basic_service_profile(
            id,
            crate::db::basic_services::BasicServiceInput {
                label: id.into(),
                provider_account_id: "vision-account".into(),
                service_kind: kind.into(),
                model: format!("actual-{kind}"),
                adapter: "openai".into(),
                parameters: serde_json::json!({}),
                expected_revision: None,
            },
        )
        .unwrap();
    }
    db.conn().execute("INSERT INTO model_pool_attempts(id,request_id,model_id,tier,started_at,ended_at,usage) VALUES('vision-attempt','r','vision-profile',1,?1,?1,'{\"prompt_tokens\":7,\"completion_tokens\":3,\"total_tokens\":10}'),('image-attempt','r','image-profile',1,?1,?1,'{\"image_count\":1}')",[at]).unwrap();
    let report = db
        .aggregate_llm_usage_in_timezone(None, at - 1, at + 1, 480)
        .unwrap();
    assert_eq!(
        (
            report.prompt_tokens,
            report.completion_tokens,
            report.total_tokens,
            report.known_usage_count
        ),
        (7, 3, 10, 1)
    );
    assert_eq!(report.days[0].date, "2026-10-21");
    let vision = report
        .models
        .iter()
        .find(|m| m.model_id == "vision-profile")
        .unwrap();
    assert_eq!(
        (&*vision.provider, &*vision.model, vision.known_usage_count),
        ("vision-provider", "actual-vision", 1)
    );
    let image = report
        .models
        .iter()
        .find(|m| m.model_id == "image-profile")
        .unwrap();
    assert_eq!((image.total_tokens, image.known_usage_count), (0, 0));
    let filtered = db
        .aggregate_llm_usage_in_timezone(Some("vision-profile"), at - 1, at + 1, 480)
        .unwrap();
    assert_eq!(filtered.total_tokens, 10);
    assert_eq!(filtered.models.len(), 1);
}
#[test]
fn local_usage_days_keep_unknown_failed_attempts_separate_and_filter_models() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    db.conn().execute("INSERT INTO llm_models(id,provider,label,model,base_url,api_key_ref,created_at,updated_at) VALUES('model-a','local','Model A','actual-a','http://127.0.0.1:18001/v1','fixture-a',1,1),('model-b','local','Model B','actual-b','http://127.0.0.1:18002/v1','fixture-b',1,1)",[]).unwrap();
    let base = chrono::DateTime::parse_from_rfc3339("2026-10-20T15:30:00Z")
        .unwrap()
        .timestamp_millis();
    for (id, recorded_at, prompt, completion) in
        [("known-a", base, 4, 1), ("known-b", base + 3600000, 6, 4)]
    {
        db.record_llm_usage(&LlmUsageInput {
            id: id.into(),
            model_id: "model-a".into(),
            provider: "local".into(),
            model: "actual-a".into(),
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            recorded_at,
            source: "test".into(),
        })
        .unwrap();
    }
    db.conn().execute("INSERT INTO model_pool_attempts(id,request_id,model_id,tier,started_at,ended_at,failure,usage) VALUES('known-a','request-a','model-a',0,?1,?1,'timeout','{}'),('unknown-b','request-b','model-b',1,?1,?1,'rate_limit',NULL),('pending-c','request-c','model-b',1,?1,NULL,NULL,NULL)",[base]).unwrap();
    let report = db
        .aggregate_llm_usage_in_timezone(None, base - 1000, base + 7200000, 480)
        .unwrap();
    assert_eq!(report.total_tokens, 15);
    assert_eq!(report.known_usage_count, 2);
    assert_eq!(report.unknown_usage_count, 1);
    assert_eq!(report.failed_attempt_count, 2);
    assert_eq!(report.pending_attempt_count, 1);
    assert_eq!(report.days[0].date, "2026-10-20");
    assert_eq!(report.days[1].date, "2026-10-21");
    let filtered = db
        .aggregate_llm_usage_in_timezone(Some("model-a"), base - 1000, base + 7200000, 480)
        .unwrap();
    assert_eq!(filtered.models.len(), 1);
    assert_eq!(filtered.unknown_usage_count, 0);
    assert_eq!(filtered.failed_attempt_count, 1);
    assert!(db
        .aggregate_llm_usage_in_timezone(None, base, base + 1000, 900)
        .is_err());
}
