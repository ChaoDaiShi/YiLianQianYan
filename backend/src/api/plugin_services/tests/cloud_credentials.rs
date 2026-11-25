use super::*;

#[tokio::test]
async fn owned_cloud_bearer_requires_the_reviewed_service_revision() {
    let mut settings = values();
    settings.insert("web".into(), json!("https://service.example.test/portal"));
    let (server, expected) = fixture_with_service(
        settings.clone(),
        json!({
            "deployment":"cloud","cloud_auth":"bearer","model_mode":"independent",
            "executable_key":"","working_directory_key":"","argument_keys":[],
            "health_path":"/health","mcp_path":"/mcp"
        }),
    );
    let enabled =
        crate::api::capabilities::change_import_owned(&server, "fixture.qq", expected, "enable")
            .unwrap();
    let revision = super::revision(&configuration(&server, "fixture.qq", "napcat").unwrap());
    let credential_url = format!("{PATH}/credential");
    let value = json!({"configuration_revision":revision,"token":"synthetic-cloud-service-token"});
    assert_eq!(
        request(
            server.clone(),
            Method::PUT,
            &credential_url,
            value.clone(),
            false
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            server.clone(),
            Method::PUT,
            &credential_url,
            json!({"configuration_revision":"stale","token":"synthetic-cloud-service-token"}),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (code, reply) = request(server.clone(), Method::PUT, &credential_url, value, true).await;
    assert_eq!(code, StatusCode::NO_CONTENT, "{reply}");
    assert!(reply.is_null());

    let owned_id = managed_mcp_id("fixture.qq", "napcat");
    let reference = |server: &AppServer| {
        let body: String = server
            .db
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [format!("plugin_cloud_credential:{owned_id}")],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!body.contains("synthetic-cloud-service-token"));
        serde_json::from_str::<Value>(&body).unwrap()["reference"].clone()
    };
    let old_reference =
        serde_json::from_value::<crate::integrations::secret::SecretRef>(reference(&server))
            .unwrap();
    let config = configuration(&server, "fixture.qq", "napcat").unwrap();
    let bearer = credential::bearer(&server, "fixture.qq", "napcat", &config).unwrap();
    assert!(bearer.configured().await);
    for target in [
        config.health_url.as_deref().unwrap(),
        config.mcp_url.as_deref().unwrap(),
    ] {
        let header = bearer.authorization(target).await.unwrap();
        assert!(header.is_sensitive());
        assert_eq!(
            header.to_str().unwrap(),
            "Bearer synthetic-cloud-service-token"
        );
    }
    assert!(bearer
        .authorization("https://service.example.test:9443/mcp")
        .await
        .is_err());
    let (_, ordinary) = request(
        server.clone(),
        Method::GET,
        "/api/plugins/fixture.qq/settings/runtime",
        Value::Null,
        true,
    )
    .await;
    assert!(!ordinary
        .to_string()
        .contains("synthetic-cloud-service-token"));
    assert_eq!(ordinary["values"], json!(settings));

    settings.insert("web".into(), json!("https://second.example.test/portal"));
    assert_eq!(
        request(
            server.clone(),
            Method::PUT,
            "/api/plugins/fixture.qq/settings/runtime",
            json!({"values":settings}),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    let updated_config = configuration(&server, "fixture.qq", "napcat").unwrap();
    let updated_revision = super::revision(&updated_config);
    let updated_bearer =
        credential::bearer(&server, "fixture.qq", "napcat", &updated_config).unwrap();
    assert!(!updated_bearer.configured().await);
    assert_eq!(
        request(
            server.clone(),
            Method::POST,
            &format!("{PATH}/ensure"),
            json!({}),
            true
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        request(
            server.clone(),
            Method::DELETE,
            &credential_url,
            json!({"configuration_revision":revision}),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let replacement =
        json!({"configuration_revision":updated_revision,"token":"synthetic-replacement-token"});
    assert_eq!(
        request(
            server.clone(),
            Method::PUT,
            &credential_url,
            replacement.clone(),
            true
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert!(server
        .secret_store
        .get(&old_reference)
        .await
        .unwrap()
        .is_none());
    assert!(!bearer.configured().await);
    let replacement_reference =
        serde_json::from_value::<crate::integrations::secret::SecretRef>(reference(&server))
            .unwrap();
    assert_eq!(
        request(
            server.clone(),
            Method::DELETE,
            &credential_url,
            json!({"configuration_revision":updated_revision}),
            true
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert!(server
        .secret_store
        .get(&replacement_reference)
        .await
        .unwrap()
        .is_none());
    assert!(updated_bearer
        .authorization(updated_config.mcp_url.as_deref().unwrap())
        .await
        .is_err());

    assert_eq!(
        request(
            server.clone(),
            Method::PUT,
            &credential_url,
            replacement,
            true
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let final_reference =
        serde_json::from_value::<crate::integrations::secret::SecretRef>(reference(&server))
            .unwrap();
    assert_eq!(
        request(
            server.clone(),
            Method::DELETE,
            "/api/capabilities/imports/fixture.qq",
            json!({"revision":enabled.revision}),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(server
        .secret_store
        .get(&final_reference)
        .await
        .unwrap()
        .is_none());
    let preview = import_store::inspect(&server.db, enabled.package, 500).unwrap();
    let reinstalled = import_store::confirm(&server.db, &preview.preview_id, 501).unwrap();
    crate::api::capabilities::change_import_owned(
        &server,
        "fixture.qq",
        reinstalled.revision,
        "enable",
    )
    .unwrap();
    assert!(!updated_bearer.configured().await);
    assert_eq!(request(server, Method::PUT, &credential_url,
        json!({"configuration_revision":updated_revision,"token":"synthetic-replacement-token"}), true).await.0, StatusCode::CONFLICT);
}

#[test]
fn cloud_service_uses_its_configured_https_origin_and_independent_model() {
    let mut settings = values();
    settings.insert("web".into(), json!("https://plugin.example.test/portal"));
    let (server, revision) = fixture_with_service(
        settings,
        json!({
            "deployment":"cloud","model_mode":"independent","executable_key":"","working_directory_key":"",
            "argument_keys":[],"health_path":"/health","mcp_path":"/mcp",
        }),
    );
    crate::api::capabilities::change_import_owned(&server, "fixture.qq", revision, "enable")
        .unwrap();
    let (definition, settings, revision) = resolve_record(&server, "fixture.qq", "napcat").unwrap();
    let configured = definition
        .configuration(&settings, revision.clone())
        .unwrap();
    assert_eq!(configured.deployment, ServiceDeployment::Cloud);
    assert_eq!(configured.model_mode, ServiceModelMode::Independent);
    assert!(configured.launch.is_none());
    assert_eq!(
        configured.health_url.as_deref(),
        Some("https://plugin.example.test/health")
    );
    assert_eq!(
        configured.mcp_url.as_deref(),
        Some("https://plugin.example.test/mcp")
    );
    let mut local = settings.clone();
    local.insert("web".into(), json!("http://127.0.0.1:1234"));
    assert!(definition.configuration(&local, revision).is_err());
}
