use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_availability_errors_switch() {
        assert_eq!(
            classify(&LlmError::Provider {
                status: 429,
                code: None
            }),
            Some(FailureReason::RateLimit)
        );
        assert_eq!(
            classify(&LlmError::Provider {
                status: 400,
                code: None
            }),
            None
        );
        assert_eq!(
            classify(&LlmError::Provider {
                status: 403,
                code: Some("content_policy_violation".into())
            }),
            None
        );
        assert_eq!(
            classify(&LlmError::Provider {
                status: 404,
                code: Some("model_not_found".into())
            }),
            Some(FailureReason::ModelUnavailable)
        );
    }
}
#[cfg(test)]
mod scheduling_tests {
    use super::*;
    use crate::config::types::ModelConfig;
    use crate::integrations::secret::{InMemorySecretStore, SecretResolver};
    use axum::{response::IntoResponse, routing::post, Router};
    use std::sync::Mutex;
    async fn server(status: u16, stream: bool) -> (String, tokio::task::JoinHandle<()>) {
        let app=Router::new().route("/chat/completions",post(move || async move {if status!=200{return (axum::http::StatusCode::from_u16(status).unwrap(),[("content-type","application/json")],"{\"error\":{\"code\":\"test_error\"}}".to_string()).into_response();}if stream{(axum::http::StatusCode::OK,[("content-type","text/event-stream")],"data: {\"id\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"complete\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_string()).into_response()}else{(axum::http::StatusCode::OK,[("content-type","application/json")],"{\"id\":\"test\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"complete\"},\"finish_reason\":\"stop\"}]}".to_string()).into_response()}}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, task)
    }
    fn runtime(urls: Vec<String>, events: Arc<Mutex<Vec<ModelRouteEvent>>>) -> ModelPoolRuntime {
        let candidates = urls
            .into_iter()
            .enumerate()
            .map(|(i, url)| PoolCandidate {
                profile_id: format!("p{i}"),
                tier: i + 1,
                client: LlmClient::new(
                    &ModelConfig {
                        base_url: url,
                        api_key: "fixture".into(),
                        api_key_env: String::new(),
                        ..ModelConfig::default()
                    },
                    Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new()))),
                ),
            })
            .collect();
        ModelPoolRuntime::new(
            ModelPool {
                id: "pool".into(),
                label: "pool".into(),
                service_kind: "chat".into(),
                enabled: true,
                revision: 1,
                attempt_timeout_ms: 1000,
                total_timeout_ms: 3000,
                tiers: vec![],
            },
            candidates,
            "conversation".into(),
            Arc::new(move |e| events.lock().unwrap().push(e)),
        )
    }
    #[tokio::test]
    async fn ordered_invoke_switches_429_to_second_and_stops_on_400() {
        let (a, ta) = server(429, false).await;
        let (b, tb) = server(200, false).await;
        let events = Arc::new(Mutex::new(Vec::new()));
        let rt = runtime(vec![a, b.clone()], events.clone());
        assert!(rt.invoke(&[], &[], 10).await.is_ok());
        let e = events.lock().unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[1].phase, "switched");
        assert_eq!(e[1].to_tier, Some(2));
        drop(e);
        let (c, tc) = server(400, false).await;
        let events = Arc::new(Mutex::new(Vec::new()));
        assert!(runtime(vec![c, b], events.clone())
            .invoke(&[], &[], 10)
            .await
            .is_err());
        assert_eq!(events.lock().unwrap().len(), 1);
        ta.abort();
        tb.abort();
        tc.abort();
    }
    #[tokio::test]
    async fn streaming_switch_keeps_one_complete_accumulator() {
        let (a, ta) = server(503, false).await;
        let (b, tb) = server(200, true).await;
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut text = String::new();
        let result = runtime(vec![a, b], events.clone())
            .stream(&[], &[], 10, |t| text.push_str(t))
            .await
            .unwrap();
        assert_eq!(text, "complete");
        assert_eq!(result.content, "complete");
        assert_eq!(
            events.lock().unwrap()[1].reason,
            Some(FailureReason::ServiceError)
        );
        ta.abort();
        tb.abort();
    }
    #[tokio::test]
    async fn partial_stream_is_archived_and_new_accumulator_replaces_it() {
        let app=Router::new().route("/chat/completions",post(||async{(axum::http::StatusCode::OK,[("content-type","text/event-stream")],"data: {\"id\":\"partial\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"interrupted\"},\"finish_reason\":null}]}\n\n")}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (second, t2) = server(200, true).await;
        let events = Arc::new(Mutex::new(Vec::new()));
        let result = runtime(vec![url, second], events.clone())
            .stream(&[], &[], 10, |_| {})
            .await
            .unwrap();
        assert_eq!(result.content, "complete");
        let events = events.lock().unwrap();
        assert!(events[1].replace_content);
        assert_eq!(
            events[1].interrupted_content.as_deref(),
            Some("interrupted")
        );
        task.abort();
        t2.abort();
    }
    #[tokio::test]
    async fn total_budget_bounds_all_candidates_and_cancel_does_not_switch() {
        let app = Router::new().route(
            "/chat/completions",
            post(|| async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                "{}"
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut rt = runtime(vec![url.clone(), url.clone(), url.clone()], events.clone());
        rt.pool.attempt_timeout_ms = 25;
        rt.pool.total_timeout_ms = 40;
        let started = tokio::time::Instant::now();
        assert!(rt.invoke(&[], &[], 10).await.is_err());
        assert!(started.elapsed() < Duration::from_millis(250));
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.phase != "exhausted")
                .count()
                <= 2
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let rt = runtime(vec![url.clone(), url], events.clone());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), rt.invoke(&[], &[], 10))
                .await
                .is_err()
        );
        tokio::task::yield_now().await;
        assert_eq!(events.lock().unwrap().len(), 1);
        task.abort();
    }
}

