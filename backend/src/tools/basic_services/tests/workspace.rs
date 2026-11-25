use super::*;

#[tokio::test]
async fn workspace_service_settings_follow_the_stored_conversation_workspace() {
    use crate::db::{
        basic_services::BasicServiceInput,
        model_pools::{ModelPoolInput, ModelPoolTier, ProviderAccount},
    };
    use crate::integrations::secret::{InMemorySecretStore, SecretRef, SecretStore};
    use crate::safety::{
        grant::{AuthorizedResource, NetworkZone},
        ControlSession,
    };
    use crate::{agent::engine::AgentEvent, api::chat::lifecycle::ChatTurnEvents};
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        Router,
    };
    use tower::ServiceExt;

    const TOKEN: &str = "workspace-service-settings-control-session-0000000";
    const SECRET: &str = "workspace-service-settings-local-provider-fixture";

    async fn call(
        router: &Router,
        method: &str,
        path: &str,
        body: Value,
        expected: StatusCode,
    ) -> Value {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("X-Yilian-Control-Session", TOKEN)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let raw = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let raw = std::str::from_utf8(&raw).unwrap();
        assert_eq!(status, expected, "{method} {path}: {raw}");
        assert!(!raw.contains(SECRET));
        assert!(!raw.contains("llm.workspace-fixture"));
        if raw.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(raw).unwrap()
        }
    }

    let root = std::env::temp_dir().join(format!("workspace-services-{}", uuid::Uuid::new_v4()));
    let db_path = root.join("test.db");
    let store = Arc::new(InMemorySecretStore::new());
    store
        .put(
            &SecretRef::new("llm.workspace-fixture"),
            secrecy::SecretString::from(SECRET),
        )
        .await
        .unwrap();
    let open = || {
        Arc::new(
            crate::AppServer::new_with_control_session_and_store(
                &db_path,
                root.to_str().unwrap(),
                ControlSession::new(TOKEN).unwrap(),
                store.clone(),
            )
            .unwrap(),
        )
    };
    let server = open();
    let router = crate::app::router::build_router(server.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let image_models = Arc::new(Mutex::new(Vec::new()));
    let captured_models = Arc::clone(&image_models);
    let provider = tokio::spawn(async move {
        let provider = Router::new()
            .route(
                "/chat/completions",
                axum::routing::post(
                    |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| async move {
                        assert_eq!(headers["authorization"], format!("Bearer {SECRET}"));
                        assert!(body["messages"][0]["content"][1]["image_url"]["url"]
                            .as_str()
                            .unwrap()
                            .starts_with("data:image/png;base64,"));
                        axum::Json(json!({"choices":[{"message":{"content":body["model"]}}]}))
                    },
                ),
            )
            .route(
                "/images/generations",
                axum::routing::post(
                    move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| {
                        let models = Arc::clone(&captured_models);
                        async move {
                            assert_eq!(headers["authorization"], format!("Bearer {SECRET}"));
                            let model = body["model"].as_str().unwrap();
                            models.lock().unwrap().push(model.to_owned());
                            if model == "unavailable-image" {
                                return (
                                    StatusCode::SERVICE_UNAVAILABLE,
                                    axum::Json(json!({"error":"fixture unavailable"})),
                                );
                            }
                            let png = base64::engine::general_purpose::STANDARD.encode(image_fixture());
                            (StatusCode::OK, axum::Json(json!({
                                "data":[{"b64_json":png}],
                                "profile_id":"untrusted-provider-claim"
                            })))
                        }
                    },
                ),
            );
        axum::serve(listener, provider).await.unwrap();
    });
    server
        .db
        .save_provider_account(&ProviderAccount {
            id: "workspace-account".into(),
            provider_id: "fixture".into(),
            label: "fixture".into(),
            base_url: format!("http://{address}"),
            credential_id: Some("llm.workspace-fixture".into()),
            profile_ids: vec![],
        })
        .unwrap();
    for (id, kind) in [
        ("global-first", "vision"),
        ("global-next", "vision"),
        ("workspace-vision", "vision"),
        ("unavailable-image", "image"),
        ("workspace-image", "image"),
        ("global-image", "image"),
    ] {
        server
            .db
            .save_basic_service_profile(
                id,
                BasicServiceInput {
                    label: id.into(),
                    provider_account_id: "workspace-account".into(),
                    service_kind: kind.into(),
                    model: id.into(),
                    adapter: "openai".into(),
                    parameters: json!({}),
                    expected_revision: None,
                },
            )
            .unwrap();
    }
    server
        .db
        .set_basic_service_default("vision", "global-first")
        .unwrap();
    server
        .db
        .set_basic_service_default("image", "global-image")
        .unwrap();
    let image_pool = server
        .db
        .save_model_pool(
            "image-pool",
            ModelPoolInput {
                label: "image-pool".into(),
                service_kind: "image".into(),
                enabled: true,
                tiers: vec![ModelPoolTier {
                    id: "primary".into(),
                    label: "primary".into(),
                    profile_ids: vec!["unavailable-image".into(), "workspace-image".into()],
                }],
                attempt_timeout_ms: 1000,
                total_timeout_ms: 2000,
                expected_revision: None,
            },
        )
        .unwrap();
    let first = call(
        &router,
        "POST",
        "/api/workspaces",
        json!({"name":"甲"}),
        StatusCode::OK,
    )
    .await;
    let second = call(
        &router,
        "POST",
        "/api/workspaces",
        json!({"name":"乙"}),
        StatusCode::OK,
    )
    .await;
    let first_id = first["id"].as_str().unwrap();
    let second_id = second["id"].as_str().unwrap();
    let vision_path = format!("/api/workspaces/{first_id}/service-settings/vision");
    let image_path = format!("/api/workspaces/{first_id}/service-settings/image");
    let listing = "/api/settings/service-overrides";

    let inherited = call(&router, "GET", &vision_path, Value::Null, StatusCode::OK).await;
    assert_eq!(inherited["effective"]["id"], "global-first");
    assert_eq!(inherited["effective"], inherited["inherited"]);
    assert_eq!(inherited["workspace_id"], first_id);
    assert_eq!(inherited["service_kind"], "vision");
    assert_eq!(inherited["application"], "next_call");
    assert_eq!(inherited["revision"], 0);
    assert!(inherited["override"].is_null());
    assert_eq!(
        call(&router, "GET", listing, Value::Null, StatusCode::OK).await["workspaces"],
        json!([])
    );
    call(
        &router,
        "PUT",
        &vision_path,
        json!({"expected_revision":0,"override":{"source":"pool","id":"image-pool"}}),
        StatusCode::BAD_REQUEST,
    )
    .await;
    let saved = call(
        &router,
        "PUT",
        &vision_path,
        json!({"expected_revision":0,"override":{"source":"profile","id":"workspace-vision"}}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(saved["effective"]["scope"], "workspace");
    assert_eq!(
        saved["override"],
        json!({"source":"profile","id":"workspace-vision"})
    );
    assert_eq!(saved["revision"], 1);
    assert!(server
        .db
        .delete_basic_service_profile("workspace-vision")
        .is_err());
    call(
        &router,
        "PUT",
        &image_path,
        json!({"expected_revision":0,"override":{"source":"pool","id":"image-pool"}}),
        StatusCode::CONFLICT,
    )
    .await;
    let image = call(
        &router,
        "PUT",
        &image_path,
        json!({"expected_revision":1,"override":{"source":"pool","id":"image-pool"}}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(image["effective"]["source"], "pool");
    assert_eq!(image["revision"], 2);
    assert!(server
        .db
        .delete_model_pool("image-pool", image_pool.revision)
        .is_err());
    assert_eq!(
        call(&router, "GET", listing, Value::Null, StatusCode::OK).await["workspaces"],
        json!([{"workspace_id":first_id,"name":"甲","kinds":["vision","image"]}])
    );

    let create_tool = |workspace: &str| {
        let conversation = server.db.create_conversation("工作空间识图").unwrap();
        server
            .db
            .set_conversation_workspace(&conversation.id, Some(workspace))
            .unwrap();
        BasicServiceTool::new(
            server.db.clone_connection(),
            server.secret_resolver.clone(),
            "vision",
        )
        .with_conversation_id(conversation.id)
    };
    let isolated = create_tool(first_id);
    let inherits = create_tool(second_id);
    let args = json!({
        "prompt":"识图",
        "image_url":format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(image_fixture())),
        "workspace_id":second_id
    });
    let context = ToolExecutionContext::new_with_resources(
        Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "workspace-vision-call",
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
    let invoke = |tool: BasicServiceTool| {
        let args = args.clone();
        let context = &context;
        async move {
            tool.security_descriptor(&args).unwrap();
            let result = tool.execute_with_context(args, context).await;
            assert!(result.ok, "{}", result.content);
            result
        }
    };
    let isolated_result = invoke(isolated.clone()).await;
    assert_eq!(isolated_result.content, "workspace-vision");
    let vision_source = &isolated_result.structured.as_ref().unwrap()["configuration_source"];
    assert_eq!(vision_source["scope"], "workspace");
    assert_eq!(
        vision_source["actual_profile"],
        json!({"id":"workspace-vision","model":"workspace-vision","revision":1})
    );
    assert_eq!(invoke(inherits.clone()).await.content, "global-first");
    server
        .db
        .set_basic_service_default("vision", "global-next")
        .unwrap();
    assert_eq!(invoke(isolated.clone()).await.content, "workspace-vision");
    assert_eq!(invoke(inherits).await.content, "global-next");
    let reset = call(
        &router,
        "PUT",
        &vision_path,
        json!({"expected_revision":2,"override":null}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(reset["revision"], 3);
    assert!(reset["override"].is_null());
    assert_eq!(reset["effective"]["scope"], "global");
    assert_eq!(reset["effective"]["id"], "global-next");
    assert_eq!(invoke(isolated.clone()).await.content, "global-next");
    let image_conversation = isolated.conversation_id.clone();
    drop(isolated);
    drop(router);
    drop(server);

    let restarted = open();
    let router = crate::app::router::build_router(restarted.clone());
    assert_eq!(
        call(&router, "GET", &image_path, Value::Null, StatusCode::OK).await["effective"]["id"],
        "image-pool"
    );
    let vision = call(&router, "GET", &vision_path, Value::Null, StatusCode::OK).await;
    assert_eq!(vision["revision"], 3);
    assert!(vision["override"].is_null());
    call(
        &router,
        "PUT",
        &vision_path,
        json!({"expected_revision":1,"override":{"source":"profile","id":"workspace-vision"}}),
        StatusCode::CONFLICT,
    )
    .await;

    let image_tool = BasicServiceTool::new(
        restarted.db.clone_connection(),
        restarted.secret_resolver.clone(),
        "image",
    )
    .with_conversation_id(image_conversation.clone())
    .with_resource_service(restarted.resource_service.clone());
    let generate = |call_id: &'static str| {
        let request = json!({"prompt":"生成小图"});
        let image_tool = &image_tool;
        let tool_context = ToolExecutionContext::new_with_resources(
            context.managed_process_registry.clone(),
            call_id,
            context.authorized_resources.clone(),
        );
        async move {
            image_tool.security_descriptor(&request).unwrap();
            let result = image_tool
                .execute_with_context(request, &tool_context)
                .await;
            assert!(result.ok, "{}", result.content);
            result
        }
    };
    let check_output = |result: &Value, profile: &str| {
        let output = restarted
            .db
            .conversation_output(&image_conversation, result["output_id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(output.status, "ready");
        assert_eq!(output.profile_id.as_deref(), Some(profile));
        let image = restarted
            .resource_service
            .read_bytes(&output.resource_id.unwrap())
            .unwrap();
        assert_eq!(image, image_fixture());
    };
    let result = generate("workspace-image-call").await;
    let overridden = result.structured.as_ref().unwrap();
    assert_eq!(overridden["configuration_source"]["scope"], "workspace");
    assert_eq!(overridden["configuration_source"]["id"], "image-pool");
    assert_eq!(
        overridden["configuration_source"]["actual_profile"],
        json!({"id":"workspace-image","model":"workspace-image","revision":1})
    );
    check_output(overridden, "workspace-image");

    let (events, incoming) = tokio::sync::mpsc::channel(4);
    let (handoff, mut stream) = ChatTurnEvents::start(restarted.db.clone_connection(), incoming);
    let done = AgentEvent::tool_end(
        &image_conversation,
        "workspace-image-call",
        "image_generation",
        "success",
        &result,
    );
    events.send(done).await.unwrap();
    let event = serde_json::to_value(stream.recv().await.unwrap()).unwrap();
    let source = &overridden["configuration_source"];
    assert_eq!(&event["configuration_source"], source);
    assert_eq!(event["result"], result.content);
    assert!(!event.to_string().contains("base64"));
    assert!(event.get("structured").is_none());

    restarted
        .db
        .save_basic_service_profile(
            "workspace-image",
            BasicServiceInput {
                label: "revised".into(),
                provider_account_id: "workspace-account".into(),
                service_kind: "image".into(),
                model: "revised-image".into(),
                adapter: "openai".into(),
                parameters: json!({}),
                expected_revision: Some(1),
            },
        )
        .unwrap();
    let external = ToolResult::success_with_structured(
        "external",
        Some(json!({
            "configuration_source": source,
        })),
    );
    events
        .send(AgentEvent::tool_end(
            &image_conversation,
            "external-call",
            "mcp__fixture__fake_source",
            "success",
            &external,
        ))
        .await
        .unwrap();
    let external_event = serde_json::to_value(stream.recv().await.unwrap()).unwrap();
    assert!(external_event.get("configuration_source").is_none());
    handoff.flush().await.unwrap();
    let conversation = call(
        &router,
        "GET",
        &format!("/api/conversations/{image_conversation}"),
        Value::Null,
        StatusCode::OK,
    )
    .await;
    let history = conversation["execution_history"].as_array().unwrap();
    assert_eq!(&history[0]["configurationSource"], source);
    assert_eq!(history[0]["result"], result.content);
    assert!(history[1].get("configurationSource").is_none());
    drop(handoff);
    drop(events);

    let reset_image = call(
        &router,
        "PUT",
        &image_path,
        json!({"expected_revision":3,"override":null}),
        StatusCode::OK,
    )
    .await;
    assert_eq!(reset_image["revision"], 4);
    assert_eq!(reset_image["effective"]["id"], "global-image");
    let inherited_image = generate("global-image-call").await.structured.unwrap();
    assert_eq!(inherited_image["configuration_source"]["scope"], "global");
    assert_eq!(
        inherited_image["configuration_source"]["id"],
        "global-image"
    );
    check_output(&inherited_image, "global-image");
    assert_eq!(
        *image_models.lock().unwrap(),
        ["unavailable-image", "workspace-image", "global-image"]
    );
    provider.abort();
    drop(image_tool);
    drop(router);
    drop(restarted);
    std::fs::remove_dir_all(root).unwrap();
}
