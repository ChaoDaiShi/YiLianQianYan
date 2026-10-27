use super::*;
use crate::modules::knowledge_build::{
    prompts::{ASSESSMENT_PROMPT, CLAIMS_PROMPT, FOLLOWUP_PROMPT, STRUCTURE_PROMPT},
    stages::{AssessmentsDraft, ClaimsDraft, StructureDraft},
    QueryPlan,
};
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

fn example(prompt: &str) -> &str {
    prompt.lines().find(|line| line.starts_with('{')).unwrap()
}

#[test]
fn staged_prompt_examples_match_their_independent_contracts() {
    let structure: StructureDraft = parse(
        completion(example(STRUCTURE_PROMPT), "stop"),
        "知识结构",
        4096,
    )
    .unwrap();
    assert_eq!(structure.nodes.len(), 2);
    assert_eq!(
        structure.nodes[1].parent_id.as_deref(),
        Some(structure.nodes[0].id.as_str())
    );
    let claims: ClaimsDraft =
        parse(completion(example(CLAIMS_PROMPT), "stop"), "论点", 4096).unwrap();
    assert_eq!(claims.claims.len(), 1);
    assert_eq!(claims.claims[0].evidence.len(), 1);
    let assessments: AssessmentsDraft =
        parse(completion(example(ASSESSMENT_PROMPT), "stop"), "来源", 4096).unwrap();
    assert_eq!(assessments.source_assessments.len(), 1);
    let query_example = FOLLOWUP_PROMPT
        .split_once('{')
        .unwrap()
        .1
        .split_once('}')
        .unwrap()
        .0;
    let plan: QueryPlan = parse(
        completion(&format!("{{{query_example}}}"), "stop"),
        "查询",
        512,
    )
    .unwrap();
    assert_eq!(plan.queries.len(), 4);
}

#[test]
fn stages_reject_misplaced_fields_instead_of_discarding_or_moving_them() {
    let mut output: serde_json::Value = serde_json::from_str(example(STRUCTURE_PROMPT)).unwrap();
    output["source_assessments"] = json!([]);
    output["next_queries"] = json!([]);
    let error = parse::<StructureDraft>(
        completion(&output.to_string(), "stop"),
        "第1轮知识结构",
        4096,
    )
    .err()
    .unwrap();
    assert!(error.contains("未约定字段"));
    assert!(
        error.contains("仅允许字段 summary、nodes、edges、limitations"),
        "{error}"
    );
    assert!(!error.contains("source_assessments"));
    let mut claims: serde_json::Value = serde_json::from_str(example(CLAIMS_PROMPT)).unwrap();
    claims["claims"][0]["status"] = json!("corroborated");
    assert!(parse::<ClaimsDraft>(completion(&claims.to_string(), "stop"), "论点", 4096).is_err());
    let mut assessments: serde_json::Value =
        serde_json::from_str(example(ASSESSMENT_PROMPT)).unwrap();
    assessments["result"] = json!({});
    assert!(
        parse::<AssessmentsDraft>(completion(&assessments.to_string(), "stop"), "来源", 4096)
            .is_err()
    );
    let duplicated = r#"{"source_assessments":[],"source_assessments":[]}"#;
    assert!(parse::<AssessmentsDraft>(completion(duplicated, "stop"), "来源", 4096).is_err());
}

#[test]
fn staged_outputs_require_their_own_fields_and_arrays() {
    let mut output: serde_json::Value = serde_json::from_str(example(CLAIMS_PROMPT)).unwrap();
    output.as_object_mut().unwrap().remove("contradictions");
    assert!(
        parse::<ClaimsDraft>(completion(&output.to_string(), "stop"), "论点", 4096)
            .err()
            .unwrap()
            .contains("缺少必填字段 contradictions")
    );
    let mut output: serde_json::Value = serde_json::from_str(example(ASSESSMENT_PROMPT)).unwrap();
    output["source_assessments"][0]
        .as_object_mut()
        .unwrap()
        .remove("basis_id");
    assert!(
        parse::<AssessmentsDraft>(completion(&output.to_string(), "stop"), "来源", 4096)
            .err()
            .unwrap()
            .contains("缺少必填字段 basis_id")
    );
    let mut output: serde_json::Value = serde_json::from_str(example(STRUCTURE_PROMPT)).unwrap();
    output["nodes"][0]["source_ids"] = json!("s1");
    assert!(
        parse::<StructureDraft>(completion(&output.to_string(), "stop"), "知识结构", 4096).is_err()
    );
}

#[test]
fn source_classification_selects_retained_evidence_instead_of_retyping_quotations() {
    let output = r#"{"source_assessments":[{"source_id":"s6","source_type":"paper","reason":"材料给出了论文作者与年份","basis_id":"excerpt-1"}]}"#;
    assert!(parse::<AssessmentsDraft>(completion(output, "stop"), "来源", 4096).is_ok());
}
