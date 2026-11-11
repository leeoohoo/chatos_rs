// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

#[tokio::test]
async fn sibling_requests_reject_foreign_origins_and_non_user_credentials() {
    let client = MemoryEngineClient::new_direct(
        "https://gateway.example/api/memory",
        Duration::from_secs(1),
        "chatos",
    )
    .unwrap()
    .with_bearer_token("user-token");
    for url in [
        "https://foreign.example/api/local/runtime-context-assertions",
        "http://gateway.example/api/local/runtime-context-assertions",
        "https://gateway.example:8443/api/local/runtime-context-assertions",
        "https://username:password@gateway.example/api/local/runtime-context-assertions",
        "https://gateway.example/api/local/runtime-context-assertions#fragment",
    ] {
        let error = client
            .post_with_user_auth::<Value, _>(url, &json!({}))
            .await
            .unwrap_err();
        assert!(error.contains("origin"));
    }
    let endpoint = "https://gateway.example/api/local/runtime-context-assertions";
    let no_user = MemoryEngineClient::new_direct(
        "https://gateway.example/api/memory",
        Duration::from_secs(1),
        "chatos",
    )
    .unwrap();
    let system = MemoryEngineClient::new_system(
        "https://gateway.example/api/memory",
        Duration::from_secs(1),
        "system",
        "secret",
    )
    .unwrap()
    .with_bearer_token("user-token");
    let internal = client.with_internal_service_auth("service", "secret");
    for client in [no_user, system, internal] {
        assert!(client
            .post_with_user_auth::<Value, _>(endpoint, &json!({}))
            .await
            .unwrap_err()
            .contains("user bearer session"));
    }
}

#[tokio::test]
async fn sibling_auth_recovery_is_bounded_and_does_not_refresh_forbidden_requests() {
    for (responses, expected_error, expected_refreshes) in [
        (
            vec![("401 Unauthorized", "{}"), ("401 Unauthorized", "{}")],
            "refresh failed",
            1,
        ),
        (
            vec![
                ("401 Unauthorized", "{}"),
                ("200 OK", r#"{"token":"new-token"}"#),
                ("401 Unauthorized", "{}"),
            ],
            "status=401",
            1,
        ),
        (vec![("403 Forbidden", "{}")], "status=403", 0),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                paths.push(first.split_whitespace().nth(1).unwrap().to_string());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            paths
        });
        let client = MemoryEngineClient::new_direct(
            format!("http://{address}/api/memory"),
            Duration::from_secs(2),
            "chatos",
        )
        .unwrap()
        .with_bearer_token("expired-token")
        .with_bearer_token_refresh_endpoint(format!("http://{address}/api/user/auth/refresh"));
        let error = client
            .post_with_user_auth::<Value, _>(
                &format!("http://{address}/api/local/runtime-context-assertions"),
                &json!({}),
            )
            .await
            .unwrap_err();
        assert!(error.contains(expected_error), "{error}");
        let paths = server.join().unwrap();
        assert_eq!(
            paths
                .iter()
                .filter(|path| *path == "/api/user/auth/refresh")
                .count(),
            expected_refreshes
        );
    }
}
