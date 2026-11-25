use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

#[test]
fn address_in_use_is_classified_as_port_unavailable() {
    assert_eq!(
        classify_backend_start_error("Address already in use (os error 10048)"),
        StartupDiagnosticCategory::BackendPortUnavailable,
    );
}

#[test]
fn other_start_errors_are_classified_as_spawn_failed() {
    assert_eq!(
        classify_backend_start_error("permission denied"),
        StartupDiagnosticCategory::BackendSpawnFailed,
    );
}

#[test]
fn startup_diagnostics_use_static_redacted_messages() {
    let diagnostic = StartupDiagnostic::new(StartupDiagnosticCategory::BackendHealthTimeout);

    assert_eq!(diagnostic.category.as_str(), "BackendHealthTimeout");
    assert_eq!(diagnostic.message, "Local backend health check timed out",);
    assert!(diagnostic.timestamp > 0);
}

#[test]
fn health_probe_accepts_only_the_expected_healthy_service() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 512];
        let _ = stream.read(&mut request).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"status\":\"healthy\",\"service\":\"yilian-backend\",\"database\":\"healthy\"}",
            )
            .unwrap();
    });

    assert!(probe_backend_health(port, Duration::from_secs(1)));
    server.join().unwrap();
}

#[test]
fn health_probe_rejects_an_unrelated_service() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 512];
        let _ = stream.read(&mut request).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"status\":\"healthy\",\"service\":\"other\",\"database\":\"healthy\"}",
            )
            .unwrap();
    });

    assert!(!probe_backend_health(port, Duration::from_secs(1)));
    server.join().unwrap();
}
