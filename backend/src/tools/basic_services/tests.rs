use super::*;
fn image_fixture() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(3, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
fn output_tool() -> (BasicServiceTool, ResourceService) {
    let root = std::env::temp_dir().join(format!("conversation-output-{}", uuid::Uuid::new_v4()));
    let db = Database::new(&root.join("test.db")).unwrap();
    db.initialize_conversation_outputs().unwrap();
    let conversation = db.create_conversation("图像成果").unwrap();
    let service = ResourceService::new(
        db.clone_connection(),
        root.join("resources"),
        crate::shared::event::EventHub::new(4),
    );
    let tool = BasicServiceTool::new(
        db,
        Arc::new(SecretResolver::new(Arc::new(
            crate::integrations::secret::InMemorySecretStore::new(),
        ))),
        "image",
    )
    .with_conversation_id(conversation.id)
    .with_resource_service(service.clone());
    (tool, service)
}
#[tokio::test]
async fn generated_bytes_become_real_resources_and_vision_uses_only_this_conversation() {
    let (tool, service) = output_tool();
    let output = tool
        .db
        .create_conversation_image_output(
            &tool.conversation_id,
            "call-1",
            "PNG",
            Some("fixture"),
            None,
        )
        .unwrap();
    let bytes = image_fixture();
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    let complete = tool
        .persist_image(&output, decode_image_data_url(&data).unwrap())
        .await
        .unwrap();
    assert_eq!(complete.status, "ready");
    let id = complete.resource_id.clone().unwrap();
    assert_eq!(service.read_bytes(&id).unwrap(), bytes);
    assert_eq!(
        tool.persist_image(&output, bytes)
            .await
            .unwrap()
            .resource_id,
        Some(id.clone())
    );
    assert_eq!(
        tool.vision_image(&json!({"resource_id":id})).unwrap(),
        Some(data)
    );
    let other = tool.db.create_conversation("其他对话").unwrap();
    let other_tool =
        BasicServiceTool::new(tool.db.clone_connection(), tool.resolver.clone(), "vision")
            .with_conversation_id(other.id)
            .with_resource_service(service);
    assert!(other_tool.vision_image(&json!({"resource_id":id})).is_err());
    assert!(tool
        .vision_image(&json!({"resource_id":id,"image_url":"https://example.invalid/image.png"}))
        .is_err());
    assert_eq!(
        tool.db
            .conversation_outputs(&tool.conversation_id)
            .unwrap()
            .len(),
        1
    );
    assert!(tool
        .db
        .conversation_output("other-conversation", &output.id)
        .unwrap()
        .is_none());
}
#[tokio::test]
async fn remote_image_requires_get_grant_before_download_and_is_persisted_afterward() {
    use crate::safety::grant::{AuthorizedResource, NetworkZone};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (tool, service) = output_tool();
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    let app = axum::Router::new().route(
        "/image",
        axum::routing::get(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                image_fixture()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let output = tool
        .db
        .create_conversation_image_output(
            &tool.conversation_id,
            "call-remote",
            "remote",
            None,
            Some(&format!("http://{address}/image")),
        )
        .unwrap();
    let empty = ToolExecutionContext::new_with_resources(
        Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "download",
        vec![],
    );
    assert!(tool.download_image(&output, &empty).await.is_err());
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    let descriptor = tool
        .security_descriptor(&json!({"output_id":output.id}))
        .unwrap();
    assert!(
        matches!(&descriptor.resources[0],ResourceDescriptor::Network { method,.. } if method=="GET")
    );
    let context = ToolExecutionContext::new_with_resources(
        Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "download",
        vec![AuthorizedResource::Network {
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port: address.port(),
            method: "GET".into(),
            zone: NetworkZone::Loopback,
            grant_id: None,
            one_shot_approval: true,
        }],
    );
    let saved = tool.download_image(&output, &context).await.unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert_eq!(saved.status, "ready");
    assert_eq!(
        service
            .read_bytes(saved.resource_id.as_deref().unwrap())
            .unwrap(),
        image_fixture()
    );
    assert!(!serde_json::to_string(&saved)
        .unwrap()
        .contains(&format!("http://{address}")));
    server.abort();
}
#[test]
fn invalid_image_payloads_are_not_accepted_as_files() {
    assert!(decode_image_data_url("data:text/html;base64,PGh0bWw+").is_err());
    assert!(decode_image_data_url("data:image/png;base64,not base64!").is_err());
}
#[test]
fn agent_specific_profile_is_resolved_before_default() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    db.init_basic_services().unwrap();
    db.init_provider_accounts().unwrap();
    db.save_provider_account(&crate::db::model_pools::ProviderAccount {
        id: "account".into(),
        provider_id: "fixture".into(),
        label: "fixture".into(),
        base_url: "https://example.invalid/v1".into(),
        credential_id: Some("llm.fixture".into()),
        profile_ids: vec![],
    })
    .unwrap();
    for id in ["default", "explicit"] {
        db.save_basic_service_profile(
            id,
            crate::db::basic_services::BasicServiceInput {
                label: id.into(),
                provider_account_id: "account".into(),
                service_kind: "image".into(),
                model: id.into(),
                adapter: "openai".into(),
                parameters: json!({}),
                expected_revision: None,
            },
        )
        .unwrap();
    }
    db.set_basic_service_default("image", "default").unwrap();
    db.conn().execute("INSERT INTO agent_definitions(id,name,description,instructions,allowed_tools,capabilities,max_iterations,enabled,source,created_at,updated_at) VALUES('actor','Agent','','','[]','[]',10,1,'user',1,1)",[]).unwrap();
    db.save_connector_binding(
        "actor",
        &crate::db::connectors::ConnectorBinding {
            id: String::new(),
            kind: "agent_model".into(),
            source_id: "agent:actor".into(),
            target_id: "model:explicit".into(),
            service_kind: Some("image".into()),
            model_source: Some("profile".into()),
        },
    )
    .unwrap();
    let resolver = Arc::new(SecretResolver::new(Arc::new(
        crate::integrations::secret::InMemorySecretStore::new(),
    )));
    let defaults = BasicServiceTool::new(db.clone_connection(), resolver.clone(), "image");
    assert_eq!(defaults.selected().unwrap().1[0].id, "default");
    let bound = BasicServiceTool::new(db, resolver, "image").with_agent_id("actor".into());
    assert_eq!(bound.selected().unwrap().1[0].id, "explicit");
    assert_eq!(
        bound.endpoint(&bound.selected().unwrap().1[0]).unwrap(),
        "https://example.invalid/v1/images/generations"
    );
}
#[tokio::test]
async fn vision_missing_image_is_reported_before_service_selection_or_authorization() {
    let (existing, service) = output_tool();
    let tool = BasicServiceTool::new(
        existing.db.clone_connection(),
        existing.resolver.clone(),
        "vision",
    )
    .with_conversation_id(existing.conversation_id.clone())
    .with_resource_service(service);
    let args = json!({"prompt":"请描述图片"});
    assert!(tool
        .security_descriptor(&args)
        .unwrap_err()
        .to_string()
        .contains("resource_id"));
    let context = ToolExecutionContext::new_with_resources(
        Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "missing",
        vec![],
    );
    let result = tool.execute_with_context(args, &context).await;
    assert!(!result.ok);
    assert_eq!(
        result.structured.as_ref().unwrap()["code"],
        "VISION_IMAGE_REQUIRED"
    );
    assert!(result.content.contains("当前对话"));
    assert!(result.content.contains("image_url"));
}

#[tokio::test]
async fn vision_uses_selected_account_and_bound_real_image_in_local_request() {
    use crate::db::{basic_services::BasicServiceInput, model_pools::ProviderAccount};
    use crate::integrations::secret::{InMemorySecretStore, SecretRef, SecretStore};
    use crate::safety::grant::{AuthorizedResource, NetworkZone};
    let (existing, service) = output_tool();
    let bytes = image_fixture();
    let expected = bytes.clone();
    let app = axum::Router::new().route(
        "/chat/completions",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| {
                let expected = expected.clone();
                async move {
                    assert_eq!(
                        headers[axum::http::header::AUTHORIZATION],
                        "Bearer local-vision-fixture"
                    );
                    assert_eq!(body["model"], "vision-fixture");
                    let url = body["messages"][0]["content"][1]["image_url"]["url"]
                        .as_str()
                        .unwrap();
                    let data = url.strip_prefix("data:image/png;base64,").unwrap();
                    assert_eq!(
                        base64::engine::general_purpose::STANDARD
                            .decode(data)
                            .unwrap(),
                        expected
                    );
                    axum::Json(json!({"choices":[{"message":{"content":"真实测试图片"}}]}))
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    existing.db.init_basic_services().unwrap();
    existing.db.init_provider_accounts().unwrap();
    for (id, credential) in [
        ("other-account", "llm.other"),
        ("vision-account", "llm.vision"),
    ] {
        existing
            .db
            .save_provider_account(&ProviderAccount {
                id: id.into(),
                provider_id: "fixture".into(),
                label: id.into(),
                base_url: format!("http://{address}"),
                credential_id: Some(credential.into()),
                profile_ids: vec![],
            })
            .unwrap();
    }
    existing
        .db
        .save_basic_service_profile(
            "vision-profile",
            BasicServiceInput {
                label: "vision".into(),
                provider_account_id: "vision-account".into(),
                service_kind: "vision".into(),
                model: "vision-fixture".into(),
                adapter: "openai".into(),
                parameters: json!({}),
                expected_revision: None,
            },
        )
        .unwrap();
    existing
        .db
        .set_basic_service_default("vision", "vision-profile")
        .unwrap();
    let store = Arc::new(InMemorySecretStore::new());
    store
        .put(
            &SecretRef::new("llm.vision"),
            secrecy::SecretString::from("local-vision-fixture"),
        )
        .await
        .unwrap();
    store
        .put(
            &SecretRef::new("llm.other"),
            secrecy::SecretString::from("other-fixture"),
        )
        .await
        .unwrap();
    let output = existing
        .db
        .create_conversation_image_output(
            &existing.conversation_id,
            "image-fixture",
            "fixture",
            None,
            None,
        )
        .unwrap();
    let saved = existing.persist_image(&output, bytes).await.unwrap();
    let tool = BasicServiceTool::new(
        existing.db.clone_connection(),
        Arc::new(SecretResolver::new(store)),
        "vision",
    )
    .with_conversation_id(existing.conversation_id.clone())
    .with_resource_service(service);
    let context = ToolExecutionContext::new_with_resources(
        Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "vision-fixture",
        vec![AuthorizedResource::Network {
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port: address.port(),
            method: "POST".into(),
            zone: NetworkZone::Loopback,
            grant_id: None,
            one_shot_approval: true,
        }],
    );
    let result = tool
        .execute_with_context(
            json!({"prompt":"描述图片", "resource_id":saved.resource_id.unwrap()}),
            &context,
        )
        .await;
    assert!(result.ok, "{}", result.content);
    assert_eq!(result.content, "真实测试图片");
    server.abort();
}