#[cfg(test)]
mod cancellation_audit_tests {
    use super::*;
    use crate::{
        config::types::ModelConfig,
        db::{Database, LlmModelInput},
        integrations::{
            llm::usage::DatabaseUsageRecorder,
            secret::{InMemorySecretStore, SecretResolver},
        },
    };
    use axum::{routing::post, Router};
    #[tokio::test]
    async fn cancelled_pending_chat_attempt_has_end_time() {
        let started = Arc::new(tokio::sync::Notify::new());
        let notify = started.clone();
        let app = Router::new().route(
            "/chat/completions",
            post(move || {
                let notify = notify.clone();
                async move {
                    notify.notify_one();
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    "{}"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.create_llm_model(&LlmModelInput {
            id: "fixture".into(),
            provider: "fixture".into(),
            label: "fixture".into(),
            model: "fixture".into(),
            base_url: base.clone(),
            api_format: "openai".into(),
            api_key_ref: "llm.fixture".into(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 10,
            invoke_timeout_ms: 10000,
        })
        .unwrap();
        let client = LlmClient::new_with_usage_recorder(
            &ModelConfig {
                base_url: base,
                api_key: "fixture".into(),
                api_key_env: String::new(),
                ..Default::default()
            },
            Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new()))),
            Some(Arc::new(DatabaseUsageRecorder::new(
                db.clone_connection(),
                "fixture",
                "fixture",
                "fixture",
                "model_pool",
            ))),
        );
        let rt = ModelPoolRuntime::new(
            ModelPool {
                id: "pool".into(),
                label: "pool".into(),
                service_kind: "chat".into(),
                enabled: true,
                revision: 1,
                attempt_timeout_ms: 10000,
                total_timeout_ms: 10000,
                tiers: vec![],
            },
            vec![PoolCandidate {
                profile_id: "fixture".into(),
                tier: 1,
                client,
            }],
            "conversation".into(),
            Arc::new(|_| {}),
        );
        let call = tokio::spawn(async move { rt.invoke(&[], &[], 10).await });
        tokio::time::timeout(Duration::from_secs(3), started.notified())
            .await
            .unwrap();
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        let attempts = db.list_pool_attempts(None).unwrap();
        assert_eq!(attempts.len(), 1);
        assert!(attempts[0]["ended_at"].as_i64().is_some());
        assert_eq!(attempts[0]["failure"], "cancelled");
        assert!(attempts[0]["usage"].is_null());
        server.abort();
    }
}
#[cfg(test)]
mod business_error_tests {
    use super::*;
    #[test]
    fn registered_vendor_errors_switch_without_bypassing_content_refusal() {
        let quota = crate::integrations::llm::client::response_error(
            "minimax",
            200,
            &serde_json::json!({"base_resp":{"status_code":1008}}),
        )
        .unwrap();
        assert_eq!(classify(&quota), Some(FailureReason::Quota));
        let refusal = crate::integrations::llm::client::response_error(
            "minimax",
            200,
            &serde_json::json!({"base_resp":{"status_code":1026}}),
        )
        .unwrap();
        assert_eq!(classify(&refusal), None);
        let ali = crate::integrations::llm::client::response_error(
            "qwen",
            403,
            &serde_json::json!({"code":"AllocationQuota.FreeTierOnly"}),
        )
        .unwrap();
        assert_eq!(classify(&ali), Some(FailureReason::Quota));
        assert_eq!(
            classify(&LlmError::Provider {
                status: 403,
                code: Some("DataInspectionFailed".into())
            }),
            None
        );
    }
}
