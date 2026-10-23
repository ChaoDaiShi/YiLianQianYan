use super::*;
use crate::modules::knowledge_build::{QueryPlan, Synthesis, SYNTHESIS_PROMPT};
use serde_json::json;

fn completion(content: &str, finish: &str) -> ChatCompletionResponse {
    serde_json::from_value(json!({
        "id":"local-response-fixture",
        "choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":finish}],
        "usage":{"prompt_tokens":30,"completion_tokens":40,"total_tokens":70}
    })).unwrap()
}

#[test]
fn accepts_only_one_complete_json_object_or_a_single_complete_json_fence() {
    let object = r#"{"queries":["one","two","three","four"]}"#;
    for text in [
        object.to_string(),
        format!("```json\n{object}\n```"),
        format!("```json\r\n{object}\r\n```"),
        format!("```\n{object}\n```"),
    ] {
        let plan: QueryPlan = parse(completion(&text, "stop"), "规划", 4096).unwrap();
        assert_eq!(plan.queries.len(), 4);
    }
    for text in [
        format!("说明\n{object}"),
        format!("```json\n{object}\n```\n后记"),
        format!("```json\n{object}\n```\n```json\n{object}\n```"),
        format!("```json\n{object}"),
        r#"{"queries":["one" "two"]}"#.into(),
        "[]".into(),
        serde_json::to_string(object).unwrap(),
    ] {
        assert!(parse::<QueryPlan>(completion(&text, "stop"), "规划", 4096).is_err());
    }
}

#[test]
fn truncated_filtered_empty_and_schema_invalid_outputs_have_distinct_safe_errors() {
    let text = r#"{"queries":["one","two","three","four"]}"#;
    let truncated = parse::<QueryPlan>(completion(text, "length"), "第1轮综合", 4096)
        .err()
        .unwrap();
    assert!(truncated.contains("输出被截断"));
    assert!(truncated.contains("结束原因=length"));
    assert!(truncated.contains("40 token"));
    assert!(truncated.contains("4096 token"));
    assert!(
        parse::<QueryPlan>(completion("", "content_filter"), "规划", 4096)
            .err()
            .unwrap()
            .contains("过滤")
    );
    assert!(parse::<QueryPlan>(completion("", "stop"), "规划", 4096)
        .err()
        .unwrap()
        .contains("空正文"));
    assert!(parse::<QueryPlan>(completion("{}", "stop"), "规划", 4096)
        .err()
        .unwrap()
        .contains("缺少必填字段 queries"));
    let invalid = parse::<QueryPlan>(
        completion("PRIVATE_SOURCE_BODY not JSON", "stop"),
        "规划",
        4096,
    )
    .err()
    .unwrap();
    assert!(invalid.contains("JSON语法无效"));
    assert!(!invalid.contains("PRIVATE_SOURCE_BODY"));
    let extra = parse::<QueryPlan>(
        completion(r#"{"queries":[],"PRIVATE_SOURCE_FIELD":true}"#, "stop"),
        "规划",
        4096,
    )
    .err()
    .unwrap();
    assert!(extra.contains("未约定字段"));
    assert!(!extra.contains("PRIVATE_SOURCE_FIELD"));
}

#[test]
fn synthesis_example_has_complete_fields_and_consistent_graph_references() {
    let example = SYNTHESIS_PROMPT
        .lines()
        .find(|line| line.starts_with("{\"result\""))
        .unwrap();
    let synthesis: Synthesis = parse(completion(example, "stop"), "综合", 16384).unwrap();
    let ids = synthesis
        .result
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(synthesis
        .result
        .edges
        .iter()
        .all(|edge| ids.contains(edge.from.as_str()) && ids.contains(edge.to.as_str())));
    assert!(synthesis.result.contradictions.is_empty());
    assert_eq!(synthesis.next_queries.len(), 4);
    let mut missing: serde_json::Value = serde_json::from_str(example).unwrap();
    missing.as_object_mut().unwrap().remove("next_queries");
    assert!(
        parse::<Synthesis>(completion(&missing.to_string(), "stop"), "综合", 16384)
            .err()
            .unwrap()
            .contains("缺少必填字段 next_queries")
    );
    let mut missing: serde_json::Value = serde_json::from_str(example).unwrap();
    missing["source_assessments"][0]
        .as_object_mut()
        .unwrap()
        .remove("basis_quote");
    assert!(
        parse::<Synthesis>(completion(&missing.to_string(), "stop"), "综合", 16384)
            .err()
            .unwrap()
            .contains("缺少必填字段 basis_quote")
    );
}
