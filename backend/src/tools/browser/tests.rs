use super::*;
struct Host;
#[async_trait::async_trait]
impl BrowserHost for Host {
    fn tabs(&self) -> Vec<BrowserTab> {
        vec![BrowserTab {
            session_id: "s".into(),
            tab_id: "t".into(),
            workspace_id: None,
            site_id: None,
            url: "https://example.org/current".into(),
            title: "Fixture".into(),
            navigation_id: 3,
            mode: "embedded".into(),
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            error: None,
            ephemeral: true,
        }]
    }
    async fn dispatch(
        &self,
        _request: BrowserRequest,
        _cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        Ok(BrowserReply::verified(json!({})))
    }
}
fn runtime() -> Arc<BrowserRuntime> {
    let runtime = Arc::new(BrowserRuntime::new());
    runtime.install_host(Arc::new(Host)).unwrap();
    runtime
}
#[test]
fn visible_tools_describe_actual_page_not_model_supplied_origin() {
    let tools = visible_browser_tools(runtime());
    let tool = tools
        .iter()
        .find(|tool| tool.name() == "visible_browser_click")
        .unwrap();
    let descriptor=tool.security_descriptor(&json!({"target":{"session_id":"s","tab_id":"t","navigation_id":3},"url":"https://attacker.invalid/","observation_id":"o","element_id":"e"})).unwrap();
    descriptor.validate().unwrap();
    assert!(
        matches!(&descriptor.resources[0],ResourceDescriptor::Network{url,method} if url=="https://example.org/current"&&method=="POST")
    );
    assert_eq!(descriptor.default_risk, RiskLevel::High);
    assert!(tool
        .security_descriptor(&json!({"target":{"session_id":"s","tab_id":"t","navigation_id":2}}))
        .is_err());
}
#[test]
fn visible_navigation_descriptor_authorizes_destination() {
    let tools = visible_browser_tools(runtime());
    let tool = tools
        .iter()
        .find(|tool| tool.name() == "visible_browser_navigate")
        .unwrap();
    let descriptor=tool.security_descriptor(&json!({"target":{"session_id":"s","tab_id":"t","navigation_id":3},"url":"https://destination.example/page"})).unwrap();
    descriptor.validate().unwrap();
    assert!(
        matches!(&descriptor.resources[0],ResourceDescriptor::Network{url,..} if url=="https://destination.example/page")
    );
}

#[test]
fn credential_fill_tool_accepts_only_the_live_target() {
    let tools = visible_browser_tools(runtime());
    let tool = tools
        .iter()
        .find(|tool| tool.name() == "visible_browser_fill_credential")
        .unwrap();
    let schema = tool.parameters();
    assert_eq!(schema["required"], json!(["target"]));
    assert_eq!(schema["properties"].as_object().unwrap().len(), 1);
    let descriptor = tool
        .security_descriptor(&json!({"target":{"session_id":"s","tab_id":"t","navigation_id":3}}))
        .unwrap();
    descriptor.validate().unwrap();
    assert!(
        matches!(&descriptor.resources[0], ResourceDescriptor::Network { url, method } if url == "https://example.org/current" && method == "POST")
    );
}
#[tokio::test]
async fn visible_tools_refuse_direct_execution() {
    for tool in visible_browser_tools(runtime()) {
        assert!(!tool.execute(json!({})).await.ok);
    }
}
#[test]
fn visible_tools_never_accept_operation_owner_from_model_schema() {
    for tool in visible_browser_tools(runtime()) {
        let schema = tool.parameters();
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema["properties"].get("operation_id").is_none());
        assert!(schema["properties"].get("owner").is_none());
    }
}

#[test]
fn visible_model_content_contains_current_refs_as_parseable_bounded_json() {
    let value = json!({"tab":{"session_id":"s","tab_id":"t","navigation_id":7,"url":"https://example.org/?token=hidden","mode":"embedded"},"observation":{"observation_id":"observation-current","elements":[{"id":"element-submit","role":"button","name":"Send"}],"text":"UNTRUSTED ".repeat(3000)}});
    let content = model_content("operation", &value);
    let data: Value = serde_json::from_str(&content[content.find('{').unwrap()..]).unwrap();
    assert_eq!(data["observation"]["observation_id"], "observation-current");
    assert_eq!(data["observation"]["elements"][0]["id"], "element-submit");
    assert_eq!(data["target"]["navigation_id"], 7);
    assert!(content.contains("UNTRUSTED_BROWSER_DATA"));
    assert!(!content.contains("token=hidden"));
    assert!(content.chars().count() < 8000);
}

#[test]
fn model_content_reports_browser_fill_readback() {
    let filled = model_content("operation", &json!({"action":{"filled":true}}));
    let failed = model_content("operation", &json!({"action":{"filled":false}}));

    assert_eq!(filled.lines().next(), Some("浏览器文本填写验证：通过"));
    assert_ne!(failed.lines().next(), filled.lines().next());
}

