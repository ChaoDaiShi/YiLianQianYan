//! Real router, SQLite and production Gateway; the window provider is controlled.
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tower::ServiceExt;
use yilian_backend::{
    integrations::secret::InMemorySecretStore,
    safety::execution_gateway::SecurityExecutionOutcome,
    safety::{ControlSession, SecurityExecutionRequest, SecuritySubject},
    software::*,
    tools::RiskLevel,
    AppServer,
};

struct WindowFixture {
    identity: WindowIdentity,
    actions: AtomicUsize,
    launches: AtomicUsize,
}
impl WindowFixture {
    fn window(&self) -> WindowState {
        WindowState {
            identity: self.identity.clone(),
            mode: "independent".into(),
            visible: true,
            focused: false,
            minimized: false,
            embedding_supported: false,
            reason: Some("Controlled provider".into()),
        }
    }
}
impl SoftwareHost for WindowFixture {
    fn discover(&self, _: &Registration) -> Result<Vec<WindowCandidate>, SoftwareError> {
        let mut other = self.identity.clone();
        other.hwnd = "124".into();
        other.pid = 43;
        Ok(vec![
            WindowCandidate {
                identity: self.identity.clone(),
                visible: true,
                minimized: false,
            },
            WindowCandidate {
                identity: other,
                visible: true,
                minimized: false,
            },
        ])
    }
    fn launch(&self, _: &Registration) -> Result<WindowCandidate, SoftwareError> {
        self.launches.fetch_add(1, Ordering::SeqCst);
        Ok(WindowCandidate {
            identity: self.identity.clone(),
            visible: true,
            minimized: false,
        })
    }
    fn verify(&self, identity: &WindowIdentity) -> Result<WindowState, SoftwareError> {
        if identity != &self.identity {
            return Err(SoftwareError::new(
                "target_changed",
                "Controlled identity differs",
            ));
        }
        Ok(self.window())
    }
    fn present(
        &self,
        identity: &WindowIdentity,
        _: &str,
        _: Option<Viewport>,
    ) -> Result<WindowState, SoftwareError> {
        self.verify(identity)
    }
    fn release(&self, _: &WindowIdentity) -> Result<(), SoftwareError> {
        Ok(())
    }
    fn observe(
        &self,
        identity: &WindowIdentity,
        _: &str,
        _: bool,
    ) -> Result<Vec<SoftwareControl>, SoftwareError> {
        self.verify(identity)?;
        Ok(vec![])
    }
    fn control(
        &self,
        identity: &WindowIdentity,
        _: &SoftwareControl,
        _: &str,
        _: Option<&str>,
        cancel: &AtomicBool,
    ) -> Result<(), SoftwareError> {
        self.verify(identity)?;
        if cancel.load(Ordering::Acquire) {
            return Err(SoftwareError::new("cancelled", "Cancelled"));
        }
        self.actions.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn shutdown(&self) -> Result<(), SoftwareError> {
        Ok(())
    }
}
struct Fixture {
    root: PathBuf,
    server: Arc<AppServer>,
    host: Arc<WindowFixture>,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-software-http-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let server = Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("app.db"),
                root.to_str().unwrap(),
                ControlSession::generate(),
                Arc::new(InMemorySecretStore::new()),
            )
            .unwrap(),
        );
        let host = Arc::new(WindowFixture {
            identity: WindowIdentity {
                hwnd: "123".into(),
                pid: 42,
                process_started_at: 134000000000000001,
                executable: std::env::current_exe()
                    .unwrap()
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into(),
                class_name: "OwnedFixture".into(),
            },
            actions: AtomicUsize::new(0),
            launches: AtomicUsize::new(0),
        });
        server.software_runtime.install_host(host.clone());
        Self { root, server, host }
    }
    async fn call(&self, method: &str, path: &str, body: Value, authorized: bool) -> (u16, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if authorized {
            request = request.header(
                "x-yilian-control-session",
                self.server.control_session.token(),
            );
        }
        let response = yilian_backend::app::router::build_router(self.server.clone())
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    async fn register(&self, allow: bool) -> Value {
        let(status,value)=self.call("POST","/api/software/registrations",json!({"name":"Controlled software","adapter":"generic_fixture",
            "executable":self.host.identity.executable,"workspace_id":null,"allow_observe":allow,"allow_actions":allow}),true).await;
        assert_eq!(status, 200, "{value}");
        value
    }
    fn cleanup(self) {
        let root = self.root;
        drop(self.server);
        drop(self.host);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn software_http_auth_and_browser_safe_identity_roundtrip_are_enforced() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture
            .call("GET", "/api/software", Value::Null, false)
            .await
            .0,
        401
    );
    let (status, _) = fixture
        .call(
            "POST",
            "/api/software/sessions/missing/wechat-frame",
            json!({"generation": 1}),
            false,
        )
        .await;
    assert_eq!(status, 401);
    let registration = fixture.register(false).await;
    assert_eq!(registration["allow_observe"], false);
    assert_eq!(registration["allow_actions"], false);
    let id = registration["id"].as_str().unwrap();
    let (status, candidates) = fixture
        .call(
            "POST",
            &format!("/api/software/registrations/{id}/discover"),
            json!({}),
            true,
        )
        .await;
    assert_eq!(status, 200, "{candidates}");
    assert_eq!(candidates["candidates"].as_array().unwrap().len(), 2);
    let identity = candidates["candidates"][0]["identity"].clone();
    assert_eq!(identity["process_started_at"], "134000000000000001");
    let operation = uuid::Uuid::new_v4().to_string();
    let request = json!({"operation_id":operation,"identity":identity});
    let (status, session) = fixture
        .call(
            "POST",
            &format!("/api/software/registrations/{id}/attach"),
            request.clone(),
            true,
        )
        .await;
    assert_eq!(status, 200, "{session}");
    let (status, duplicate) = fixture
        .call(
            "POST",
            &format!("/api/software/registrations/{id}/attach"),
            request,
            true,
        )
        .await;
    assert_eq!(status, 200, "{duplicate}");
    assert_eq!(session["id"], duplicate["id"]);
    let mut unsafe_identity = identity;
    unsafe_identity["process_started_at"] = json!(134000000000000001u64);
    assert_eq!(
        fixture
            .call(
                "POST",
                &format!("/api/software/registrations/{id}/attach"),
                json!({"operation_id":uuid::Uuid::new_v4().to_string(),"identity":unsafe_identity}),
                true
            )
            .await
            .0,
        422
    );
    let target: Session = serde_json::from_value(session).unwrap();
    assert!(fixture
        .server
        .software_runtime
        .ensure_ai_allowed(&target.target(), "observe")
        .is_err());
    fixture.cleanup();
}

