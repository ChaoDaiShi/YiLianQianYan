use super::*;
use crate::safety::grant::AuthorizedResource;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn context(host: &str, port: u16, zone: NetworkZone) -> ToolExecutionContext {
    ToolExecutionContext::new_with_resources(
        Arc::new(crate::isolation::ManagedProcessRegistry::new()),
        "http-regression",
        vec![AuthorizedResource::Network {
            scheme: "http".into(),
            host: host.into(),
            port,
            method: "GET".into(),
            zone,
            grant_id: None,
            one_shot_approval: false,
        }],
    )
}

#[test]
fn mapped_ipv4_addresses_keep_their_network_zone() {
    for (ip, zone) in [
        ("::ffff:127.0.0.1", NetworkZone::Loopback),
        ("::ffff:10.1.2.3", NetworkZone::Private),
        ("::ffff:192.168.1.1", NetworkZone::Private),
        ("::ffff:169.254.169.254", NetworkZone::Private),
        ("::ffff:93.184.216.34", NetworkZone::Public),
        ("::1", NetworkZone::Loopback),
        ("2606:4700:4700::1111", NetworkZone::Public),
    ] {
        assert_eq!(classify_ip(ip.parse().unwrap()), zone, "{ip}");
    }
}

#[tokio::test]
async fn public_dns_grant_cannot_reach_mapped_loopback() {
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/",
        get(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            async { "local-only fixture" }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let tool = HttpRequestTool::with_resolver(Arc::new(StaticResolver {
        addresses: vec![SocketAddr::new(
            std::net::Ipv4Addr::LOCALHOST.to_ipv6_mapped().into(),
            address.port(),
        )],
        calls: Arc::new(AtomicUsize::new(0)),
    }));
    let result = tool.execute_with_context(
        serde_json::json!({"url": format!("http://audit.example:{}/", address.port()), "timeout_ms": 1000}),
        &context("audit.example", address.port(), NetworkZone::Public),
    ).await;
    server.abort();
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "public grant reached a local service"
    );
    assert!(!result.ok, "{}", result.content);
    assert!(result.content.contains("SSRF"), "{}", result.content);
}

#[tokio::test]
async fn request_failure_does_not_disclose_url_query() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 2048];
        socket.read(&mut request).await.unwrap();
        socket
            .write_all(b"not an HTTP response\r\n\r\n")
            .await
            .unwrap();
    });
    let tool = HttpRequestTool::new();
    let result = tool.execute_with_context(
        serde_json::json!({"url": format!("http://127.0.0.1:{}/?api_key=audit-fixture-sensitive", address.port()), "timeout_ms": 1000}),
        &context("127.0.0.1", address.port(), NetworkZone::Loopback),
    ).await;
    server.await.unwrap();
    assert!(!result.ok);
    assert!(
        !result.content.contains("audit-fixture-sensitive"),
        "query leaked into tool output"
    );
    assert!(!result.content.contains("api_key="));
}

#[tokio::test]
async fn interrupted_response_body_is_not_reported_as_success() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 2048];
        socket.read(&mut request).await.unwrap();
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial",
            )
            .await
            .unwrap();
    });
    let result = HttpRequestTool::new().execute_with_context(
        serde_json::json!({"url": format!("http://127.0.0.1:{}/?api_key=audit-fixture-sensitive", address.port()), "timeout_ms": 1000}),
        &context("127.0.0.1", address.port(), NetworkZone::Loopback),
    ).await;
    server.await.unwrap();
    assert!(
        !result.ok,
        "truncated response was reported as success: {}",
        result.content
    );
    assert!(!result.content.contains("audit-fixture-sensitive"));
}
