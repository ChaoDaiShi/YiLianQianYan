use super::*;
use axum::{routing::post, Json, Router};
use std::sync::Arc;

#[cfg(test)]
mod service_tests {
    use super::*;
    use crate::integrations::secret::{InMemorySecretStore, SecretRef, SecretStore};
    fn profile(kind: &str, adapter: &str) -> BasicServiceProfile {
        BasicServiceProfile {
            id: "profile".into(),
            label: "fixture".into(),
            provider_account_id: "account".into(),
            service_kind: kind.into(),
            model: "fixture-model".into(),
            adapter: adapter.into(),
            parameters: json!({}),
            revision: 1,
            test: None,
        }
    }
    #[tokio::test]
    async fn real_http_multimodal_and_image_adapters() {
        for (kind, adapter, path) in [
            ("vision", "openai", "/chat/completions"),
            ("image", "openai", "/images/generations"),
            ("image", "minimax", "/image_generation"),
            (
                "image",
                "dashscope",
                "/api/v1/services/aigc/multimodal-generation/generation",
            ),
        ] {
            let kind_owned = kind.to_owned();
            let adapter_owned = adapter.to_owned();
            let app=Router::new().route(path,post(move|Json(body):Json<Value>|{let kind=kind_owned.clone();let adapter=adapter_owned.clone();async move{assert_eq!(body["model"],"fixture-model");let result=if kind=="vision"{assert_eq!(body["messages"][0]["content"][1]["image_url"]["url"],"data:image/png;base64,fixture");json!({"choices":[{"message":{"content":"真实响应"}}]})}else if adapter=="minimax"{assert_eq!(body["prompt"],"fixture prompt");json!({"data":{"image_urls":["https://example.invalid/image.png"]},"base_resp":{"status_code":0}})}else if adapter=="dashscope"{assert_eq!(body["input"]["messages"][0]["content"][0]["text"],"fixture prompt");json!({"output":{"choices":[{"message":{"content":[{"image":"https://example.invalid/image.png"}]}}]}})}else{assert_eq!(body["prompt"],"fixture prompt");json!({"data":[{"b64_json":"fixture"}]})};Json(result)}}));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let db = Database::new(
                &std::env::temp_dir().join(format!("yilian-service-{}.db", uuid::Uuid::new_v4())),
            )
            .unwrap();
            db.init_provider_accounts().unwrap();
            db.init_basic_services().unwrap();
            db.save_provider_account(&crate::db::model_pools::ProviderAccount {
                id: "account".into(),
                provider_id: "fixture".into(),
                label: "fixture".into(),
                base_url: base,
                credential_id: Some("llm.fixture".into()),
                profile_ids: vec![],
            })
            .unwrap();
            let store = Arc::new(InMemorySecretStore::new());
            store
                .put(
                    &SecretRef::new("llm.fixture"),
                    secrecy::SecretString::from("fixture"),
                )
                .await
                .unwrap();
            let resolver = SecretResolver::new(store);
            let result = invoke_profile(
                &db,
                &resolver,
                &profile(kind, adapter),
                BasicServiceRequest {
                    prompt: "fixture prompt".into(),
                    image_url: Some("data:image/png;base64,fixture".into()),
                },
            )
            .await
            .unwrap();
            assert_eq!(result["service_kind"], kind);
            task.abort();
        }
    }
}
#[cfg(test)]
mod pool_tests {
    use super::*;
    use crate::{
        db::{
            basic_services::BasicServiceInput,
            model_pools::{ModelPoolInput, ModelPoolTier, ProviderAccount},
        },
        integrations::secret::{InMemorySecretStore, SecretRef, SecretStore},
    };
    #[tokio::test]
    async fn image_pool_http_downgrade_preserves_attempt_order() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.init_basic_services().unwrap();
        db.init_model_pools().unwrap();
        db.init_provider_accounts().unwrap();
        let store = Arc::new(InMemorySecretStore::new());
        let mut tasks = Vec::new();
        for (index, status) in [(1, 429), (2, 200)] {
            let app=Router::new().route("/images/generations",post(move||async move{(axum::http::StatusCode::from_u16(status).unwrap(),Json(if status==200{json!({"data":[{"url":"https://example.invalid/generated.png"}],"usage":{"image_count":1}})}else{json!({"error":{"code":"rate_limit_exceeded"}})}))}));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            tasks.push(tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap()
            }));
            let id = format!("account-{index}");
            let key = format!("llm.{id}");
            store
                .put(
                    &SecretRef::new(&key),
                    secrecy::SecretString::from("fixture"),
                )
                .await
                .unwrap();
            db.save_provider_account(&ProviderAccount {
                id: id.clone(),
                provider_id: "fixture".into(),
                label: id.clone(),
                base_url: base,
                credential_id: Some(key),
                profile_ids: vec![],
            })
            .unwrap();
            db.save_basic_service_profile(
                &format!("profile-{index}"),
                BasicServiceInput {
                    label: format!("profile-{index}"),
                    provider_account_id: id,
                    service_kind: "image".into(),
                    model: "fixture".into(),
                    adapter: "openai".into(),
                    parameters: json!({}),
                    expected_revision: None,
                },
            )
            .unwrap();
        }
        let pool = db
            .save_model_pool(
                "image-pool",
                ModelPoolInput {
                    label: "fixture".into(),
                    service_kind: "image".into(),
                    enabled: true,
                    attempt_timeout_ms: 1000,
                    total_timeout_ms: 3000,
                    expected_revision: None,
                    tiers: vec![
                        ModelPoolTier {
                            id: "one".into(),
                            label: "one".into(),
                            profile_ids: vec!["profile-1".into()],
                        },
                        ModelPoolTier {
                            id: "two".into(),
                            label: "two".into(),
                            profile_ids: vec!["profile-2".into()],
                        },
                    ],
                },
            )
            .unwrap();
        let result = invoke_service_pool(
            &db,
            &SecretResolver::new(store),
            &pool,
            &db.list_basic_service_profiles().unwrap(),
            BasicServiceRequest {
                prompt: "fixture".into(),
                image_url: None,
            },
            "conversation",
            None,
        )
        .await
        .unwrap();
        assert_eq!(result["profile_id"], "profile-2");
        assert_eq!(result["model_routes"][1]["phase"], "switched");
        assert_eq!(result["model_routes"][1]["to_tier"], 2);
        let attempts = db.list_pool_attempts(None).unwrap();
        assert_eq!(attempts.len(), 2);
        assert!(attempts.iter().any(|a| a["usage"]["image_count"] == 1));
        for task in tasks {
            task.abort();
        }
    }
}
#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use crate::{
        db::model_pools::{ModelPool, ModelPoolTier, ProviderAccount},
        integrations::secret::{InMemorySecretStore, SecretRef, SecretStore},
    };
    #[tokio::test]
    async fn cancelled_pending_image_pool_attempt_has_end_time() {
        let started = Arc::new(tokio::sync::Notify::new());
        let notify = started.clone();
        let app = Router::new().route(
            "/images/generations",
            post(move || {
                let notify = notify.clone();
                async move {
                    notify.notify_one();
                    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                    "{}"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        db.save_provider_account(&ProviderAccount {
            id: "account".into(),
            provider_id: "fixture".into(),
            label: "fixture".into(),
            base_url: base,
            credential_id: Some("llm.fixture".into()),
            profile_ids: vec![],
        })
        .unwrap();
        let profile = BasicServiceProfile {
            id: "profile".into(),
            label: "fixture".into(),
            provider_account_id: "account".into(),
            service_kind: "image".into(),
            model: "fixture".into(),
            adapter: "openai".into(),
            parameters: json!({}),
            revision: 1,
            test: None,
        };
        let pool = ModelPool {
            id: "pool".into(),
            label: "fixture".into(),
            service_kind: "image".into(),
            enabled: true,
            revision: 1,
            attempt_timeout_ms: 10000,
            total_timeout_ms: 10000,
            tiers: vec![ModelPoolTier {
                id: "one".into(),
                label: "one".into(),
                profile_ids: vec!["profile".into()],
            }],
        };
        let store = Arc::new(InMemorySecretStore::new());
        store
            .put(
                &SecretRef::new("llm.fixture"),
                secrecy::SecretString::from("fixture"),
            )
            .await
            .unwrap();
        let test_db = db.clone_connection();
        let call = tokio::spawn(async move {
            invoke_service_pool(
                &test_db,
                &SecretResolver::new(store),
                &pool,
                &[profile],
                BasicServiceRequest {
                    prompt: "fixture".into(),
                    image_url: None,
                },
                "conversation",
                None,
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), started.notified())
            .await
            .unwrap();
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        let attempts = db.list_pool_attempts(None).unwrap();
        assert_eq!(attempts.len(), 1);
        assert!(attempts[0]["ended_at"].as_i64().is_some());
        assert_eq!(attempts[0]["failure"], "cancelled");
        server.abort();
    }
}
