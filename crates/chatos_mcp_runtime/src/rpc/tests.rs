// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::sync::mpsc;

use super::*;

#[tokio::test]
async fn dropping_http_tool_call_sends_cancel_with_the_same_request_id_and_headers() {
    #[derive(Clone)]
    struct Capture(mpsc::UnboundedSender<(Value, Option<String>)>);

    async fn mcp(
        axum::extract::State(capture): axum::extract::State<Capture>,
        headers: axum::http::HeaderMap,
        axum::Json(request): axum::Json<Value>,
    ) -> axum::Json<Value> {
        capture
            .0
            .send((
                request.clone(),
                headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned),
            ))
            .unwrap();
        if request.get("method").and_then(Value::as_str) == Some("tools/call") {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        axum::Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {}
        }))
    }

    let (sender, mut receiver) = mpsc::unbounded_channel();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new()
                .route("/mcp", axum::routing::post(mcp))
                .with_state(Capture(sender)),
        )
        .await
        .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let headers = HashMap::from([(
        "Authorization".to_string(),
        "Bearer runtime-token".to_string(),
    )]);
    let call = tokio::spawn(async move {
        jsonrpc_http_tool_call_cancellable(
            format!("http://{address}/mcp").as_str(),
            Some(&headers),
            json!({"name": "demo", "arguments": {}}),
            Some(Duration::from_secs(30)),
        )
        .await
    });
    let (tool_call, tool_authorization) =
        tokio::time::timeout(Duration::from_secs(5), receiver.recv())
            .await
            .unwrap()
            .unwrap();
    let request_id = tool_call.get("id").cloned().unwrap();
    assert_eq!(tool_authorization.as_deref(), Some("Bearer runtime-token"));
    call.abort();
    let (cancel, cancel_authorization) =
        tokio::time::timeout(Duration::from_secs(5), receiver.recv())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        cancel.get("method").and_then(Value::as_str),
        Some("notifications/cancelled")
    );
    assert_eq!(cancel.pointer("/params/requestId"), Some(&request_id));
    assert_eq!(
        cancel_authorization.as_deref(),
        Some("Bearer runtime-token")
    );
    server.abort();
}

#[tokio::test]
async fn accepted_http_tool_call_is_polled_until_completed() {
    #[derive(Clone)]
    struct PollState {
        poll_count: Arc<AtomicUsize>,
        poll_token: Arc<std::sync::Mutex<Option<String>>>,
        poll_caller: Arc<std::sync::Mutex<Option<String>>>,
        saw_secret: Arc<std::sync::Mutex<bool>>,
    }

    async fn mcp(axum::Json(request): axum::Json<Value>) -> axum::Json<Value> {
        axum::Json(json!({
            "jsonrpc": "2.0",
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "status": "accepted",
                "invocation_id": "invocation-1",
                "queued": true
            }
        }))
    }

    async fn invocation_status(
        axum::extract::State(state): axum::extract::State<PollState>,
        headers: axum::http::HeaderMap,
    ) -> axum::Json<Value> {
        *state.poll_token.lock().unwrap() = headers
            .get("x-mcp-management-internal-token")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        *state.poll_caller.lock().unwrap() = headers
            .get("x-mcp-management-caller-service")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        *state.saw_secret.lock().unwrap() =
            headers.contains_key("x-mcp-management-internal-secret");
        let poll_index = state.poll_count.fetch_add(1, Ordering::SeqCst);
        let body = if poll_index == 0 {
            json!({
                "invocation_id": "invocation-1",
                "session_id": "session-1",
                "caller_service": "task-runner",
                "resource_id": "resource-1",
                "exposed_tool_name": "demo_tool",
                "status": "running",
                "async_execution": true,
                "created_at_unix_ms": 1,
                "started_at_unix_ms": 2
            })
        } else {
            json!({
                "invocation_id": "invocation-1",
                "session_id": "session-1",
                "caller_service": "task-runner",
                "resource_id": "resource-1",
                "exposed_tool_name": "demo_tool",
                "status": "completed",
                "async_execution": true,
                "created_at_unix_ms": 1,
                "started_at_unix_ms": 2,
                "completed_at_unix_ms": 3,
                "terminal_result": {
                    "content": [
                        {
                            "type": "text",
                            "text": "done"
                        }
                    ]
                }
            })
        };
        axum::Json(body)
    }

    let state = PollState {
        poll_count: Arc::new(AtomicUsize::new(0)),
        poll_token: Arc::new(std::sync::Mutex::new(None)),
        poll_caller: Arc::new(std::sync::Mutex::new(None)),
        saw_secret: Arc::new(std::sync::Mutex::new(false)),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_state = state.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new()
                .route("/mcp", axum::routing::post(mcp))
                .route(
                    "/api/internal/runtime/invocations/invocation-1",
                    axum::routing::get(invocation_status),
                )
                .with_state(server_state),
        )
        .await
        .unwrap();
    });
    let headers = HashMap::from([
        (
            "authorization".to_string(),
            "Bearer runtime-token".to_string(),
        ),
        (
            "x-mcp-management-internal-secret".to_string(),
            "a-long-mcp-management-secret".to_string(),
        ),
        (
            "x-mcp-management-caller-service".to_string(),
            "task-runner".to_string(),
        ),
        (
            "x-mcp-management-internal-scope".to_string(),
            "runtime.invocations.read".to_string(),
        ),
    ]);

    let result = jsonrpc_http_tool_call_cancellable(
        format!("http://{address}/mcp").as_str(),
        Some(&headers),
        json!({"name": "demo", "arguments": {}}),
        Some(Duration::from_secs(5)),
    )
    .await
    .expect("final tool result");

    assert_eq!(
        result,
        json!({
            "content": [
                {
                    "type": "text",
                    "text": "done"
                }
            ]
        })
    );
    assert_eq!(state.poll_count.load(Ordering::SeqCst), 2);
    assert_eq!(
        state.poll_caller.lock().unwrap().as_deref(),
        Some("task-runner")
    );
    assert!(state
        .poll_token
        .lock()
        .unwrap()
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty()));
    assert!(!*state.saw_secret.lock().unwrap());
    server.abort();
}

