use super::super::execution::NodeContext;
use super::super::input_binding_resolver::TypedInputBindingResolver;
use super::*;
use serde_json::json;

fn binding(target: &[&str], path: &[&str]) -> InputBinding {
    InputBinding {
        target_path: target.iter().map(|s| s.to_string()).collect(),
        source: InputBindingSource::DependencyOutput {
            node_id: TaskNodeId::new("a").unwrap(),
            path: path.iter().map(|s| s.to_string()).collect(),
        },
    }
}
fn context(value: Value) -> NodeContext {
    NodeContext::new(
        "goal",
        "instruction",
        vec![],
        vec![("a".into(), value)],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .unwrap()
}
fn resolve(
    literal: Value,
    bindings: Vec<InputBinding>,
    value: Value,
    schema: Value,
) -> Result<Value, InputBindingError> {
    TypedInputBindingResolver::resolve(&literal, &bindings, &context(value), Some(&schema))
}
#[test]
fn u31_binding_model_roundtrip_limits_and_collisions() {
    let item = binding(&["nested", "value"], &["structured", "value"]);
    assert_eq!(
        serde_json::from_value::<InputBinding>(serde_json::to_value(&item).unwrap()).unwrap(),
        item
    );
    assert!(bindings_from_input(&json!({"capability_bindings":[{"target_path":["v"],"source":{"kind":"secret","node_id":"a","path":["v"]}}]})).is_err());
    assert!(validate_set(&vec![item.clone(); 65]).is_err());
    for path in [
        vec![],
        vec!["".into()],
        vec!["*".into()],
        vec!["a\nb".into()],
        vec!["x".repeat(129)],
        vec!["x".into(); 17],
    ] {
        assert!(validate_path(&path).is_err());
    }
    assert_eq!(
        validate_set(&[item.clone(), item.clone()])
            .unwrap_err()
            .code,
        "duplicate_binding_target"
    );
    assert_eq!(
        validate_set(&[item, binding(&["nested"], &["structured"])])
            .unwrap_err()
            .code,
        "binding_target_collision"
    );
}
#[test]
fn u31_resolver_nested_override_missing_null_and_no_array_indexes() {
    let b = binding(&["nested", "value"], &["structured", "v"]);
    assert_eq!(
        resolve(
            json!({"nested":{"value":"literal"},"keep":true}),
            vec![b.clone()],
            json!({"structured":{"v":42}}),
            json!({"type":"object","required":["nested"]})
        )
        .unwrap(),
        json!({"nested":{"value":42},"keep":true})
    );
    assert_eq!(
        resolve(
            json!({}),
            vec![b.clone()],
            json!({"structured":{}}),
            json!({"type":"object"})
        )
        .unwrap_err()
        .code,
        "input_binding_path_missing"
    );
    let empty = context(json!({}));
    let no_sources = NodeContext {
        dependency_outputs: Default::default(),
        ..empty
    };
    assert_eq!(
        TypedInputBindingResolver::resolve(&json!({}), &[b.clone()], &no_sources, Some(&json!({})))
            .unwrap_err()
            .code,
        "input_binding_source_missing"
    );
    assert_eq!(
        resolve(
            json!({}),
            vec![b],
            json!({"structured":{"v":null}}),
            json!({"properties":{"nested":{"properties":{"value":{"type":"null"}}}}})
        )
        .unwrap(),
        json!({"nested":{"value":null}})
    );
    assert_eq!(
        resolve(
            json!({}),
            vec![binding(&["v"], &["structured", "items", "0"])],
            json!({"structured":{"items":[1]}}),
            json!({})
        )
        .unwrap_err()
        .code,
        "input_binding_path_missing"
    );
}
#[test]
fn u31_schema_all_types_no_coercion_integer_required_enum() {
    assert!(super::super::binding_schema::validate_arguments(
        &json!(42.0),
        &json!({"type":"integer","enum":[42]})
    )
    .is_ok());
    assert!(super::super::binding_schema::validate_arguments(
        &json!(9007199254740993u64),
        &json!({"maximum":9007199254740992u64})
    )
    .is_err());
    let cases = [
        ("string", json!("value")),
        ("number", json!(1.5)),
        ("integer", json!(42)),
        ("integer", json!(42.0)),
        ("boolean", json!(true)),
        ("object", json!({"a":1})),
        ("array", json!([1, 2])),
        ("null", Value::Null),
    ];
    for (kind, value) in cases {
        assert!(resolve(
            json!({}),
            vec![binding(&["v"], &["structured", "v"])],
            json!({"structured":{"v":value}}),
            json!({"type":"object","properties":{"v":{"type":kind}},"required":["v"]})
        )
        .is_ok());
    }
    for (kind, value) in [
        ("integer", json!("42")),
        ("integer", json!(42.5)),
        ("string", json!(42)),
        ("boolean", json!("true")),
        ("string", Value::Null),
    ] {
        assert_eq!(
            resolve(
                json!({}),
                vec![binding(&["v"], &["structured", "v"])],
                json!({"structured":{"v":value}}),
                json!({"properties":{"v":{"type":kind}}})
            )
            .unwrap_err()
            .code,
            "input_binding_type_mismatch"
        );
    }
    for schema in [
        json!({"required":["missing"]}),
        json!({"properties":{"v":{"enum":["one","two"]}}}),
        json!({"properties":{"v":{"pattern":".*"}}}),
    ] {
        assert_eq!(
            resolve(
                json!({}),
                vec![binding(&["v"], &["structured", "v"])],
                json!({"structured":{"v":"bad"}}),
                schema
            )
            .unwrap_err()
            .code,
            "input_binding_schema_mismatch"
        );
    }
    assert!(super::super::binding_schema::validate_arguments(
        &json!({"v":[1,"2"]}),
        &json!({"properties":{"v":{"type":"array","items":{"type":"integer"}}}})
    )
    .is_err());
}
#[test]
fn u31_bindings_bounds_headers_and_empty_legacy_compatibility() {
    let literal = json!({"unknown":{"keep":true}});
    assert_eq!(
        resolve(
            literal.clone(),
            vec![],
            json!({}),
            json!({"required":["not-present"]})
        )
        .unwrap(),
        literal
    );
    assert_eq!(
        resolve(
            json!({}),
            vec![binding(&["v"], &["structured", "v"])],
            json!({"structured":{"v":"x".repeat(15900)}}),
            json!({"properties":{"token":{"x-mcp-header":"Authorization"}}})
        )
        .unwrap_err()
        .code,
        "header_unsupported"
    );
    let ctx = NodeContext {
        dependency_outputs: [("a".into(), json!({"structured":{"v":"x".repeat(17000)}}))].into(),
        ..context(json!({}))
    };
    assert_eq!(
        TypedInputBindingResolver::resolve(
            &json!({}),
            &[binding(&["v"], &["structured", "v"])],
            &ctx,
            Some(&json!({}))
        )
        .unwrap_err()
        .code,
        "input_binding_exceeds_limit"
    );
}
