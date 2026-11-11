// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use memory_engine_sdk::SdkCountThreadRecordsRequest;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

struct AssertionServer {
    base_url: String,
    requests: Arc<Mutex<Vec<(String, String, Value)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AssertionServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut expected_token = "renewed-token";
            while !stopping.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("accept request: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let path = first.split_whitespace().nth(1).unwrap().to_string();
                let mut authorization = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("authorization") {
                            authorization = value.trim().to_string();
                        }
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let body: Value = if bytes.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes).unwrap()
                };
                captured
                    .lock()
                    .unwrap()
                    .push((path.clone(), authorization.clone(), body.clone()));
                let (status, body, header) = if path == "/api/user/auth/refresh" {
                    assert_eq!(authorization, "Bearer expired-token");
                    ("200 OK", json!({"token":"renewed-token"}), "")
                } else if authorization != format!("Bearer {expected_token}") {
                    ("401 Unauthorized", json!({"error":"expired"}), "")
                } else if path.starts_with("/api/memory/") {
                    expected_token = "rotated-token";
                    (
                        "200 OK",
                        json!({"count":0}),
                        "x-access-token: rotated-token\r\n",
                    )
                } else {
                    assert_eq!(path, "/api/local/runtime-context-assertions");
                    (
                        "200 OK",
                        json!({
                            "runtime_context_assertion":"signed-assertion",
                            "source_id":body["source_id"],
                            "canonical_agent_subject_id":body["canonical_agent_subject_id"],
                            "thread_id":body["thread_id"],
                            "project_partition_key":body["project_id"].as_str().map(|id| format!("project:{id}")).unwrap_or_else(|| "__none__".into()),
                            "runtime_identity_revision":"revision-1"
                        }),
                        "",
                    )
                };
                let body = body.to_string();
                // A rejected caller may close as soon as it receives the status.
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{header}Connection: close\r\n\r\n{body}", body.len());
            }
        });
        Self {
            base_url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for AssertionServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

#[tokio::test]
async fn assertion_and_memory_share_refresh_and_rotation_across_projects_and_operations() {
    let server = AssertionServer::start();
    let config = LocalMemoryRuntimeConfig::new(
        format!("{}/api/memory", server.base_url),
        "chatos",
        Duration::from_secs(2),
    )
    .with_access_token(Some("expired-token".into()));
    let client = config.build_client().unwrap();
    let provider = LocalConnectorRuntimeContextAssertionProvider::new(
        runtime_context_assertion_endpoint(&config.base_url).unwrap(),
        client.clone(),
    );
    for (index, (project_partition_key, operation)) in [
        ("project:first", "compose"),
        ("project:second", "record_ingest"),
        ("__none__", "summarize"),
        ("project:third", "repair"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            provider
                .issue(RuntimeContextAssertionRequest {
                    principal_id: "owner-1",
                    tenant_id: "owner-1",
                    source_id: "chatos",
                    canonical_agent_subject_id: "agent:jiguli",
                    thread_id: "client-agent:agent:jiguli",
                    project_partition_key,
                    operation,
                })
                .await
                .unwrap(),
            "signed-assertion"
        );
        if index == 0 {
            let result = client
                .count_thread_records(
                    "client-agent:agent:jiguli",
                    &SdkCountThreadRecordsRequest {
                        tenant_id: "owner-1".into(),
                        role: None,
                        record_type: None,
                        summary_status: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(result, 0);
        }
    }
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 7);
    assert_eq!(
        requests
            .iter()
            .filter(|(path, _, _)| path == "/api/user/auth/refresh")
            .count(),
        1
    );
    assert_eq!(requests[0].1, "Bearer expired-token");
    assert_eq!(requests[2].1, "Bearer renewed-token");
    assert!(requests[4..]
        .iter()
        .all(|(_, token, _)| token == "Bearer rotated-token"));
}

#[tokio::test]
async fn concurrent_project_assertions_refresh_the_shared_session_once() {
    let server = AssertionServer::start();
    let config = LocalMemoryRuntimeConfig::new(
        format!("{}/api/memory", server.base_url),
        "chatos",
        Duration::from_secs(2),
    )
    .with_access_token(Some("expired-token".into()));
    let provider = LocalConnectorRuntimeContextAssertionProvider::new(
        runtime_context_assertion_endpoint(&config.base_url).unwrap(),
        config.build_client().unwrap(),
    );
    let request = |project_partition_key| RuntimeContextAssertionRequest {
        principal_id: "owner-1",
        tenant_id: "owner-1",
        source_id: "chatos",
        canonical_agent_subject_id: "agent:jiguli",
        thread_id: "client-agent:agent:jiguli",
        project_partition_key,
        operation: "compose",
    };
    let (first, second) = tokio::join!(
        provider.issue(request("project:first")),
        provider.issue(request("project:second")),
    );
    assert_eq!(first.unwrap(), "signed-assertion");
    assert_eq!(second.unwrap(), "signed-assertion");
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|(path, _, _)| path == "/api/user/auth/refresh")
            .count(),
        1
    );
}
