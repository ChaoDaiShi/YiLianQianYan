use super::http::{parse_request, read_head};
use super::policy::public_ip;
use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

#[test]
fn network_blocks_special_addresses_and_ambiguous_requests() {
    for ip in [
        "0.0.0.0",
        "127.0.0.1",
        "10.0.0.1",
        "100.64.0.1",
        "169.254.169.254",
        "192.168.1.1",
        "::1",
        "::ffff:127.0.0.1",
        "fc00::1",
        "2002:7f00:1::",
    ] {
        assert!(!public_ip(ip.parse().unwrap()), "{ip}");
    }
    assert!(public_ip("8.8.8.8".parse().unwrap()));
    assert!(parse_request(b"POST http://example.com/ HTTP/1.1\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n").is_err());
    assert!(parse_request(b"CONNECT user@localhost:443 HTTP/1.1\r\n\r\n").is_err());
    let request=parse_request(b"GET http://example.com/ HTTP/1.1\r\nHost: 127.0.0.1:9420\r\nX-Yilian-Control-Session: must-not-forward\r\n\r\n").unwrap();
    assert!(request.head.is_empty());
}
#[tokio::test]
async fn network_private_fetch_websocket_and_connect_denied_until_explicit_site() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let target = listener.local_addr().unwrap().port();
    let proxy = start(Vec::new(), Vec::new()).await.unwrap();
    for request in [
        format!("GET http://127.0.0.1:{target}/fetch HTTP/1.1\r\n\r\n"),
        format!("GET http://127.0.0.1:{target}/frame HTTP/1.1\r\n\r\n"),
        format!("GET http://127.0.0.1:{target}/ws HTTP/1.1\r\nUpgrade: websocket\r\n\r\n"),
        format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\n\r\n"),
    ] {
        let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port()))
            .await
            .unwrap();
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut reply = Vec::new();
        socket.read_to_end(&mut reply).await.unwrap();
        assert!(reply.starts_with(b"HTTP/1.1 403"));
    }
    assert!(timeout(Duration::from_millis(50), listener.accept())
        .await
        .is_err());
    proxy
        .authorize_local_origin(&format!("http://127.0.0.1:{target}"))
        .unwrap();
    assert!(proxy.allows(&format!("http://127.0.0.1:{target}/")));
    assert!(!proxy.allows("http://127.0.0.1:9420/"));
    assert!(proxy
        .authorize_local_origin("http://127.0.0.1:9420")
        .is_err());
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let head = read_head(&mut socket).await.unwrap();
        assert!(String::from_utf8(head)
            .unwrap()
            .starts_with("GET /ok HTTP/1.1"));
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .unwrap();
    });
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port()))
        .await
        .unwrap();
    socket
        .write_all(format!("GET http://127.0.0.1:{target}/ok HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut reply = Vec::new();
    socket.read_to_end(&mut reply).await.unwrap();
    assert!(reply.ends_with(b"ok"));
    peer.await.unwrap();
    proxy
        .revoke_local_origin(&format!("http://127.0.0.1:{target}"))
        .unwrap();
    assert!(!proxy.allows(&format!("http://127.0.0.1:{target}/")));
}
#[tokio::test]
async fn network_session_allowances_do_not_leak_and_drop_stops_listener() {
    let first = start(vec!["http://localhost:3000".into()], vec![3010])
        .await
        .unwrap();
    let second = start(Vec::new(), Vec::new()).await.unwrap();
    assert!(first.allows("http://localhost:3000"));
    assert!(!second.allows("http://localhost:3000"));
    assert!(!first.allows("http://localhost:3010"));
    assert!(first.allows("https://example.com"));
    assert!(!first.allows("file:///C:/Windows"));
    assert!(!first.allows("http://tauri.localhost"));
    let port = first.port();
    drop(first);
    for _ in 0..20 {
        if TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err()
        {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("dropped proxy kept listening");
}

#[tokio::test]
async fn network_reserved_destination_denied_even_if_initially_registered() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let target = listener.local_addr().unwrap().port();
    let proxy = start(vec![format!("http://127.0.0.1:{target}")], vec![target])
        .await
        .unwrap();
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port()))
        .await
        .unwrap();
    socket
        .write_all(format!("GET http://127.0.0.1:{target}/api/health HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 403"));
    assert!(timeout(Duration::from_millis(50), listener.accept())
        .await
        .is_err());
    for url in [
        "http://tauri.localhost",
        "http://nested.tauri.localhost",
        "http://asset.localhost.",
        "http://nested.ipc.localhost",
    ] {
        assert!(!proxy.allows(url));
    }
    for url in [
        " http://example.com",
        "http://example.com/has space",
        "http://a:b@example.com",
    ] {
        assert!(!proxy.allows(url));
    }
    assert!(proxy
        .browser_args()
        .contains("--proxy-bypass-list=<-loopback>"));
}

