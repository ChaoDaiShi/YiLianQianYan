use super::*;
use crate::{
    browser::{BrowserAction, BrowserError, BrowserHost, BrowserReply, BrowserRequest, BrowserTab},
    execution::{
        context::{ChatTurnModel, CURRENT_CHAT_TURN_MODEL},
        owner::CURRENT_EXECUTION_OWNER,
        ExecutionOwner,
    },
    safety::{
        execution_gateway::SecurityExecutionOutcome, SecurityExecutionRequest, SecuritySubject,
    },
};
use axum::{http::HeaderMap, response::IntoResponse};
use std::{net::SocketAddr, path::PathBuf};
use tokio_util::sync::CancellationToken;

const CHILD_TEST: &str = "api::plugin_services::tests::windows_fixture::managed_http_child";

mod goal_cases;

fn free_address() -> SocketAddr {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

#[tokio::test]
#[ignore = "started as the child process by managed_http_service_shares_browser_mcp_and_model_runtime"]
async fn managed_http_child() {
    let path = std::env::current_dir().unwrap().join("fixture-address");
    let address: SocketAddr = std::fs::read_to_string(path).unwrap().parse().unwrap();
    let tcp = tokio::net::TcpListener::bind(address).await.unwrap();
    let router = Router::new()
        .route(
            "/",
            get(|| async { Json(json!({"pid":std::process::id()})) }),
        )
        .route(
            "/health",
            get(|| async {
                if std::path::Path::new("fixture-exit").is_file() {
                    std::process::exit(23);
                }
                if std::path::Path::new("degraded").exists() {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::NO_CONTENT
                }
            }),
        )
        .route("/mcp", post(mcp));
    axum::serve(tcp, router).await.unwrap();
}

async fn mcp(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
    tokio::spawn(run_mcp(headers, request)).await.unwrap()
}

async fn run_mcp(headers: HeaderMap, request: Value) -> Json<Value> {
    let result = match request["method"].as_str().unwrap_or_default() {
        "server/discover" => json!({"capabilities":{"tools":{}}}),
        "tools/list" => {
            json!({"tools":[{"name":"identify_instance","inputSchema":{"type":"object"}}]})
        }
        "tools/call" => {
            let arguments = &request["params"]["arguments"];
            if arguments["wait"] == true {
                std::fs::write("mcp-inflight", "active").unwrap();
                while !std::path::Path::new("mcp-continue").exists() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
            let mut result = json!({"pid":std::process::id()});
            if let (Some(endpoint), Some(token)) = (
                headers.get("x-yilian-model-endpoint"),
                headers.get("x-yilian-model-token"),
            ) {
                let endpoint = endpoint.to_str().unwrap();
                let token = token.to_str().unwrap();
                let client = reqwest::Client::builder().no_proxy().build().unwrap();
                let send = |purpose| {
                    client.post(endpoint).bearer_auth(token).json(&json!({
                    "purpose":purpose,"input":"fixture prompt","max_output_tokens":5,"output":"json_object",
                })).send()
                };
                result["wrong_purpose"] = json!(send("embedding").await.unwrap().status().as_u16());
                let response = send("chat").await.unwrap();
                result["proxy_status"] = json!(response.status().as_u16());
                result["model_result"] = response.json::<Value>().await.unwrap();
                result["replay_status"] = json!(send("chat").await.unwrap().status().as_u16());
            }
            std::fs::write("last-model-result", result.to_string()).unwrap();
            json!({"content":[{"type":"text","text":"observed plugin service"}],"structuredContent":result})
        }
        _ => json!({}),
    };
    Json(json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
}

#[derive(Default)]
struct NativeTabFixture(parking_lot::Mutex<Vec<BrowserTab>>);

#[async_trait::async_trait]
impl BrowserHost for NativeTabFixture {
    fn tabs(&self) -> Vec<BrowserTab> {
        self.0.lock().clone()
    }

    async fn dispatch(
        &self,
        request: BrowserRequest,
        _: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        assert_eq!(request.action, BrowserAction::Open);
        let url = request.payload["url"].as_str().unwrap();
        let observed: Value = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let tab = BrowserTab {
            session_id: Uuid::new_v4().to_string(),
            tab_id: Uuid::new_v4().to_string(),
            workspace_id: None,
            site_id: None,
            url: url.into(),
            title: observed["pid"].to_string(),
            navigation_id: 1,
            mode: "hidden".into(),
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            error: None,
            ephemeral: false,
        };
        self.0.lock().push(tab.clone());
        Ok(BrowserReply::verified(json!({"tab":tab})))
    }
}

fn local_service(address: SocketAddr, cwd: &PathBuf) -> (Arc<AppServer>, u64) {
    std::fs::create_dir_all(cwd).unwrap();
    std::fs::write(cwd.join("fixture-address"), address.to_string()).unwrap();
    let mut config = values();
    for (key, value) in [
        ("executable", json!(std::env::current_exe().unwrap())),
        ("cwd", json!(cwd)),
        ("web", json!(format!("http://{address}"))),
        ("arg0", json!("--ignored")),
        ("arg1", json!("--exact")),
        ("arg2", json!(CHILD_TEST)),
        ("arg3", json!("--nocapture")),
    ] {
        config.insert(key.into(), value);
    }
    let (server, revision) = fixture_with_service(
        config,
        json!({
            "health_path":"/health","mcp_path":"/mcp","model_mode":"platform_chat",
        }),
    );
    let enabled =
        crate::api::capabilities::change_import_owned(&server, "fixture.qq", revision, "enable")
            .unwrap();
    (server, enabled.revision)
}

#[tokio::test]
async fn unexpected_plugin_process_exit_leaves_the_control_plane_available() {
    let cwd = std::env::temp_dir().join(format!("yilian-plugin-exit-{}", Uuid::new_v4()));
    let address = free_address();
    let (server, _) = local_service(address, &cwd);
    let ensure = format!("{PATH}/ensure");
    let (code, pending) = request(server.clone(), Method::POST, &ensure, json!({}), true).await;
    assert_eq!(code, StatusCode::CONFLICT);
    let (code, started)=request(server.clone(),Method::POST,&ensure,
        json!({"allow_model":true,"launch":pending["launch"],"configuration_revision":pending["configuration_revision"]}),true).await;
    assert_eq!(code, StatusCode::OK, "{started}");
    std::fs::write(cwd.join("fixture-exit"), "quit fixture only").unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let _ = client.get(format!("http://{address}/health")).send().await;
    let exited = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let status = server
                .plugin_services
                .status("fixture.qq", "napcat")
                .unwrap();
            if status.state == "exited" {
                break status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(exited.exit_code, Some(23));
    let (code, health) = request(server, Method::GET, "/api/health", Value::Null, false).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(health["database"], "healthy");
    std::fs::remove_dir_all(cwd).unwrap();
}

#[tokio::test]
async fn managed_http_service_shares_browser_mcp_and_model_runtime() {
    let cwd = std::env::temp_dir().join(format!("yilian-managed-plugin-{}", Uuid::new_v4()));
    let (server, revision) = local_service(free_address(), &cwd);
    let native = Arc::new(NativeTabFixture::default());
    server.browser_runtime.install_host(native.clone()).unwrap();

    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    {
        let mut config = server.config.write();
        config.model.provider = "ollama".into();
        config.model.name = "global-fallback-must-not-be-used".into();
        config.model.api_key_env.clear();
        config.model.api_key_ref = None;
        config.model.base_url = format!("http://{}/v1", provider.local_addr().unwrap());
        config.permissions.mode = crate::config::types::PermissionMode::FullAccess;
    }
    let selected_profile = server
        .db
        .create_llm_model(&crate::db::LlmModelInput {
            id: "turn-model-profile".into(),
            provider: "ollama".into(),
            label: "turn fixture".into(),
            model: "fixture-model".into(),
            base_url: format!("http://{}/v1", provider.local_addr().unwrap()),
            api_format: "openai".into(),
            api_key_ref: "fixture.plugin.model".into(),
            api_key_env: String::new(),
            temperature: 0.0,
            max_tokens: 100,
            invoke_timeout_ms: 3000,
        })
        .unwrap();
    let provider_task = tokio::spawn(async move {
        let fixtures = Router::new().route("/v1/chat/completions", post(|Json(payload): Json<Value>| async move {
            assert!(matches!(payload["model"].as_str(), Some("fixture-model" | "child-fixture-model")));
            if payload["stream"] == true {
                return ([ ("content-type", "text/event-stream") ], concat!(
                    "data: {\"id\":\"resume\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"resumed\"},\"finish_reason\":\"stop\"}]}\n\n",
                    "data: [DONE]\n\n"
                )).into_response();
            }
            assert_eq!(payload["response_format"]["type"], "json_object");
            assert_eq!(payload["max_tokens"], 5);
            let reported_model = format!("{}-v1", payload["model"].as_str().unwrap());
            Json(json!({"id":"provider-fixture","model":reported_model,"choices":[{"index":0,"message":{"role":"assistant","content":"{\"verified\":true}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}})).into_response()
        }));
        axum::serve(provider, fixtures).await.unwrap();
    });

    let ensure = format!("{PATH}/ensure");
    let (code, pending) = request(
        server.clone(),
        Method::POST,
        &ensure,
        json!({"acquire_page":true}),
        true,
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT);
    let approval = json!({"acquire_page":true,"allow_model":true,"launch":pending["launch"],"configuration_revision":pending["configuration_revision"]});
    let (first, second) = tokio::join!(
        request(
            server.clone(),
            Method::POST,
            &ensure,
            approval.clone(),
            true
        ),
        request(server.clone(), Method::POST, &ensure, approval, true),
    );
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second.0, StatusCode::OK, "{}", second.1);
    assert_eq!(first.1["instance_id"], second.1["instance_id"]);
    assert_eq!(first.1["pid"], second.1["pid"]);
    assert_eq!(second.1["service_health"], "ready");
    assert_eq!(native.tabs().len(), 2);
    assert!(native
        .tabs()
        .iter()
        .all(|tab| tab.title == second.1["pid"].to_string()));

    let server_id = second.1["mcp_server_id"].as_str().unwrap().to_owned();
    assert!(server.db.get_mcp_server(&server_id).unwrap().is_none());
    assert_eq!(
        server
            .mcp_runtime_manager
            .get_server(&server_id)
            .unwrap()
            .status
            .as_str(),
        "ready"
    );
    let registry = server.build_agent_tool_registry().await;
    let tool = registry
        .list_tools()
        .into_iter()
        .find(|tool| tool.description.contains("identify_instance"))
        .unwrap()
        .name;
    let conv = server
        .db
        .create_conversation("plugin model fixture")
        .unwrap()
        .id;
    let turn_id = server
        .db
        .accept_conversation_turn(&crate::db::MessageRow {
            id: "origin-user-message".into(),
            conversation_id: conv.clone(),
            role: "user".into(),
            content: "fixture turn".into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_result: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();
    server
        .db
        .save_chat_options(
            &conv,
            &crate::api::chat::options::ChatRequestOptions {
                model_profile_id: Some("unrelated-conversation-preference".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let turn = ChatTurnModel {
        conversation_id: conv.clone(),
        turn_id: turn_id.clone(),
        profile_id: Some(selected_profile.id.clone()),
    };
    let first_turn = turn.clone();
    let invocation = SecurityExecutionRequest {
        conversation_id: conv.clone(),
        tool_call_id: "fixture-call".into(),
        tool_name: tool.clone(),
        arguments: json!({"wait":true}),
        subject: SecuritySubject::local_user(),
    };
    let gateway =
        crate::safety::production_gateway::build(&server, &server.workspace_root, registry, None);
    let owner = ExecutionOwner::Conversation {
        conversation_id: conv.clone(),
    };
    let mcp_call = tokio::spawn(async move {
        CURRENT_CHAT_TURN_MODEL
            .scope(
                Some(first_turn),
                CURRENT_EXECUTION_OWNER.scope(
                    owner,
                    gateway.execute(&invocation, crate::tools::trait_def::RiskLevel::High),
                ),
            )
            .await
            .unwrap()
    });
    tokio::time::timeout(Duration::from_secs(8), async {
        while !cwd.join("mcp-inflight").exists() {
            assert!(!mcp_call.is_finished(), "gateway must dispatch the fixture");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    native.0.lock().clear();
    let (_, active) = request(server.clone(), Method::GET, PATH, Value::Null, true).await;
    assert_eq!(active["page_uses"], 0);
    assert_eq!(active["task_uses"], 1);
    assert_eq!(
        request(
            server.clone(),
            Method::POST,
            &format!("{PATH}/stop"),
            Value::Null,
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert!(crate::api::capabilities::change_import_owned(
        &server,
        "fixture.qq",
        revision,
        "disable"
    )
    .is_err());
    std::fs::write(cwd.join("mcp-continue"), "continue").unwrap();
    let SecurityExecutionOutcome::Executed { tool_result, .. } = mcp_call.await.unwrap() else {
        panic!("tool did not execute");
    };
    assert!(tool_result.ok, "{tool_result:?}");
    let data = tool_result.structured.unwrap();
    assert_eq!(data["pid"], second.1["pid"]);
    assert_eq!(data["wrong_purpose"], 400, "{data}");
    assert_eq!(data["proxy_status"], 200, "{data}");
    assert_eq!(data["replay_status"], 401, "{data}");
    assert_eq!(data["model_result"]["tool_call_id"], "fixture-call");
    assert_eq!(data["model_result"]["turn_id"], turn_id);
    assert_eq!(
        data["model_result"]["source"]["profile_id"],
        selected_profile.id
    );
    assert_eq!(
        data["model_result"]["source"]["reported_model"],
        "fixture-model-v1"
    );
    assert_eq!(data["model_result"]["usage"]["total_tokens"], 10);
    assert_eq!(data["model_result"]["output"]["verified"], true);

    server.config.write().permissions.mode = crate::config::types::PermissionMode::PartialApproval;
    {
        use crate::integrations::llm::types::{ChatMessage, ToolCall, ToolCallFunction};
        server
            .db
            .persist_chat_message(
                &conv,
                &ChatMessage {
                    role: "assistant".into(),
                    content: None,
                    reasoning_content: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "fixture-approved".into(),
                        call_type: "function".into(),
                        function: ToolCallFunction {
                            name: tool.clone(),
                            arguments: "{}".into(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                },
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap();
    }
    let (approval, _) = CURRENT_CHAT_TURN_MODEL
        .scope(Some(turn.clone()), async {
            server.approval_store.create_or_get_pending(
                conv.clone(),
                "fixture-approved".into(),
                tool.clone(),
                json!({}),
                crate::tools::RiskLevel::High,
                "fixture requested approval".into(),
                "local-user".into(),
            )
        })
        .await;
    let path = format!("/api/approvals/{}/approve", approval.approval_id);
    let reply = crate::api::build_router(server.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(path)
                .header(CONTROL_SESSION_HEADER, "a".repeat(64))
                .header("content-type", "application/json")
                .body(Body::from(json!({"conversation_id":conv}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    let events = to_bytes(reply.into_body(), usize::MAX).await.unwrap();
    let events = String::from_utf8_lossy(&events);
    assert!(
        events.contains("\"type\":\"tool_end\"")
            && events.contains("\"tool_call_id\":\"fixture-approved\""),
        "{events}"
    );
    let after_approval: Value =
        serde_json::from_slice(&std::fs::read(cwd.join("last-model-result")).unwrap()).unwrap();
    assert_eq!(after_approval["proxy_status"], 200, "{after_approval}");
    assert_eq!(
        after_approval["model_result"]["tool_call_id"],
        "fixture-approved"
    );
    assert_eq!(after_approval["model_result"]["turn_id"], turn_id);
    assert_eq!(
        after_approval["model_result"]["source"]["profile_id"],
        selected_profile.id
    );

    {
        let mut config = server.config.write();
        config.model.name = "fixture-model".into();
        config.permissions.mode = crate::config::types::PermissionMode::FullAccess;
    }
    goal_cases::child_uses_its_own_model(&server, &tool, &server_id, turn).await;
    goal_cases::goal_node_uses_the_persisted_proofloop_budget(&server, &tool, &cwd).await;

    std::fs::write(cwd.join("degraded"), "health failed").unwrap();
    let (code, _) = request(server.clone(), Method::POST, &ensure, json!({}), true).await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    let (_, degraded) = request(server.clone(), Method::GET, PATH, Value::Null, true).await;
    assert_eq!(degraded["service_health"], "degraded");
    assert_eq!(degraded["task_uses"], 0);
    let (code, stopped) = request(
        server.clone(),
        Method::POST,
        &format!("{PATH}/stop"),
        Value::Null,
        true,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(stopped["state"], "stopped");
    assert!(server.mcp_runtime_manager.get_server(&server_id).is_none());
    provider_task.abort();
    std::fs::remove_dir_all(cwd).unwrap();
}
