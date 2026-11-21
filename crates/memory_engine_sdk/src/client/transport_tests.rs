use super::{
    append_optional_bool_query, append_optional_i64_query, append_optional_query,
    normalize_base_url, should_retry_send_error, CONNECT_RETRY_DELAYS, SOURCE_SCOPE,
    TOKEN_AUDIENCE,
};
use crate::MemoryEngineClient;
use std::time::Duration;

#[test]
fn normalize_base_url_appends_default_api_prefix() {
    assert_eq!(
        normalize_base_url("http://localhost:3000/".to_string()),
        "http://localhost:3000/api/memory-engine/v1"
    );
}

#[test]
fn normalize_base_url_preserves_existing_memory_engine_path() {
    assert_eq!(
        normalize_base_url("http://localhost:3000/api/memory-engine/v1///".to_string()),
        "http://localhost:3000/api/memory-engine/v1"
    );
    assert_eq!(
        normalize_base_url("http://localhost:3000/custom/api/memory-engine/v2".to_string()),
        "http://localhost:3000/custom/api/memory-engine/v2"
    );
    assert_eq!(
        normalize_base_url("https://gateway.example/api/memory/".to_string()),
        "https://gateway.example/api/memory"
    );
}

#[test]
fn append_optional_query_helpers_build_expected_query_string() {
    let mut query = "tenant_id=tenant-1".to_string();
    append_optional_query(&mut query, "record_type", Some("summary rollup"));
    append_optional_i64_query(&mut query, "limit", Some(20));
    append_optional_bool_query(&mut query, "active", Some(true));

    assert_eq!(
        query,
        "tenant_id=tenant-1&record_type=summary%20rollup&limit=20&active=true"
    );
}

#[test]
fn append_optional_query_helpers_ignore_empty_inputs() {
    let mut query = String::new();
    append_optional_query(&mut query, "record_type", Some("   "));
    append_optional_query(&mut query, "role", None);
    append_optional_i64_query(&mut query, "limit", None);
    append_optional_bool_query(&mut query, "active", None);

    assert!(query.is_empty());
}

#[tokio::test]
async fn connect_errors_are_retried_only_within_the_bounded_budget() {
    let error = reqwest::Client::new()
        .get("http://127.0.0.1:0/connect-retry-test")
        .send()
        .await
        .expect_err("unused local port should refuse the connection");

    assert!(should_retry_send_error(&error, 0, false));
    assert!(should_retry_send_error(
        &error,
        CONNECT_RETRY_DELAYS.len() - 1,
        false
    ));
    assert!(!should_retry_send_error(
        &error,
        CONNECT_RETRY_DELAYS.len(),
        false
    ));
}

#[tokio::test]
async fn send_json_recovers_when_the_service_starts_during_connect_backoff() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve port");
    let address = reservation.local_addr().expect("reserved address");
    drop(reservation);
    let server = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let listener = TcpListener::bind(address).expect("bind delayed server");
        let (mut stream, _) = listener.accept().expect("accept retried request");
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).expect("read request");
        stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                )
                .expect("write response");
    });
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .expect("loopback test client");
    let client = MemoryEngineClient::new_platform_with_http_client(
        format!("http://{address}/api/memory-engine/v1"),
        http,
    );

    let response: serde_json::Value = client
        .send_json(reqwest::Method::GET, "/retry-probe", Option::<&()>::None)
        .await
        .expect("connect retry should recover");

    assert_eq!(response, serde_json::json!({}));
    server.join().expect("delayed server");
}

#[test]
fn with_bearer_token_stores_authorization_header_value() {
    let client = MemoryEngineClient::new_platform("http://localhost:3000", Duration::from_secs(5))
        .expect("client")
        .with_bearer_token(" user-token-1 ");

    let request = client
        .apply_auth(
            client.http.get("http://localhost:3000/test"),
            "/test",
            client.current_access_token().as_deref(),
        )
        .expect("apply auth")
        .build()
        .expect("request");

    assert_eq!(
        request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer user-token-1")
    );
}