#[tokio::test]
async fn software_http_launch_requires_the_exact_reviewed_registration() {
    let fixture = Fixture::new();
    let registration = fixture.register(false).await;
    let id = registration["id"].as_str().unwrap();
    let(status,body)=fixture.call("POST",&format!("/api/software/registrations/{id}/launch"),json!({
        "operation_id":uuid::Uuid::new_v4().to_string(),"confirmed":true,"revision":0,"expected_executable":registration["executable"]}),true).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(fixture.host.launches.load(Ordering::SeqCst), 0);
    let(status,body)=fixture.call("POST",&format!("/api/software/registrations/{id}/launch"),json!({
        "operation_id":uuid::Uuid::new_v4().to_string(),"confirmed":false,"revision":registration["revision"],"expected_executable":registration["executable"]}),true).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(fixture.host.launches.load(Ordering::SeqCst), 0);
    fixture.cleanup();
}

#[tokio::test]
async fn software_production_gateway_gates_high_risk_and_rechecks_revocation() {
    let fixture = Fixture::new();
    let registration = fixture.register(true).await;
    let runtime = &fixture.server.software_runtime;
    let session = runtime
        .attach(
            registration["id"].as_str().unwrap(),
            fixture.host.identity.clone(),
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
    fixture.server.config.write().permissions.mode =
        yilian_backend::config::types::PermissionMode::FullApproval;
    let gateway = yilian_backend::safety::production_gateway::build(
        &fixture.server,
        fixture.root.to_str().unwrap(),
        fixture.server.tool_registry.clone(),
        None,
    );
    let request = SecurityExecutionRequest {
        conversation_id: "controlled-software".into(),
        tool_call_id: "observe".into(),
        tool_name: "software_observe".into(),
        arguments: json!({"target":session.target()}),
        subject: SecuritySubject::local_user(),
    };
    let outcome = gateway.execute(&request, RiskLevel::High).await.unwrap();
    assert!(
        matches!(outcome, SecurityExecutionOutcome::RequiresApproval { .. }),
        "{outcome:?}"
    );
    assert_eq!(fixture.host.actions.load(Ordering::SeqCst), 0);
    runtime.stop(session.target()).unwrap();
    let outcome = gateway.execute_approved(&request, RiskLevel::High).await;
    assert!(
        !matches!(outcome, Ok(SecurityExecutionOutcome::Executed { .. })),
        "A stale approval executed after stop"
    );
    assert_eq!(fixture.host.actions.load(Ordering::SeqCst), 0);
    drop(gateway);
    fixture.cleanup();
}