#[test]
fn http_response_body_limit_accepts_boundary_size() {
    assert!(ensure_http_response_body_within_limit(1024, 1024).is_ok());
}

#[test]
fn http_response_body_limit_rejects_oversized_body() {
    let err =
        ensure_http_response_body_within_limit(1025, 1024).expect_err("oversized body should fail");

    assert!(err.contains("exceeded limit"));
    assert!(err.contains("1025 bytes > 1024 bytes"));
}

#[test]
fn stdio_response_line_limit_accepts_boundary_size() {
    assert!(ensure_stdio_response_line_within_limit(1024, 1024).is_ok());
}

#[test]
fn stdio_response_line_limit_rejects_oversized_line() {
    let err = ensure_stdio_response_line_within_limit(1025, 1024)
        .expect_err("oversized line should fail");

    assert!(err.contains("exceeded limit"));
    assert!(err.contains("1025 bytes > 1024 bytes"));
}

#[test]
fn http_tools_list_cache_key_sorts_headers() {
    let headers_a = HashMap::from([
        ("X-Zed".to_string(), "last".to_string()),
        ("X-Alpha".to_string(), "first".to_string()),
    ]);
    let headers_b = HashMap::from([
        ("X-Alpha".to_string(), "first".to_string()),
        ("X-Zed".to_string(), "last".to_string()),
    ]);

    let cache_key = tools_list_http_cache_key("https://example.test/mcp", Some(&headers_a), None);
    assert_eq!(
        cache_key,
        tools_list_http_cache_key("https://example.test/mcp", Some(&headers_b), None)
    );
    assert!(!cache_key.contains("first"));
    assert!(!cache_key.contains("last"));
}

#[test]
fn stdio_tools_list_cache_key_includes_config_shape() {
    let base = McpStdioServer {
        name: "demo".to_string(),
        command: "node".to_string(),
        args: Some(vec!["server.js".to_string()]),
        cwd: Some("/workspace".to_string()),
        env: Some(HashMap::from([("TOKEN".to_string(), "one".to_string())])),
        user_id: None,
    };
    let mut changed = base.clone();
    changed.args = Some(vec!["other.js".to_string()]);

    assert_ne!(
        tools_list_stdio_cache_key(&base),
        tools_list_stdio_cache_key(&changed)
    );
    assert!(!tools_list_stdio_cache_key(&base).contains("one"));
}

#[test]
fn stdio_tools_list_cache_key_includes_user_id() {
    let mut first = McpStdioServer {
        name: "demo".to_string(),
        command: "node".to_string(),
        args: Some(vec!["server.js".to_string()]),
        cwd: Some("/workspace".to_string()),
        env: None,
        user_id: Some("user-a".to_string()),
    };
    let mut second = first.clone();
    second.user_id = Some("user-b".to_string());

    assert_ne!(
        tools_list_stdio_cache_key(&first),
        tools_list_stdio_cache_key(&second)
    );

    first.user_id = Some("user-b".to_string());
    assert_eq!(
        tools_list_stdio_cache_key(&first),
        tools_list_stdio_cache_key(&second)
    );
}

#[test]
fn stdio_session_cache_key_includes_server_name() {
    let mut first = McpStdioServer {
        name: "alpha".to_string(),
        command: "node".to_string(),
        args: Some(vec!["server.js".to_string()]),
        cwd: Some("/workspace".to_string()),
        env: None,
        user_id: None,
    };
    let mut second = first.clone();
    second.name = "beta".to_string();

    assert_ne!(
        stdio_session_cache_key(&first),
        stdio_session_cache_key(&second)
    );

    first.name = "beta".to_string();
    assert_eq!(
        stdio_session_cache_key(&first),
        stdio_session_cache_key(&second)
    );
}