#[test]
fn internal_service_auth_issues_scoped_token_without_static_secret() {
    let client = MemoryEngineClient::new_platform("http://localhost:3000", Duration::from_secs(5))
        .expect("client")
        .with_internal_service_auth("task-runner", "a-long-task-runner-memory-secret");
    let request = client
        .apply_auth(
            client.http.get("http://localhost:3000/admin/sources"),
            "/admin/sources",
            None,
        )
        .expect("apply auth")
        .build()
        .expect("request");
    assert!(!request.headers().contains_key("x-memory-operator-token"));
    assert_eq!(
        request
            .headers()
            .get("x-memory-caller")
            .and_then(|value| value.to_str().ok()),
        Some("task-runner")
    );
    let token = request
        .headers()
        .get("x-memory-internal-token")
        .and_then(|value| value.to_str().ok())
        .expect("internal token");
    chatos_service_runtime::verify_internal_service_token(
        token,
        "a-long-task-runner-memory-secret",
        "task-runner",
        TOKEN_AUDIENCE,
        SOURCE_SCOPE,
    )
    .expect("valid token");
}

#[tokio::test]
async fn unauthorized_memory_request_refreshes_once_and_reuses_the_rotated_token() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
    let address = listener.local_addr().expect("server address");
    let captured = Arc::new(Mutex::new(Vec::<String>::new()));
    let server_captured = Arc::clone(&captured);
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let count = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..count]).to_string();
            server_captured
                .lock()
                .expect("captured")
                .push(request.clone());
            let first_line = request.lines().next().unwrap_or_default();
            let authorization = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
                .unwrap_or_default();
            let (status, body) = if first_line.contains("/api/user/auth/refresh") {
                assert!(authorization.contains("Bearer expired-token"));
                ("200 OK", r#"{"token":"renewed-token"}"#)
            } else if authorization.contains("Bearer renewed-token") {
                ("200 OK", r#"{"ok":true}"#)
            } else {
                ("401 Unauthorized", r#"{"error":"expired"}"#)
            };
            write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
        }
    });
    let client = MemoryEngineClient::new_direct(
        format!("http://{address}/api/memory"),
        Duration::from_secs(2),
        "chatos",
    )
    .expect("client")
    .with_bearer_token("expired-token")
    .with_bearer_token_refresh_endpoint(format!("http://{address}/api/user/auth/refresh"));

    for _ in 0..2 {
        let response: serde_json::Value = client
            .send_json(reqwest::Method::GET, "/probe", Option::<&()>::None)
            .await
            .expect("request should recover");
        assert_eq!(response, serde_json::json!({"ok": true}));
    }

    server.join().expect("server");
    let requests = captured.lock().expect("captured");
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /api/user/auth/refresh "))
            .count(),
        1
    );
    assert!(requests[3].contains("Bearer renewed-token"));
}

#[tokio::test]
async fn forbidden_memory_request_does_not_refresh() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
    let address = listener.local_addr().expect("server address");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).expect("read request");
        stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 21\r\nConnection: close\r\n\r\n{\"error\":\"forbidden\"}",
                )
                .expect("write response");
    });
    let client = MemoryEngineClient::new_direct(
        format!("http://{address}/api/memory"),
        Duration::from_secs(2),
        "chatos",
    )
    .expect("client")
    .with_bearer_token("valid-token")
    .with_bearer_token_refresh_endpoint(format!("http://{address}/api/user/auth/refresh"));

    let error = client
        .send_json::<serde_json::Value, ()>(reqwest::Method::GET, "/probe", None)
        .await
        .expect_err("403 must remain a permission error");

    assert!(error.contains("status=403 Forbidden"));
    server.join().expect("server");
}

#[tokio::test]
async fn response_header_rotates_the_shared_token_for_all_client_clones() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
    let address = listener.local_addr().expect("server address");
    let server = std::thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 2048];
            let count = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..count]);
            if index == 0 {
                assert!(request.contains("Bearer current-token"));
                stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Access-Token: proactive-token\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                        )
                        .expect("write first response");
            } else {
                assert!(request.contains("Bearer proactive-token"));
                stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                        )
                        .expect("write second response");
            }
        }
    });
    let client = MemoryEngineClient::new_direct(
        format!("http://{address}/api/memory"),
        Duration::from_secs(2),
        "chatos",
    )
    .expect("client")
    .with_bearer_token("current-token");
    let clone = client.clone();

    let _: serde_json::Value = client
        .send_json(reqwest::Method::GET, "/probe", Option::<&()>::None)
        .await
        .expect("first request");
    let _: serde_json::Value = clone
        .send_json(reqwest::Method::GET, "/probe", Option::<&()>::None)
        .await
        .expect("clone request");

    server.join().expect("server");
}
