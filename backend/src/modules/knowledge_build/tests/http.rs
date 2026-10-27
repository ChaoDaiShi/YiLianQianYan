use super::super::*;

#[derive(Default)]
struct ResearchHttpTrace {
    queries: std::sync::Mutex<Vec<(String, usize)>>,
    plans: std::sync::Mutex<Vec<String>>,
    syntheses: std::sync::Mutex<Vec<(String, usize)>>,
    learning_calls: std::sync::Mutex<Vec<String>>,
    fenced_syntheses: std::sync::Mutex<Vec<usize>>,
    stages: std::sync::Mutex<Vec<(String, String, usize)>>,
    cancel_search_started: tokio::sync::Notify,
    release_cancel_search: tokio::sync::Notify,
}

async fn research_mock_search(
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

async fn research_mock_llm(
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
                json!({"queries":(0..4).map(|index|format!("{topic}::round-{round}::query-{index}")).collect::<Vec<_>>()})
            }
            _ => panic!("Unexpected synthesis stage"),
        }
    } else {
        trace.plans.lock().unwrap().push(topic.into());
        json!({"queries":(0..4).map(|index|format!("{topic}::round-0::query-{index}")).collect::<Vec<_>>()})
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

#[tokio::test]
async fn configured_http_research_runs_quick_and_deep_imports_real_text_and_cancels_once() {
    use crate::integrations::secret::{InMemorySecretStore, SecretRef, SecretStore};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::post,
        Router,
    };
    use secrecy::SecretString;
    use tower::ServiceExt;

    let trace = Arc::new(ResearchHttpTrace::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let mock = Router::new()
        .route("/search", post(research_mock_search))
        .route("/v1/chat/completions", post(research_mock_llm))
        .with_state(Arc::clone(&trace));
    let mock_task = tokio::spawn(async move {
        axum::serve(listener, mock).await.unwrap();
    });

    let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let model_ref = SecretRef::new("knowledge.fixture.model");
    let search_ref = SecretRef::new("knowledge.fixture.search");
    store
        .put(
            &model_ref,
            SecretString::from("local-model-fixture".to_string()),
        )
        .await
        .unwrap();
    store
        .put(
            &search_ref,
            SecretString::from("local-search-fixture".to_string()),
        )
        .await
        .unwrap();
    let root = std::env::temp_dir().join(format!("knowledge-http-{}", uuid::Uuid::new_v4()));
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    // Inject the memory store at construction: no OsSecretStore is created or read.
    let server = Arc::new(
        AppServer::new_with_control_session_and_store(
            &root.join("fixture.db"),
            workspace.to_str().unwrap(),
            crate::safety::ControlSession::generate(),
            Arc::clone(&store),
        )
        .unwrap(),
    );
    {
        let mut config = server.config.write();
        config.model.base_url = "http://127.0.0.1:1".into();
        config.model.name = "unconfigured-legacy-must-not-run".into();
        config.model.provider = "openai".into();
        config.model.api_key.clear();
        config.model.api_key_env.clear();
        config.model.api_key_ref = None;
        config.model.invoke_timeout_ms = 10_000;
    }
    server
        .db
        .create_llm_model(&crate::db::LlmModelInput {
            id: "knowledge-pool-profile".into(),
            provider: "openai".into(),
            label: "local pool profile".into(),
            model: "knowledge-pool-fixture".into(),
            base_url: format!("{base}/v1"),
            api_format: "openai".into(),
            api_key_ref: model_ref.key.clone(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 16_384,
            invoke_timeout_ms: 10_000,
        })
        .unwrap();
    server
        .db
        .save_model_pool(
            "knowledge-default-pool",
            crate::db::model_pools::ModelPoolInput {
                label: "local default pool".into(),
                service_kind: "chat".into(),
                enabled: true,
                attempt_timeout_ms: 10_000,
                total_timeout_ms: 10_000,
                tiers: vec![crate::db::model_pools::ModelPoolTier {
                    id: "primary".into(),
                    label: "primary".into(),
                    profile_ids: vec!["knowledge-pool-profile".into()],
                }],
                expected_revision: None,
            },
        )
        .unwrap();
    server
        .db
        .set_model_pool_binding("default", Some("knowledge-default-pool"))
        .unwrap();
    assert!(server.db.get_active_llm_model().unwrap().is_none());
    server
        .db
        .save_search_provider("tavily", &base, Some(&search_ref))
        .unwrap();
    let import_router = crate::api::knowledge_build::router().with_state(Arc::clone(&server));

    for (mode, prefix, query_count, synthesis_counts) in [
        (BuildMode::Quick, "quick", 4, vec![12usize]),
        (BuildMode::Deep, "deep", 12, vec![13usize, 26, 40]),
    ] {
        let marker = format!("{prefix}-unique-{}", uuid::Uuid::new_v4());
        let build = KnowledgeBuild::new(marker.clone(), mode);
        server.db.create_knowledge_build(&build).unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(20),
            run(Arc::clone(&server), build.clone()),
        )
        .await
        .unwrap();
        let completed = server.db.knowledge_build(&build.id).unwrap().unwrap();
        assert_eq!(completed.status, "completed", "{:?}", completed.error);
        assert_eq!(completed.progress.queries_completed, query_count);
        assert_eq!(completed.progress.rounds_completed, mode.rounds());
        assert_eq!(completed.sources.len(), mode.source_limit());
        let queries = trace
            .queries
            .lock()
            .unwrap()
            .iter()
            .filter(|(query, _)| query.starts_with(&marker))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(queries.len(), query_count);
        assert_eq!(
            queries
                .iter()
                .map(|(query, _)| query)
                .collect::<HashSet<_>>()
                .len(),
            query_count
        );
        for (query, quota) in &queries {
            assert_eq!(
                completed
                    .sources
                    .iter()
                    .filter(|source| source.query == *query)
                    .count(),
                *quota
            );
        }
        let syntheses = trace
            .syntheses
            .lock()
            .unwrap()
            .iter()
            .filter(|(topic, _)| topic == &marker)
            .map(|(_, count)| *count)
            .collect::<Vec<_>>();
        assert_eq!(syntheses, synthesis_counts);
        let stages: Vec<_> = trace
            .stages
            .lock()
            .unwrap()
            .iter()
            .filter(|(topic, _, _)| topic == &marker)
            .cloned()
            .collect();
        let expected_stages: Vec<_> = (0..mode.rounds())
            .flat_map(|round| {
                let count = if mode == BuildMode::Quick {
                    12
                } else {
                    [13, 26, 40][round]
                };
                let new_count = if mode == BuildMode::Quick {
                    12
                } else {
                    [13, 13, 14][round]
                };
                let mut stages = vec![
                    (marker.clone(), "structure".to_string(), count),
                    (marker.clone(), "claims".to_string(), count),
                    (marker.clone(), "source_assessments".to_string(), new_count),
                ];
                if round + 1 < mode.rounds() {
                    stages.push((marker.clone(), "followup_queries".to_string(), 0));
                }
                stages
            })
            .collect();
        assert_eq!(stages, expected_stages);
        assert_eq!(
            trace
                .plans
                .lock()
                .unwrap()
                .iter()
                .filter(|topic| *topic == &marker)
                .count(),
            1
        );

        // Exercise the real import handler, ResourceService storage, DB import,
        // original document search, and the repeated-import idempotent response.
        let mut imported_document = None;
        for expected_created in [true, false] {
            let response = import_router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/knowledge/builds/{}/import", build.id))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap();
            let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(result["created"], expected_created);
            let document_id = result["document"]["id"].as_str().unwrap().to_string();
            if let Some(previous) = &imported_document {
                assert_eq!(previous, &document_id);
            }
            imported_document = Some(document_id.clone());
            let text = server.db.knowledge_text(&document_id).unwrap().unwrap();
            assert!(text.contains(&marker));
            assert!(server
                .db
                .search_knowledge(&marker)
                .unwrap()
                .iter()
                .any(|hit| hit.document_id == document_id));
            let resource_id = result["document"]["resource_id"].as_str().unwrap();
            assert_eq!(
                server.resource_service.read_bytes(resource_id).unwrap(),
                text.as_bytes()
            );
            assert_eq!(
                server
                    .db
                    .knowledge_build_for_document(&document_id)
                    .unwrap(),
                Some(build.id.clone())
            );
        }
    }

    assert_eq!(*trace.fenced_syntheses.lock().unwrap(), vec![13, 26, 40]);
    for (topic, expected_error) in [
        ("truncated-fixture", "输出被截断"),
        ("missing-field-fixture", "缺少必填字段 nodes"),
        ("mixed-prose-fixture", "JSON语法无效"),
        ("misplaced-field-fixture", "未约定字段"),
        ("invalid-hierarchy-fixture", "循环"),
        ("fabricated-quote-fixture", "缺失证据片段"),
        ("invalid-assessment-fixture", "可在实际标题或摘录定位"),
    ] {
        let build = KnowledgeBuild::new(topic.into(), BuildMode::Deep);
        server.db.create_knowledge_build(&build).unwrap();
        run(Arc::clone(&server), build.clone()).await;
        let failed = server.db.knowledge_build(&build.id).unwrap().unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.progress.queries_completed, 4);
        assert_eq!(failed.sources.len(), 13);
        assert_eq!(failed.progress.rounds_completed, 0);
        let error = failed.error.unwrap();
        assert!(error.contains(expected_error), "{error}");
        let stages: Vec<_> = trace
            .stages
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _, _)| name == topic)
            .map(|(_, stage, _)| stage.clone())
            .collect();
        let expected = match topic {
            "fabricated-quote-fixture" => vec!["structure", "claims"],
            "invalid-assessment-fixture" => vec!["structure", "claims", "source_assessments"],
            _ => vec!["structure"],
        };
        assert_eq!(
            stages, expected,
            "Failure must stop before the next stage or another model request"
        );
        assert!(failed.result.is_none());
        assert_eq!(
            trace
                .syntheses
                .lock()
                .unwrap()
                .iter()
                .filter(|(name, _)| name == topic)
                .count(),
            1
        );
        assert_eq!(
            trace
                .queries
                .lock()
                .unwrap()
                .iter()
                .filter(|(query, _)| query.starts_with(topic))
                .count(),
            4
        );
    }

    // The learning entry point must resolve the same pool even though there is
    // no active profile and the legacy configuration cannot make a request.
    let workspace = crate::workspace::Workspace::new(
        crate::workspace::WorkspaceId::generate(),
        "learning fixture".into(),
        String::new(),
        None,
        1,
    )
    .unwrap();
    server.db.create_workspace(&workspace).unwrap();
    let document = server.db.knowledge_documents().unwrap().remove(0);
    let learning = server
        .db
        .create_learning_session(
            workspace.id.as_str(),
            "learning-pool-fixture",
            &[crate::modules::learning::LearningSource {
                document_id: document.id,
                resource_id: document.resource_id,
                name: document.name.clone(),
                hash: document.hash,
            }],
        )
        .unwrap();
    crate::modules::learning::generate(&server, &learning)
        .await
        .unwrap();
    assert_eq!(
        server
            .db
            .learning_session(workspace.id.as_str(), &learning.id)
            .unwrap()
            .status,
        "ready"
    );
    let notes = server.db.learning_notes(&learning.id).unwrap();
    assert_eq!(notes.len(), 1);
    let note_text = String::from_utf8(
        server
            .resource_service
            .read_bytes(&notes[0].resource_id)
            .unwrap(),
    )
    .unwrap();
    assert!(note_text.contains(&document.name));
    assert_eq!(
        *trace.learning_calls.lock().unwrap(),
        vec!["learning-pool-fixture"]
    );

    // Keep an actual search request in flight, cancel twice, then explicitly
    // attempt to run the same cancelled build. No subsequent query or synthesis
    // may execute and the persisted cancellation must not be overwritten.
    let cancelled_build = KnowledgeBuild::new("cancel-fixture".into(), BuildMode::Deep);
    server.db.create_knowledge_build(&cancelled_build).unwrap();
    let worker = tokio::spawn(run(Arc::clone(&server), cancelled_build.clone()));
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        trace.cancel_search_started.notified(),
    )
    .await
    .unwrap();
    for _ in 0..2 {
        assert_eq!(
            server
                .db
                .cancel_knowledge_build(&cancelled_build.id)
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
    trace.release_cancel_search.notify_one();
    run(Arc::clone(&server), cancelled_build.clone()).await;
    assert_eq!(
        server
            .db
            .knowledge_build(&cancelled_build.id)
            .unwrap()
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        trace
            .queries
            .lock()
            .unwrap()
            .iter()
            .filter(|(query, _)| query.starts_with("cancel-fixture::"))
            .count(),
        1
    );
    assert_eq!(
        trace
            .plans
            .lock()
            .unwrap()
            .iter()
            .filter(|topic| topic.as_str() == "cancel-fixture")
            .count(),
        1
    );
    assert!(!trace
        .syntheses
        .lock()
        .unwrap()
        .iter()
        .any(|(topic, _)| topic == "cancel-fixture"));
    mock_task.abort();
}