#[test]
fn explicit_stdio_user_session_identity_does_not_persist_rotating_env_secrets() {
    let first = McpStdioServer {
        name: "demo".to_string(),
        command: "node".to_string(),
        args: Some(vec!["server.js".to_string()]),
        cwd: Some("/workspace".to_string()),
        env: Some(HashMap::from([(
            "TOKEN".to_string(),
            "secret-one".to_string(),
        )])),
        user_id: Some("owner:device:session".to_string()),
    };
    let mut second = first.clone();
    second.env = Some(HashMap::from([(
        "TOKEN".to_string(),
        "secret-two".to_string(),
    )]));

    let session_key = stdio_session_cache_key(&first);
    assert_eq!(session_key, stdio_session_cache_key(&second));
    assert!(!session_key.contains("secret-one"));
    assert!(!session_key.contains("secret-two"));
    assert_ne!(
        tools_list_stdio_cache_key(&first),
        tools_list_stdio_cache_key(&second)
    );
}

#[test]
fn tools_list_cache_returns_fresh_entries_and_drops_expired_entries() {
    let key = format!("test-cache-key-{}", uuid::Uuid::new_v4());
    let result = Ok(vec![json!({"name": "demo_tool"})]);
    store_tools_list_cache(key.clone(), result.clone());
    assert_eq!(cached_tools_list(key.as_str()), Some(result));

    let cache = MCP_TOOLS_LIST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().expect("cache lock");
    guard.insert(
        key.clone(),
        ToolsListCacheEntry {
            expires_at: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .expect("expired instant"),
            result: Ok(vec![json!({"name": "expired_tool"})]),
        },
    );
    drop(guard);

    assert!(cached_tools_list(key.as_str()).is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn stdio_jsonrpc_reuses_session_for_same_config() {
    let count_file = std::env::temp_dir().join(format!(
        "chatos_mcp_stdio_session_count_{}",
        uuid::Uuid::new_v4()
    ));
    let script = r#"
count=$(cat "$COUNT_FILE" 2>/dev/null || echo 0)
echo $((count + 1)) > "$COUNT_FILE"
while IFS= read -r line; do
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  printf '{"jsonrpc":"2.0","id":"%s","result":{"ok":true}}\n' "$id"
done
"#;
    let cfg = McpStdioServer {
        name: format!("session-reuse-{}", uuid::Uuid::new_v4()),
        command: "sh".to_string(),
        args: Some(vec!["-c".to_string(), script.to_string()]),
        cwd: None,
        env: Some(HashMap::from([(
            "COUNT_FILE".to_string(),
            count_file.to_string_lossy().to_string(),
        )])),
        user_id: None,
    };

    let first = jsonrpc_stdio_call(&cfg, "demo/one", json!({}), None)
        .await
        .expect("first stdio response");
    let second = jsonrpc_stdio_call(&cfg, "demo/two", json!({}), None)
        .await
        .expect("second stdio response");
    assert_eq!(first.pointer("/ok"), Some(&Value::Bool(true)));
    assert_eq!(second.pointer("/ok"), Some(&Value::Bool(true)));
    assert_eq!(
        std::fs::read_to_string(&count_file)
            .expect("count file")
            .trim(),
        "1"
    );

    super::stdio::remove_stdio_session(stdio_session_cache_key(&cfg).as_str());
    let _ = std::fs::remove_file(count_file);
}

#[cfg(unix)]
#[tokio::test]
async fn stdio_jsonrpc_deduplicates_concurrent_cold_start() {
    let count_file = std::env::temp_dir().join(format!(
        "chatos_mcp_stdio_cold_start_count_{}",
        uuid::Uuid::new_v4()
    ));
    let script = r#"
count=$(cat "$COUNT_FILE" 2>/dev/null || echo 0)
echo $((count + 1)) > "$COUNT_FILE"
while IFS= read -r line; do
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  printf '{"jsonrpc":"2.0","id":"%s","result":{"ok":true}}\n' "$id"
done
"#;
    let cfg = McpStdioServer {
        name: format!("cold-start-{}", uuid::Uuid::new_v4()),
        command: "sh".to_string(),
        args: Some(vec!["-c".to_string(), script.to_string()]),
        cwd: None,
        env: Some(HashMap::from([(
            "COUNT_FILE".to_string(),
            count_file.to_string_lossy().to_string(),
        )])),
        user_id: None,
    };

    let mut handles = Vec::new();
    for index in 0..8 {
        let cfg = cfg.clone();
        handles.push(tokio::spawn(async move {
            jsonrpc_stdio_call(&cfg, "demo/concurrent", json!({ "index": index }), None).await
        }));
    }

    for handle in handles {
        let value = handle.await.expect("join stdio request").expect("response");
        assert_eq!(value.pointer("/ok"), Some(&Value::Bool(true)));
    }

    assert_eq!(
        std::fs::read_to_string(&count_file)
            .expect("count file")
            .trim(),
        "1"
    );

    super::stdio::remove_stdio_session(stdio_session_cache_key(&cfg).as_str());
    let _ = std::fs::remove_file(count_file);
}
