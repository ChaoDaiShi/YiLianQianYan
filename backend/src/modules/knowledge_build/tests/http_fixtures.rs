use super::*;

#[derive(Default)]
pub(super) struct ResearchHttpTrace {
    pub(super) queries: std::sync::Mutex<Vec<(String, usize)>>,
    pub(super) plans: std::sync::Mutex<Vec<String>>,
    pub(super) syntheses: std::sync::Mutex<Vec<(String, usize)>>,
    pub(super) learning_calls: std::sync::Mutex<Vec<String>>,
    pub(super) fenced_syntheses: std::sync::Mutex<Vec<usize>>,
    pub(super) stages: std::sync::Mutex<Vec<(String, String, usize)>>,
    pub(super) cancel_search_started: tokio::sync::Notify,
    pub(super) release_cancel_search: tokio::sync::Notify,
}

pub(super) async fn research_mock_search(
    axum::extract::State(trace): axum::extract::State<Arc<ResearchHttpTrace>>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    assert_eq!(headers["authorization"], "Bearer local-search-fixture");
    let query = body["query"].as_str().unwrap().to_string();
    let count = body["max_results"].as_u64().unwrap() as usize;
    let request_index = {
        let mut queries = trace.queries.lock().unwrap();
        queries.push((query.clone(), count));
        queries.len()
    };
    if query.starts_with("cancel-fixture::") {
        trace.cancel_search_started.notify_one();
        trace.release_cancel_search.notified().await;
    }
    axum::Json(json!({
        "request_id":format!("search-{request_index}"),
        "results":(0..count).map(|index|json!({
            "title":format!("教材章节 {request_index}-{index}"),
            "url":format!("https://search-fixture.invalid/{request_index}/{index}"),
            "content":format!("{query} 的来源标记 source-{request_index}-{index}。"),
        })).collect::<Vec<_>>()
    }))
}

pub(super) async fn research_mock_llm(
    axum::extract::State(trace): axum::extract::State<Arc<ResearchHttpTrace>>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    assert_eq!(headers["authorization"], "Bearer local-model-fixture");
    assert_eq!(body["stream"], false);
    assert_eq!(body["model"], "knowledge-pool-fixture");
    let input: serde_json::Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    let topic = input["topic"].as_str().unwrap();
    let learning_request = input.pointer("/sources/0/document_id").is_some();
    if learning_request {
        assert!(body.get("response_format").is_none());
    } else {
        assert_eq!(body["response_format"], json!({"type":"json_object"}));
        assert_eq!(body["max_tokens"], input["output_token_budget"]);
    }
    let mut payload = if learning_request {
        trace.learning_calls.lock().unwrap().push(topic.into());
        let source = &input["sources"][0];
        let quote = source["excerpt"]
            .as_str()
            .unwrap()
            .chars()
            .take(80)
            .collect::<String>();
        json!({
            "notes":format!("学习笔记：{quote}"),
            "note_sources":[{"document_id":source["document_id"],"quote":quote}],
            "exercises":[{"question":"以下哪项来自所选资料？","options":[{"id":"a","label":quote},{"id":"b","label":"没有出现在资料中的内容"}],"correct_option_id":"a","explanation":quote,"sources":[{"document_id":source["document_id"],"quote":quote}]}]
        })
    } else if let Some(stage) = input["stage"].as_str() {
        let sources = input["sources"].as_array().cloned().unwrap_or_default();
        trace
            .stages
            .lock()
            .unwrap()
            .push((topic.into(), stage.into(), sources.len()));
        let round = if stage == "structure" {
            let mut syntheses = trace.syntheses.lock().unwrap();
            let round = syntheses
                .iter()
                .filter(|(existing, _)| existing == topic)
                .count()
                + 1;
            syntheses.push((topic.into(), sources.len()));
            round
        } else {
            trace
                .syntheses
                .lock()
                .unwrap()
                .iter()
                .filter(|(existing, _)| existing == topic)
                .count()
        };
        match stage {
            "structure" => {
                assert!(input.get("previous_result").is_none());
                let first = sources.first().unwrap();
                let last = sources.last().unwrap();
                json!({
                "summary":format!("{topic} 第{round}轮整理"),
                "nodes":[
                    {"id":"root","parent_id":null,"title":topic,"summary":first["excerpt"],"source_ids":[first["id"]]},
                    {"id":"child","parent_id":"root","title":"本轮新增资料","summary":last["excerpt"],"source_ids":[last["id"]]}
                ],
                "edges":[],"limitations":[]
                })
            }
            "claims" => {
                assert!(input["structure"]["nodes"].is_array());
                let first = sources.first().unwrap();
                let quote_id = if topic == "fabricated-quote-fixture" {
                    json!("nonexistent-evidence")
                } else {
                    first["quotes"][0]["id"].clone()
                };
                json!({"claims":[{"id":"claim","text":first["quotes"][0]["text"],"evidence":[{"source_id":first["id"],"quote_id":quote_id,"stance":"supports"}]}],"contradictions":[]})
            }
            "source_assessments" => json!({
                "source_assessments":sources.iter().map(|source|json!({"source_id":source["id"],"source_type":"textbook","reason":"标题标明教材章节","basis_id":if topic == "invalid-assessment-fixture" {json!("nonexistent-basis")} else {source["basis"][0]["id"].clone()}})).collect::<Vec<_>>()
            }),
            "followup_queries" => {
                assert!(input["result"]["claims"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|claim| claim["status"] == "single_source"));
                json!({"queries":(0..input["query_count"].as_u64().unwrap()).map(|index|format!("{topic}::round-{round}::query-{index}")).collect::<Vec<_>>()})
            }
            _ => panic!("Unexpected synthesis stage"),
        }
    } else {
        trace.plans.lock().unwrap().push(topic.into());
        json!({"queries":(0..input["query_count"].as_u64().unwrap()).map(|index|format!("{topic}::round-0::query-{index}")).collect::<Vec<_>>()})
    };
    let synthesis = input["stage"] == "structure";
    if synthesis && topic == "missing-field-fixture" {
        payload.as_object_mut().unwrap().remove("nodes");
    }
    if synthesis && topic == "misplaced-field-fixture" {
        payload["source_assessments"] = json!([]);
    }
    if synthesis && topic == "invalid-hierarchy-fixture" {
        payload["nodes"][0]["parent_id"] = json!("child");
    }
    let (content, finish_reason) = if synthesis && topic == "truncated-fixture" {
        let mut content = payload.to_string();
        content.pop();
        (content, "length")
    } else if synthesis && topic == "mixed-prose-fixture" {
        (format!("这里是结果：\n{}", payload), "stop")
    } else if synthesis && topic.starts_with("deep-") {
        trace
            .fenced_syntheses
            .lock()
            .unwrap()
            .push(input["sources"].as_array().unwrap().len());
        (format!("```json\n{}\n```", payload), "stop")
    } else {
        (payload.to_string(), "stop")
    };
    axum::Json(json!({
        "id":"local-knowledge-completion",
        "choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":finish_reason}],
        "usage":{"prompt_tokens":30,"completion_tokens":40,"total_tokens":70}
    }))
}
