use serde_json::{json, Map, Value};

use crate::safety::{redact_and_digest, redact_error};

#[test]
fn secret_values_never_affect_the_safe_digest() {
    let first = redact_and_digest(&json!({
        "nested": {"api_key": "alpha", "Password": "one"},
        "items": [{"authorization": "Bearer raw-token"}]
    }));
    let second = redact_and_digest(&json!({
        "nested": {"api_key": "beta", "Password": "two"},
        "items": [{"authorization": "Bearer another-token"}]
    }));

    assert_eq!(
        first.value,
        json!({
            "items": [{"authorization": "[REDACTED]"}],
            "nested": {"Password": "[REDACTED]", "api_key": "[REDACTED]"}
        })
    );
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.digest.len(), 64);
}

#[test]
fn canonical_digest_is_independent_of_object_insertion_order() {
    let mut first = Map::new();
    first.insert("z".to_string(), json!(1));
    first.insert("a".to_string(), json!({"b": 2, "a": 1}));

    let mut second = Map::new();
    second.insert("a".to_string(), json!({"a": 1, "b": 2}));
    second.insert("z".to_string(), json!(1));

    assert_eq!(
        redact_and_digest(&Value::Object(first)).digest,
        redact_and_digest(&Value::Object(second)).digest
    );
}

#[test]
fn long_text_and_data_uris_are_replaced_with_bounded_metadata() {
    let long_text = "界".repeat(2_049);
    let data_uri = format!("data:image/png;base64,{}", "A".repeat(4_096));
    let redacted = redact_and_digest(&json!({
        "long": long_text,
        "image": data_uri,
    }));

    assert_eq!(redacted.value["long"]["kind"], "truncated_text");
    assert_eq!(redacted.value["long"]["length"], 2_049);
    assert!(
        redacted.value["long"]["preview"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= 256
    );
    assert_eq!(redacted.value["long"]["sha256"].as_str().unwrap().len(), 64);

    assert_eq!(redacted.value["image"]["kind"], "omitted_data_uri");
    assert!(redacted.value["image"]["length"].as_u64().unwrap() > 4_096);
    assert_eq!(
        redacted.value["image"]["sha256"].as_str().unwrap().len(),
        64
    );
    assert!(!redacted.value.to_string().contains("AAAA"));
}

#[test]
fn inline_credentials_are_removed_from_error_messages() {
    let raw = "request failed: Authorization: Bearer abc.def token=xyz password: hunter2";
    let safe = redact_error(raw);

    assert!(!safe.contains("abc.def"));
    assert!(!safe.contains("xyz"));
    assert!(!safe.contains("hunter2"));
    assert!(safe.matches("[REDACTED]").count() >= 3);
}

#[test]
fn non_secret_changes_produce_a_different_digest() {
    let first = redact_and_digest(&json!({"path": "a.txt", "token": "same"}));
    let second = redact_and_digest(&json!({"path": "b.txt", "token": "same"}));

    assert_ne!(first.digest, second.digest);
}

#[test]
fn long_inline_secret_keeps_original_length_without_storing_secret_text() {
    let raw = format!("token={}", "s".repeat(3_000));
    let redacted = redact_and_digest(&json!({"message": raw}));

    assert_eq!(redacted.value["message"]["kind"], "truncated_text");
    assert_eq!(redacted.value["message"]["length"], 3_006);
    assert!(!redacted.value.to_string().contains(&"s".repeat(32)));
}