#[test]
fn visible_operation_namespace_separates_owners_and_preserves_repeat() {
    let context = ToolExecutionContext::new_with_resources(
        std::sync::Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "provider-reused-call-id",
        vec![],
    )
    .with_conversation_id("legacy-reference".into());
    let first = crate::execution::ExecutionOwner::Conversation {
        conversation_id: "first".into(),
    };
    let second = crate::execution::ExecutionOwner::Conversation {
        conversation_id: "second".into(),
    };
    assert_ne!(
        agent_operation_id("visible_browser_fill", &context, Some(&first)),
        agent_operation_id("visible_browser_fill", &context, Some(&second))
    );
    assert_eq!(
        agent_operation_id("visible_browser_fill", &context, Some(&first)),
        agent_operation_id("visible_browser_fill", &context, Some(&first))
    );
    assert_ne!(
        agent_operation_id("visible_browser_fill", &context, Some(&first)),
        agent_operation_id("visible_browser_click", &context, Some(&first))
    );
}

#[tokio::test]
async fn visible_tools_nonobject_args_fail_closed_without_panic() {
    let tool = visible_browser_tools(runtime())
        .into_iter()
        .find(|tool| tool.name() == "visible_browser_tabs")
        .unwrap();
    let context = ToolExecutionContext::new_with_resources(
        std::sync::Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "malformed",
        vec![crate::safety::grant::AuthorizedResource::Desktop {
            action: "visible_browser_tabs".into(),
            target: None,
            one_shot_approval: true,
        }],
    );
    for args in [json!([]), json!("primitive"), json!(true), Value::Null] {
        assert!(tool.security_descriptor(&args).is_err());
        assert!(!tool.execute_with_context(args, &context).await.ok);
    }
}

struct EpochCountingHost(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl BrowserHost for EpochCountingHost {
    fn tabs(&self) -> Vec<BrowserTab> {
        Host.tabs()
    }
    async fn dispatch(
        &self,
        _request: BrowserRequest,
        _cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(BrowserReply::verified(json!({"tabs":self.tabs()})))
    }
}
struct RevokeDuringEvaluation {
    inner: Arc<dyn Tool>,
    runtime: Arc<BrowserRuntime>,
}
#[async_trait::async_trait]
impl Tool for RevokeDuringEvaluation {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn description(&self) -> &str {
        self.inner.description()
    }
    fn parameters(&self) -> Value {
        self.inner.parameters()
    }
    fn risk_level(&self) -> RiskLevel {
        self.inner.risk_level()
    }
    fn authorization_epoch(&self) -> Option<u64> {
        self.inner.authorization_epoch()
    }
    fn security_descriptor(&self, args: &Value) -> Result<ToolSecurityDescriptor, DescriptorError> {
        self.runtime.invalidate_authorizations();
        self.inner.security_descriptor(args)
    }
    async fn execute(&self, args: Value) -> ToolResult {
        self.inner.execute(args).await
    }
    async fn execute_with_context(
        &self,
        args: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        self.inner.execute_with_context(args, context).await
    }
}
#[tokio::test]
async fn visible_gateway_epoch_is_captured_before_initial_and_approved_evaluation() {
    for approved in [false, true] {
        let runtime = Arc::new(BrowserRuntime::new());
        let host = Arc::new(EpochCountingHost(std::sync::atomic::AtomicUsize::new(0)));
        runtime.install_host(host.clone()).unwrap();
        let inner = visible_browser_tools(runtime.clone())
            .into_iter()
            .find(|tool| tool.name() == "visible_browser_tabs")
            .unwrap();
        let mut registry = crate::tools::ToolRegistry::new();
        registry.register(Arc::new(RevokeDuringEvaluation { inner, runtime }));
        let mut config = crate::config::types::AppConfig::default();
        config.permissions.mode = crate::config::types::PermissionMode::FullAccess;
        config.sandbox.profile = crate::config::types::SandboxProfile::Open;
        let gateway = crate::safety::SecurityExecutionGateway::with_sandbox_and_registry(
            config.sandbox.clone(),
            std::env::temp_dir(),
            Arc::new(registry),
        )
        .with_permissions_config(Arc::new(parking_lot::RwLock::new(config)));
        let request = crate::safety::SecurityExecutionRequest {
            conversation_id: "legacy-without-owner".into(),
            tool_call_id: "source-call".into(),
            tool_name: "visible_browser_tabs".into(),
            arguments: json!({}),
            subject: crate::safety::SecuritySubject::local_user(),
        };
        let result = if approved {
            gateway
                .execute_approved_with_role(
                    &request,
                    crate::safety::BuiltInRole::Owner,
                    RiskLevel::High,
                )
                .await
        } else {
            gateway
                .execute_with_role(&request, crate::safety::BuiltInRole::Owner, RiskLevel::High)
                .await
        }
        .unwrap();
        match result {
            crate::safety::execution_gateway::SecurityExecutionOutcome::Executed {
                tool_result,
                ..
            } => {
                assert!(!tool_result.ok);
                assert!(tool_result
                    .content
                    .contains("permission_revoked_before_execution"));
            }
            other => panic!("Expected permission-evidence rejection, got {other:?}"),
        };
        assert_eq!(host.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
