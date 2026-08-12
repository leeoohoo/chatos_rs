// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use super::{BrowserToolsOptions, BrowserToolsService};

static REAL_BROWSER_E2E_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn list_tools_contains_browser_navigate_and_vision() {
    let dir = std::env::temp_dir().join(format!("browser_tools_test_{}", Uuid::new_v4()));
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: PathBuf::from(&dir),
        route_interception_enabled: true,
        full_cdp_access_enabled: true,
        ..Default::default()
    })
    .expect("init browser tools");

    let names: Vec<String> = service
        .list_tools()
        .into_iter()
        .filter_map(|item| {
            item.get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    let unavailable = service.unavailable_tools();
    if unavailable.is_empty() {
        assert!(names.contains(&"browser_tabs".to_string()));
        assert!(names.contains(&"browser_tab_new".to_string()));
        assert!(names.contains(&"browser_tab_switch".to_string()));
        assert!(names.contains(&"browser_tab_close".to_string()));
        assert!(names.contains(&"browser_navigate".to_string()));
        assert!(names.contains(&"browser_set_viewport".to_string()));
        assert!(names.contains(&"browser_inspect".to_string()));
        assert!(names.contains(&"browser_research".to_string()));
        assert!(names.contains(&"browser_network".to_string()));
        assert!(names.contains(&"browser_network_request".to_string()));
        assert!(names.contains(&"browser_har_start".to_string()));
        assert!(names.contains(&"browser_har_stop".to_string()));
        assert!(names.contains(&"browser_websocket_start".to_string()));
        assert!(names.contains(&"browser_websocket_frames".to_string()));
        assert!(names.contains(&"browser_websocket_stop".to_string()));
        assert!(names.contains(&"browser_route_add".to_string()));
        assert!(names.contains(&"browser_route_list".to_string()));
        assert!(names.contains(&"browser_route_remove".to_string()));
        assert!(names.contains(&"browser_route_clear".to_string()));
        assert!(names.contains(&"browser_cdp_command".to_string()));
        assert!(names.contains(&"browser_upload".to_string()));
        assert!(names.contains(&"browser_download".to_string()));
        assert!(names.contains(&"browser_vision".to_string()));
    } else if unavailable.len() == 1
        && unavailable.first().map(|(name, _)| name.as_str()) == Some("browser_vision")
    {
        assert!(names.contains(&"browser_tabs".to_string()));
        assert!(names.contains(&"browser_tab_new".to_string()));
        assert!(names.contains(&"browser_tab_switch".to_string()));
        assert!(names.contains(&"browser_tab_close".to_string()));
        assert!(names.contains(&"browser_navigate".to_string()));
        assert!(names.contains(&"browser_set_viewport".to_string()));
        assert!(names.contains(&"browser_inspect".to_string()));
        assert!(names.contains(&"browser_research".to_string()));
        assert!(names.contains(&"browser_network".to_string()));
        assert!(names.contains(&"browser_network_request".to_string()));
        assert!(names.contains(&"browser_har_start".to_string()));
        assert!(names.contains(&"browser_har_stop".to_string()));
        assert!(names.contains(&"browser_websocket_start".to_string()));
        assert!(names.contains(&"browser_websocket_frames".to_string()));
        assert!(names.contains(&"browser_websocket_stop".to_string()));
        assert!(names.contains(&"browser_route_add".to_string()));
        assert!(names.contains(&"browser_route_list".to_string()));
        assert!(names.contains(&"browser_route_remove".to_string()));
        assert!(names.contains(&"browser_route_clear".to_string()));
        assert!(names.contains(&"browser_cdp_command".to_string()));
        assert!(names.contains(&"browser_upload".to_string()));
        assert!(names.contains(&"browser_download".to_string()));
        assert!(!names.contains(&"browser_vision".to_string()));
        assert!(unavailable
            .first()
            .map(|(_, reason)| reason.contains("vision model adapter"))
            .unwrap_or(false));
    } else {
        assert!(names.is_empty());
        assert_eq!(unavailable.len(), 31);
        assert!(unavailable
            .iter()
            .all(|(_, reason)| reason.contains("agent-browser")));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_unknown_tool_returns_error() {
    let dir = std::env::temp_dir().join(format!("browser_tools_test_{}", Uuid::new_v4()));
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: PathBuf::from(&dir),
        ..Default::default()
    })
    .expect("init browser tools");
    let err = service
        .call_tool("browser_not_exists", serde_json::json!({}), None)
        .expect_err("unknown tool should fail");
    assert!(err.contains("Tool not found"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn real_har_export_removes_probe_secret_when_explicitly_enabled() {
    if std::env::var("CHATOS_REAL_BROWSER_HAR_E2E").as_deref() != Ok("1") {
        return;
    }
    let binary = std::env::var("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN")
        .expect("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN is required for real HAR E2E");
    let _env_lock = REAL_BROWSER_E2E_ENV_LOCK
        .lock()
        .expect("lock browser E2E env");
    let previous_binary = std::env::var_os("AGENT_BROWSER_BIN");
    let previous_namespace = std::env::var_os("AGENT_BROWSER_NAMESPACE");
    std::env::set_var("AGENT_BROWSER_BIN", binary);
    std::env::set_var(
        "AGENT_BROWSER_NAMESPACE",
        format!("c{}", &Uuid::new_v4().simple().to_string()[..8]),
    );

    let workspace = std::env::temp_dir().join(format!(
        "chatos_browser_har_e2e_{}",
        Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(workspace.as_path()).expect("create HAR E2E workspace");
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: workspace.clone(),
        command_timeout_seconds: 60,
        ..BrowserToolsOptions::default()
    })
    .expect("create BrowserTools service");
    let conversation_id = format!("har-e2e-{}", Uuid::new_v4().simple());
    let secret = format!("probe-secret-{}", Uuid::new_v4().simple());

    let outcome = (|| -> Result<(), String> {
        ensure_browser_tool_success(service.call_tool(
            "browser_navigate",
            serde_json::json!({"url": "about:blank"}),
            Some(conversation_id.as_str()),
        )?)?;
        ensure_browser_tool_success(service.call_tool(
            "browser_har_start",
            serde_json::json!({}),
            Some(conversation_id.as_str()),
        )?)?;
        ensure_browser_tool_success(service.call_tool(
            "browser_navigate",
            serde_json::json!({
                "url": format!("https://example.com/?token={secret}&safe=value")
            }),
            Some(conversation_id.as_str()),
        )?)?;
        let stopped = ensure_browser_tool_success(service.call_tool(
            "browser_har_stop",
            serde_json::json!({"path": "network.har"}),
            Some(conversation_id.as_str()),
        )?)?;
        let response_text = serde_json::to_string(&stopped).map_err(|error| error.to_string())?;
        if response_text.contains(secret.as_str()) {
            return Err("browser_har_stop response leaked the probe secret".to_string());
        }
        let har_text = std::fs::read_to_string(workspace.join("network.har"))
            .map_err(|error| format!("read exported HAR failed: {error}"))?;
        if har_text.contains(secret.as_str()) {
            return Err("sanitized HAR leaked the probe secret".to_string());
        }
        if !har_text.contains("%5BREDACTED%5D") {
            return Err("sanitized HAR did not preserve redacted query shape".to_string());
        }
        Ok(())
    })();

    if let Some(session) = service
        .bound
        .sessions
        .lock()
        .get(conversation_id.as_str())
        .cloned()
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build browser close runtime");
        let _ = runtime.block_on(crate::browser_runtime::run_browser_command(
            workspace.as_path(),
            &session,
            "close",
            Vec::new(),
            30,
        ));
    }
    let _ = std::fs::remove_dir_all(workspace);
    match previous_binary {
        Some(value) => std::env::set_var("AGENT_BROWSER_BIN", value),
        None => std::env::remove_var("AGENT_BROWSER_BIN"),
    }
    match previous_namespace {
        Some(value) => std::env::set_var("AGENT_BROWSER_NAMESPACE", value),
        None => std::env::remove_var("AGENT_BROWSER_NAMESPACE"),
    }

    outcome.expect("real BrowserTools HAR E2E");
}

#[test]
fn real_websocket_observer_captures_bidirectional_redacted_frames_when_explicitly_enabled() {
    if std::env::var("CHATOS_REAL_BROWSER_WEBSOCKET_E2E").as_deref() != Ok("1") {
        return;
    }
    let binary = std::env::var("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN")
        .expect("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN is required for WebSocket E2E");
    let _env_lock = REAL_BROWSER_E2E_ENV_LOCK
        .lock()
        .expect("lock browser E2E env");
    let previous_binary = std::env::var_os("AGENT_BROWSER_BIN");
    let previous_namespace = std::env::var_os("AGENT_BROWSER_NAMESPACE");
    std::env::set_var("AGENT_BROWSER_BIN", binary);
    std::env::set_var(
        "AGENT_BROWSER_NAMESPACE",
        format!("c{}", &Uuid::new_v4().simple().to_string()[..8]),
    );

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("bind ephemeral WebSocket E2E listener");
    listener
        .set_nonblocking(true)
        .expect("set WebSocket E2E listener nonblocking");
    let port = listener
        .local_addr()
        .expect("read WebSocket E2E listener address")
        .port();
    let secret = format!("probe-secret-{}", Uuid::new_v4().simple());
    let page_marker = Uuid::new_v4().simple().to_string();
    let page_listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("bind ephemeral WebSocket E2E page listener");
    page_listener
        .set_nonblocking(true)
        .expect("set WebSocket E2E page listener nonblocking");
    let page_port = page_listener
        .local_addr()
        .expect("read WebSocket E2E page listener address")
        .port();
    let page_body =
        format!("<!doctype html><title>WebSocket {page_marker}</title><h1>{page_marker}</h1>");
    let page_server = std::thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(page_listener)
                .map_err(|error| error.to_string())?;
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(20), listener.accept())
                .await
                .map_err(|_| "WebSocket E2E page accept timed out".to_string())?
                .map_err(|error| error.to_string())?;
            let mut request = [0_u8; 4096];
            let _ = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut request))
                .await
                .map_err(|_| "WebSocket E2E page request timed out".to_string())?
                .map_err(|error| error.to_string())?;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                page_body.len(),
                page_body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .map_err(|error| error.to_string())?;
            stream.shutdown().await.map_err(|error| error.to_string())?;
            Ok(())
        })
    });
    let server_secret = secret.clone();
    let server = std::thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(async move {
            let listener =
                tokio::net::TcpListener::from_std(listener).map_err(|error| error.to_string())?;
            let (stream, _) = tokio::time::timeout(Duration::from_secs(20), listener.accept())
                .await
                .map_err(|_| "WebSocket E2E server accept timed out".to_string())?
                .map_err(|error| error.to_string())?;
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .map_err(|error| error.to_string())?;
            let message = tokio::time::timeout(Duration::from_secs(10), websocket.next())
                .await
                .map_err(|_| "WebSocket E2E server receive timed out".to_string())?
                .ok_or_else(|| "WebSocket E2E client closed before sending".to_string())?
                .map_err(|error| error.to_string())?;
            if !message.is_text() {
                return Err("WebSocket E2E client did not send a text frame".to_string());
            }
            websocket
                .send(Message::Text(
                    format!("{{\"event\":\"echo\",\"password\":\"{server_secret}\"}}").into(),
                ))
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    });

    let workspace = std::env::temp_dir().join(format!(
        "chatos_browser_websocket_e2e_{}",
        Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(workspace.as_path()).expect("create WebSocket E2E workspace");
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: workspace.clone(),
        command_timeout_seconds: 60,
        ..BrowserToolsOptions::default()
    })
    .expect("create BrowserTools service");
    let conversation_id = format!("websocket-e2e-{}", Uuid::new_v4().simple());

    let outcome = (|| -> Result<(), String> {
        ensure_browser_tool_success(service.call_tool(
            "browser_navigate",
            serde_json::json!({
                "url": format!("http://127.0.0.1:{page_port}/?marker={page_marker}")
            }),
            Some(conversation_id.as_str()),
        )?)?;
        ensure_browser_tool_success(service.call_tool(
            "browser_websocket_start",
            serde_json::json!({}),
            Some(conversation_id.as_str()),
        )?)?;
        let websocket_url = format!("ws://127.0.0.1:{port}/socket?token={secret}");
        let payload = format!("{{\"event\":\"probe\",\"token\":\"{secret}\"}}");
        let expression = format!(
            "(() => {{ const ws = new WebSocket({}); ws.addEventListener('open', () => ws.send({})); window.__chatosWs = ws; return 'started'; }})()",
            serde_json::to_string(&websocket_url).map_err(|error| error.to_string())?,
            serde_json::to_string(&payload).map_err(|error| error.to_string())?,
        );
        ensure_browser_tool_success(service.call_tool(
            "browser_console",
            serde_json::json!({"expression": expression}),
            Some(conversation_id.as_str()),
        )?)?;

        let mut captured = None;
        let mut last_response = String::new();
        for _ in 0..50 {
            let frames = ensure_browser_tool_success(service.call_tool(
                "browser_websocket_frames",
                serde_json::json!({
                    "limit": 20,
                    "include_text_payloads": true,
                    "max_payload_chars": 4096
                }),
                Some(conversation_id.as_str()),
            )?)?;
            let text = serde_json::to_string(&frames).map_err(|error| error.to_string())?;
            if text.contains(secret.as_str()) {
                return Err("browser_websocket_frames leaked the probe secret".to_string());
            }
            last_response = text.chars().take(4_000).collect();
            let frame_count = frames
                .get("returned_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            if frame_count >= 2
                && text.contains("\"direction\":\"sent\"")
                && text.contains("\"direction\":\"received\"")
                && text.contains("[REDACTED]")
            {
                captured = Some(frames);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if captured.is_none() {
            return Err(format!(
                "browser_websocket_frames did not capture bidirectional redacted text frames; last sanitized response: {last_response}"
            ));
        }
        ensure_browser_tool_success(service.call_tool(
            "browser_websocket_stop",
            serde_json::json!({}),
            Some(conversation_id.as_str()),
        )?)?;
        Ok(())
    })();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build browser close runtime");
    let _ = runtime.block_on(service.close_attached_managed_session(conversation_id.as_str()));
    let server_outcome = server
        .join()
        .map_err(|_| "WebSocket E2E server thread panicked".to_string())
        .and_then(|result| result);
    let page_server_outcome = page_server
        .join()
        .map_err(|_| "WebSocket E2E page server thread panicked".to_string())
        .and_then(|result| result);
    let _ = std::fs::remove_dir_all(workspace);
    match previous_binary {
        Some(value) => std::env::set_var("AGENT_BROWSER_BIN", value),
        None => std::env::remove_var("AGENT_BROWSER_BIN"),
    }
    match previous_namespace {
        Some(value) => std::env::set_var("AGENT_BROWSER_NAMESPACE", value),
        None => std::env::remove_var("AGENT_BROWSER_NAMESPACE"),
    }

    outcome.expect("real BrowserTools WebSocket E2E");
    server_outcome.expect("real WebSocket E2E server");
    page_server_outcome.expect("real WebSocket E2E page server");
}

#[test]
fn real_managed_preview_frame_is_bounded_when_explicitly_enabled() {
    if std::env::var("CHATOS_REAL_BROWSER_PREVIEW_E2E").as_deref() != Ok("1") {
        return;
    }
    let binary = std::env::var("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN")
        .expect("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN is required for preview E2E");
    let _env_lock = REAL_BROWSER_E2E_ENV_LOCK
        .lock()
        .expect("lock browser E2E env");
    let previous_binary = std::env::var_os("AGENT_BROWSER_BIN");
    let previous_namespace = std::env::var_os("AGENT_BROWSER_NAMESPACE");
    std::env::set_var("AGENT_BROWSER_BIN", binary);
    std::env::set_var(
        "AGENT_BROWSER_NAMESPACE",
        format!("c{}", &Uuid::new_v4().simple().to_string()[..8]),
    );

    let workspace = std::env::temp_dir().join(format!(
        "chatos_browser_preview_e2e_{}",
        Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(workspace.as_path()).expect("create preview E2E workspace");
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: workspace.clone(),
        command_timeout_seconds: 60,
        ..BrowserToolsOptions::default()
    })
    .expect("create BrowserTools service");
    let conversation_id = format!("preview-e2e-{}", Uuid::new_v4().simple());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build browser preview runtime");

    let outcome = (|| -> Result<(), String> {
        ensure_browser_tool_success(service.call_tool(
            "browser_navigate",
            serde_json::json!({
                "url": "data:text/html,<html><body><h1>ChatOS preview E2E</h1></body></html>"
            }),
            Some(conversation_id.as_str()),
        )?)?;
        let frame = runtime.block_on(
            service.capture_attached_managed_session_preview_frame(conversation_id.as_str()),
        )?;
        if frame.media_type != "image/jpeg" || frame.source != "screencast" {
            return Err(format!(
                "managed preview did not use the continuous JPEG screencast path: {}/{}",
                frame.media_type, frame.source
            ));
        }
        if frame.sequence == 0 {
            return Err("managed screencast frame did not include a sequence".to_string());
        }
        if frame.bytes.is_empty() || frame.bytes.len() > 5 * 1024 * 1024 {
            return Err("managed preview frame violated the byte limit".to_string());
        }
        if frame.width == 0 || frame.height == 0 || frame.width > 8192 || frame.height > 8192 {
            return Err("managed preview frame violated the dimension limit".to_string());
        }
        Ok(())
    })();

    let _ = runtime.block_on(service.close_attached_managed_session(conversation_id.as_str()));
    let _ = std::fs::remove_dir_all(workspace);
    match previous_binary {
        Some(value) => std::env::set_var("AGENT_BROWSER_BIN", value),
        None => std::env::remove_var("AGENT_BROWSER_BIN"),
    }
    match previous_namespace {
        Some(value) => std::env::set_var("AGENT_BROWSER_NAMESPACE", value),
        None => std::env::remove_var("AGENT_BROWSER_NAMESPACE"),
    }

    outcome.expect("real BrowserTools preview E2E");
}

#[test]
fn real_tab_lifecycle_uses_stable_ids_and_preserves_last_tab_when_explicitly_enabled() {
    if std::env::var("CHATOS_REAL_BROWSER_TABS_E2E").as_deref() != Ok("1") {
        return;
    }
    let binary = std::env::var("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN")
        .expect("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN is required for tabs E2E");
    let _env_lock = REAL_BROWSER_E2E_ENV_LOCK
        .lock()
        .expect("lock browser E2E env");
    let previous_binary = std::env::var_os("AGENT_BROWSER_BIN");
    let previous_namespace = std::env::var_os("AGENT_BROWSER_NAMESPACE");
    std::env::set_var("AGENT_BROWSER_BIN", binary);
    std::env::set_var(
        "AGENT_BROWSER_NAMESPACE",
        format!("c{}", &Uuid::new_v4().simple().to_string()[..8]),
    );

    let workspace = std::env::temp_dir().join(format!(
        "chatos_browser_tabs_e2e_{}",
        Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(workspace.as_path()).expect("create tabs E2E workspace");
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: workspace.clone(),
        command_timeout_seconds: 60,
        ..BrowserToolsOptions::default()
    })
    .expect("create BrowserTools service");
    let conversation_id = format!("tabs-e2e-{}", Uuid::new_v4().simple());
    let secret = format!("probe-secret-{}", Uuid::new_v4().simple());

    let outcome = (|| -> Result<(), String> {
        ensure_browser_tool_success(service.call_tool(
            "browser_navigate",
            serde_json::json!({
                "url": format!("data:text/html,<title>Alpha</title><h1>Alpha</h1><!--{secret}-->")
            }),
            Some(conversation_id.as_str()),
        )?)?;
        let viewport = ensure_browser_tool_success(service.call_tool(
            "browser_set_viewport",
            serde_json::json!({"width": 480, "height": 720}),
            Some(conversation_id.as_str()),
        )?)?;
        if viewport
            .get("actual_width")
            .and_then(serde_json::Value::as_u64)
            != Some(480)
            || viewport
                .get("actual_height")
                .and_then(serde_json::Value::as_u64)
                != Some(720)
        {
            return Err("browser_set_viewport did not verify the requested viewport".to_string());
        }
        let opened = ensure_browser_tool_success(service.call_tool(
            "browser_tab_new",
            serde_json::json!({
                "url": "data:text/html,<title>Beta</title><h1>Beta</h1>"
            }),
            Some(conversation_id.as_str()),
        )?)?;
        let opened_text = serde_json::to_string(&opened).map_err(|error| error.to_string())?;
        if opened_text.contains(secret.as_str()) || opened_text.contains("data:text/html") {
            return Err("browser tab response leaked a non-web URL payload".to_string());
        }
        let tabs = opened
            .get("tabs")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "browser_tab_new did not return tabs".to_string())?;
        if tabs.len() != 2 {
            return Err(format!("expected 2 browser tabs, got {}", tabs.len()));
        }
        let first_tab = tabs[0]
            .get("tab_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "first browser tab has no stable id".to_string())?;
        let second_tab = tabs[1]
            .get("tab_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "second browser tab has no stable id".to_string())?;
        if first_tab == second_tab || !first_tab.starts_with('t') || !second_tab.starts_with('t') {
            return Err("browser tabs did not return distinct stable ids".to_string());
        }

        let switched = ensure_browser_tool_success(service.call_tool(
            "browser_tab_switch",
            serde_json::json!({"tab_id": first_tab}),
            Some(conversation_id.as_str()),
        )?)?;
        if switched
            .get("active_tab_id")
            .and_then(serde_json::Value::as_str)
            != Some(first_tab)
        {
            return Err("browser_tab_switch did not activate the requested stable id".to_string());
        }
        let closed = ensure_browser_tool_success(service.call_tool(
            "browser_tab_close",
            serde_json::json!({"tab_id": second_tab}),
            Some(conversation_id.as_str()),
        )?)?;
        if closed.get("tab_count").and_then(serde_json::Value::as_u64) != Some(1) {
            return Err("browser_tab_close did not leave exactly one tab".to_string());
        }

        let last_close = service.call_tool(
            "browser_tab_close",
            serde_json::json!({"tab_id": first_tab}),
            Some(conversation_id.as_str()),
        )?;
        let last_close = last_close
            .get("_structured_result")
            .cloned()
            .unwrap_or(last_close);
        if last_close
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
            || !last_close
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .contains("last browser tab")
        {
            return Err("browser_tab_close did not preserve the last tab".to_string());
        }
        Ok(())
    })();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build browser close runtime");
    let _ = runtime.block_on(service.close_attached_managed_session(conversation_id.as_str()));
    let _ = std::fs::remove_dir_all(workspace);
    match previous_binary {
        Some(value) => std::env::set_var("AGENT_BROWSER_BIN", value),
        None => std::env::remove_var("AGENT_BROWSER_BIN"),
    }
    match previous_namespace {
        Some(value) => std::env::set_var("AGENT_BROWSER_NAMESPACE", value),
        None => std::env::remove_var("AGENT_BROWSER_NAMESPACE"),
    }

    outcome.expect("real BrowserTools tabs E2E");
}

#[test]
fn real_route_and_full_cdp_commands_are_session_scoped_when_explicitly_enabled() {
    if std::env::var("CHATOS_REAL_BROWSER_PRIVILEGED_E2E").as_deref() != Ok("1") {
        return;
    }
    let binary = std::env::var("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN")
        .expect("CHATOS_BROWSER_E2E_AGENT_BROWSER_BIN is required for privileged Browser E2E");
    let _env_lock = REAL_BROWSER_E2E_ENV_LOCK
        .lock()
        .expect("lock browser E2E env");
    let previous_binary = std::env::var_os("AGENT_BROWSER_BIN");
    let previous_namespace = std::env::var_os("AGENT_BROWSER_NAMESPACE");
    std::env::set_var("AGENT_BROWSER_BIN", binary);
    std::env::set_var(
        "AGENT_BROWSER_NAMESPACE",
        format!("c{}", &Uuid::new_v4().simple().to_string()[..8]),
    );

    let workspace = std::env::temp_dir().join(format!(
        "chatos_browser_privileged_e2e_{}",
        Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(workspace.as_path()).expect("create privileged Browser E2E workspace");
    let service = BrowserToolsService::new(BrowserToolsOptions {
        workspace_dir: workspace.clone(),
        command_timeout_seconds: 60,
        route_interception_enabled: true,
        full_cdp_access_enabled: true,
        ..BrowserToolsOptions::default()
    })
    .expect("create privileged BrowserTools service");
    let conversation_id = format!("privileged-e2e-{}", Uuid::new_v4().simple());

    let outcome = (|| -> Result<(), String> {
        ensure_browser_tool_success(service.call_tool(
            "browser_navigate",
            serde_json::json!({"url": "about:blank"}),
            Some(conversation_id.as_str()),
        )?)?;
        let added = ensure_browser_tool_success(service.call_tool(
            "browser_route_add",
            serde_json::json!({
                "pattern": "https://example.invalid/api/**",
                "action": "mock_json",
                "body": {"ok": true}
            }),
            Some(conversation_id.as_str()),
        )?)?;
        let route_id = added
            .pointer("/route/route_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "browser_route_add did not return a route_id".to_string())?
            .to_string();
        let added_text = serde_json::to_string(&added).map_err(|error| error.to_string())?;
        if added_text.contains("\"body\":{\"ok\":true}") {
            return Err("browser_route_add response leaked the mock body".to_string());
        }
        let listed = ensure_browser_tool_success(service.call_tool(
            "browser_route_list",
            serde_json::json!({}),
            Some(conversation_id.as_str()),
        )?)?;
        if listed
            .get("route_count")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
            || listed
                .pointer("/routes/0/route_id")
                .and_then(serde_json::Value::as_str)
                != Some(route_id.as_str())
        {
            return Err("browser_route_list did not preserve the session-owned rule".to_string());
        }

        let cdp = ensure_browser_tool_success(service.call_tool(
            "browser_cdp_command",
            serde_json::json!({
                "target": "page",
                "method": "Runtime.evaluate",
                "params": {"expression": "1 + 1", "returnByValue": true}
            }),
            Some(conversation_id.as_str()),
        )?)?;
        if cdp
            .pointer("/result/result/value")
            .and_then(serde_json::Value::as_i64)
            != Some(2)
        {
            return Err("browser_cdp_command did not return the active page result".to_string());
        }
        let cdp_text = serde_json::to_string(&cdp).map_err(|error| error.to_string())?;
        if cdp_text.contains("cdpUrl") || cdp_text.contains("devtools/browser") {
            return Err("browser_cdp_command exposed the debugger endpoint".to_string());
        }

        ensure_browser_tool_success(service.call_tool(
            "browser_route_remove",
            serde_json::json!({"route_id": route_id}),
            Some(conversation_id.as_str()),
        )?)?;
        let empty = ensure_browser_tool_success(service.call_tool(
            "browser_route_list",
            serde_json::json!({}),
            Some(conversation_id.as_str()),
        )?)?;
        if empty.get("route_count").and_then(serde_json::Value::as_u64) != Some(0) {
            return Err("browser_route_remove did not clear the session-owned rule".to_string());
        }
        Ok(())
    })();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build browser close runtime");
    let _ = runtime.block_on(service.close_attached_managed_session(conversation_id.as_str()));
    let _ = std::fs::remove_dir_all(workspace);
    match previous_binary {
        Some(value) => std::env::set_var("AGENT_BROWSER_BIN", value),
        None => std::env::remove_var("AGENT_BROWSER_BIN"),
    }
    match previous_namespace {
        Some(value) => std::env::set_var("AGENT_BROWSER_NAMESPACE", value),
        None => std::env::remove_var("AGENT_BROWSER_NAMESPACE"),
    }

    outcome.expect("real privileged BrowserTools E2E");
}

fn ensure_browser_tool_success(result: serde_json::Value) -> Result<serde_json::Value, String> {
    let structured = result.get("_structured_result").cloned().unwrap_or(result);
    if structured
        .get("success")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        Ok(structured)
    } else {
        Err(format!("browser tool failed: {structured}"))
    }
}