#[tokio::test]
async fn network_revoke_closes_live_connect_and_keeps_unrelated_policy_updates_safe() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let target = listener.local_addr().unwrap().port();
    let origin = format!("https://127.0.0.1:{target}");
    let proxy = start(vec![origin.clone()], Vec::new()).await.unwrap();
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        accepted_tx.send(()).unwrap();
        let mut ping = [0u8; 4];
        socket.read_exact(&mut ping).await.unwrap();
        assert_eq!(&ping, b"ping");
        socket.write_all(b"pong").await.unwrap();
        let mut next = [0u8; 1];
        timeout(Duration::from_secs(2), socket.read(&mut next))
            .await
            .unwrap()
            .unwrap()
    });
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port()))
        .await
        .unwrap();
    socket
        .write_all(format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    assert!(read_head(&mut socket)
        .await
        .unwrap()
        .starts_with(b"HTTP/1.1 200"));
    accepted_rx.await.unwrap();
    proxy
        .authorize_local_origin("http://localhost:3000")
        .unwrap();
    socket.write_all(b"ping").await.unwrap();
    let mut pong = [0u8; 4];
    socket.read_exact(&mut pong).await.unwrap();
    assert_eq!(&pong, b"pong");
    proxy.revoke_local_origin(&origin).unwrap();
    assert_eq!(upstream.await.unwrap(), 0);
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), socket.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(response.is_empty());
}

#[tokio::test]
async fn network_shutdown_interrupts_partial_chunked_body_without_forwarding_more() {
    for revoke in [false, true] {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let target = listener.local_addr().unwrap().port();
        let origin = format!("http://127.0.0.1:{target}");
        let proxy = start(vec![origin.clone()], Vec::new()).await.unwrap();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let upstream = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_head(&mut socket).await.unwrap();
            let mut prefix = [0u8; 5];
            socket.read_exact(&mut prefix).await.unwrap();
            assert_eq!(&prefix, b"4\r\nab");
            ready_tx.send(()).unwrap();
            let mut bytes = Vec::new();
            timeout(Duration::from_secs(2), socket.read_to_end(&mut bytes))
                .await
                .unwrap()
                .unwrap();
            bytes
        });
        let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port()))
            .await
            .unwrap();
        socket.write_all(format!("POST http://127.0.0.1:{target}/submit HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nab").as_bytes()).await.unwrap();
        ready_rx.await.unwrap();
        if revoke {
            proxy.revoke_local_origin(&origin).unwrap();
        } else {
            proxy.shutdown();
        }
        assert!(
            upstream.await.unwrap().is_empty(),
            "session close/revoke kept forwarding request data"
        );
        let _ = socket.write_all(b"cd\r\n0\r\n\r\n").await;
    }
}

#[tokio::test]
async fn network_websocket_upgrade_forwards_same_target_and_shutdown_closes_it() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let target = listener.local_addr().unwrap().port();
    let proxy = start(vec![format!("http://127.0.0.1:{target}")], Vec::new())
        .await
        .unwrap();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let head = String::from_utf8(read_head(&mut socket).await.unwrap()).unwrap();
        assert!(head.contains("Connection: Upgrade\r\n"));
        assert!(head.contains("upgrade: websocket\r\n"));
        socket.write_all(b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n").await.unwrap();
        let mut ping = [0u8; 4];
        socket.read_exact(&mut ping).await.unwrap();
        socket.write_all(&ping).await.unwrap();
        let mut next = [0u8; 1];
        timeout(Duration::from_secs(2), socket.read(&mut next))
            .await
            .unwrap()
            .unwrap()
    });
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port()))
        .await
        .unwrap();
    socket.write_all(format!("GET ws://127.0.0.1:{target}/socket HTTP/1.1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n").as_bytes()).await.unwrap();
    assert!(read_head(&mut socket)
        .await
        .unwrap()
        .starts_with(b"HTTP/1.1 101"));
    socket.write_all(b"ping").await.unwrap();
    let mut echo = [0u8; 4];
    socket.read_exact(&mut echo).await.unwrap();
    assert_eq!(&echo, b"ping");
    proxy.shutdown();
    assert_eq!(upstream.await.unwrap(), 0);
}
