use super::{Database, LlmModelInput, LlmUsageInput};

fn test_db() -> Database {
    let path =
        std::env::temp_dir().join(format!("yilian-llm-models-red-{}.db", uuid::Uuid::new_v4()));
    Database::new(&path).unwrap()
}

#[test]
fn model_profiles_round_trip_without_api_key_value() {
    let db = test_db();
    let created = db
        .create_llm_model(&LlmModelInput {
            id: "model-1".into(),
            provider: "deepseek".into(),
            label: "研发模型".into(),
            model: "deepseek-chat".into(),
            base_url: "http://127.0.0.1:9000/v1".into(),
            api_format: "openai".into(),
            api_key_ref: "llm.model-1".into(),
            api_key_env: String::new(),
            temperature: 0.2,
            max_tokens: 1024,
            invoke_timeout_ms: 30_000,
        })
        .unwrap();

    assert_eq!(created.id, "model-1");
    assert_eq!(created.model, "deepseek-chat");
    assert_eq!(created.api_key_ref, "llm.model-1");
    assert!(db.get_active_llm_model().unwrap().is_none());
}

#[test]
fn activation_and_usage_aggregate_by_day() {
    let db = test_db();
    db.create_llm_model(&LlmModelInput {
        id: "model-1".into(),
        provider: "openai".into(),
        label: "GPT".into(),
        model: "gpt-4o-mini".into(),
        base_url: "http://127.0.0.1:9000/v1".into(),
        api_format: "openai".into(),
        api_key_ref: "llm.model-1".into(),
        api_key_env: String::new(),
        temperature: 0.0,
        max_tokens: 512,
        invoke_timeout_ms: 30_000,
    })
    .unwrap();
    db.activate_llm_model("model-1").unwrap();
    assert_eq!(db.get_active_llm_model().unwrap().unwrap().id, "model-1");

    db.record_llm_usage(&LlmUsageInput {
        id: "usage-1".into(),
        model_id: "model-1".into(),
        provider: "openai".into(),
        model: "gpt-4o-mini".into(),
        prompt_tokens: 10,
        completion_tokens: 4,
        total_tokens: 14,
        recorded_at: 1_725_494_400_000,
        source: "chat".into(),
    })
    .unwrap();
    db.record_llm_usage(&LlmUsageInput {
        id: "usage-2".into(),
        model_id: "model-1".into(),
        provider: "openai".into(),
        model: "gpt-4o-mini".into(),
        prompt_tokens: 3,
        completion_tokens: 2,
        total_tokens: 5,
        recorded_at: 1_725_494_400_000 + 86_400_000,
        source: "verify".into(),
    })
    .unwrap();

    let report = db
        .aggregate_llm_usage(Some("model-1"), 1_725_494_300_000, 1_725_580_800_001)
        .unwrap();
    assert_eq!(report.total_tokens, 19);
    assert_eq!(report.prompt_tokens, 13);
    assert_eq!(report.completion_tokens, 6);
    assert_eq!(report.days.len(), 2);
    assert_eq!(report.days[0].total_tokens, 14);
}
