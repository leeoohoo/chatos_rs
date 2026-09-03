use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tungstenite::{Message, accept};

#[test]
#[ignore = "requires an installed Chrome/Chromium browser"]
fn managed_browser_internal_fallback_end_to_end() {
    let (url, stop_server) = serve_fixture();
    let websocket_url = serve_websocket_fixture(stop_server.clone());
    let mut mcp = McpProcess::spawn();
    mcp.request("initialize", json!({}));

    let opened = mcp.call_tool(
        "browser_session_open",
        json!({"mode":"managed","headless":true}),
    );
    assert!(opened["structuredContent"]["browser_session_id"].is_null());

    let console = mcp.call_tool("browser_console", json!({"action":"start"}));
    let console_subscription = console["structuredContent"]["subscription_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let network = mcp.call_tool("browser_network", json!({"action":"start"}));
    let network_subscription = network["structuredContent"]["subscription_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let har = mcp.call_tool("browser_har_start", json!({}));
    let har_subscription = har["structuredContent"]["subscription_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let websocket = mcp.call_tool("browser_websocket_start", json!({}));
    let websocket_subscription = websocket["structuredContent"]["subscription_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mock_route = mcp.call_tool(
        "browser_route_add",
        json!({

            "url_pattern":"*/api/data",
            "action":{"type":"mock_json","status":200,"body":{"source":"mock"}}
        }),
    );
    let mock_route_id = mock_route["structuredContent"]["route_id"]
        .as_str()
        .unwrap()
        .to_owned();

    mcp.call_tool("browser_navigate", json!({"url":url,"timeout_ms":15000}));
    let snapshot = mcp.call_tool("browser_snapshot", json!({}));
    assert!(
        snapshot["structuredContent"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["name"] == "Run test")
    );

    let found = mcp.call_tool("browser_find", json!({"query":"Run test"}));
    let reference = found["structuredContent"][0]["reference"].as_str().unwrap();
    mcp.call_tool("browser_click", json!({"ref":reference}));
    mcp.call_tool("browser_wait", json!({"text":"clicked","timeout_ms":5000}));
    mcp.call_tool("browser_wait", json!({"text":"mock","timeout_ms":5000}));

    let upload = mcp.call_tool("browser_find", json!({"query":"Upload fixture"}));
    let upload_ref = upload["structuredContent"][0]["reference"]
        .as_str()
        .unwrap();
    mcp.call_tool(
        "browser_upload",
        json!({

            "ref":upload_ref,
            "file_grant_ids":["grant_e2e"]
        }),
    );
    let uploaded_name = mcp.call_tool(
        "browser_cdp_send",
        json!({

            "method":"Runtime.evaluate",
            "params":{
                "expression":"document.querySelector('#file').files[0].name",
                "returnByValue":true
            }
        }),
    );
    assert_eq!(
        uploaded_name["structuredContent"]["result"]["value"],
        "selected.txt"
    );
    let reused_grant = mcp.call_tool_result(
        "browser_upload",
        json!({

            "ref":upload_ref,
            "file_grant_ids":["grant_e2e"]
        }),
    );
    assert_eq!(reused_grant["isError"], true);
    for rejected_grant in ["grant_bad", "../escape"] {
        let rejected = mcp.call_tool_result(
            "browser_upload",
            json!({

                "ref":upload_ref,
                "file_grant_ids":[rejected_grant]
            }),
        );
        assert_eq!(rejected["isError"], true);
    }

    let dialog_subscription = mcp.call_tool(
        "browser_cdp_subscribe",
        json!({

            "methods":["Page.javascriptDialogOpening","Page.javascriptDialogClosed"]
        }),
    );
    let dialog_subscription_id = dialog_subscription["structuredContent"]["subscription_id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.call_tool(
        "browser_cdp_send",
        json!({

            "method":"Runtime.evaluate",
            "params":{
                "expression":"setTimeout(() => { window.dialogResult = prompt('Enter value'); document.querySelector('#dialog-result').textContent = window.dialogResult; }, 0); true",
                "returnByValue":true
            }
        }),
    );
    thread::sleep(Duration::from_millis(100));
    mcp.call_tool(
        "browser_handle_dialog",
        json!({"accept":true,"prompt_text":"accepted"}),
    );
    mcp.call_tool("browser_wait", json!({"text":"accepted","timeout_ms":5000}));
    let dialog_events = mcp.call_tool(
        "browser_cdp_events",
        json!({

            "subscription_id":dialog_subscription_id,
            "wait_ms":2000
        }),
    );
    let dialog_json = serde_json::to_string(&dialog_events["structuredContent"]).unwrap();
    assert!(dialog_json.contains("Page.javascriptDialogOpening"));
    assert!(dialog_json.contains("Page.javascriptDialogClosed"));
    mcp.call_tool(
        "browser_cdp_unsubscribe",
        json!({"subscription_id":dialog_subscription_id}),
    );

    let downloads = mcp.call_tool("browser_downloads", json!({"action":"start"}));
    let download_subscription = downloads["structuredContent"]["subscription_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let download_link = mcp.call_tool("browser_find", json!({"query":"Download fixture"}));
    let download_ref = download_link["structuredContent"][0]["reference"]
        .as_str()
        .unwrap();
    mcp.call_tool("browser_click", json!({"ref":download_ref}));
    let mut after_sequence = 0;
    let mut attempts = 0;
    let download_artifact = loop {
        attempts += 1;
        assert!(attempts <= 10, "download did not complete");
        let collected = mcp.call_tool(
            "browser_downloads",
            json!({

                "action":"collect",
                "subscription_id":download_subscription,
                "after_sequence":after_sequence,
                "wait_ms":1000
            }),
        );
        after_sequence = collected["structuredContent"]["events"]["latest_sequence"]
            .as_u64()
            .unwrap_or(after_sequence);
        if let Some(artifact) = collected["structuredContent"]["artifacts"]
            .as_array()
            .and_then(|artifacts| artifacts.first())
        {
            break artifact.clone();
        }
    };
    let download_name = download_artifact["relative_path"].as_str().unwrap();
    assert_eq!(
        std::fs::read_to_string(mcp.artifact_path(download_name)).unwrap(),
        "download fixture"
    );
    mcp.call_tool(
        "browser_downloads",
        json!({"action":"stop","subscription_id":download_subscription}),
    );

    let console_events = mcp.call_tool(
        "browser_console",
        json!({

            "action":"events",
            "subscription_id":console_subscription,
            "wait_ms":2000
        }),
    );
    assert!(
        serde_json::to_string(&console_events["structuredContent"])
            .unwrap()
            .contains("fixture-ready")
    );
    let network_events = mcp.call_tool(
        "browser_network",
        json!({

            "action":"events",
            "subscription_id":network_subscription,
            "wait_ms":2000
        }),
    );
    let network_json = serde_json::to_string(&network_events["structuredContent"]).unwrap();
    assert!(network_json.contains("/api/data"));
    assert!(!network_json.contains("top-secret"));
    assert!(network_json.contains("[REDACTED]"));

    let websocket_expression = format!(
        "new Promise((resolve, reject) => {{ const ws = new WebSocket({}); ws.onopen = () => ws.send('ping'); ws.onmessage = event => {{ ws.close(); resolve(event.data); }}; ws.onerror = () => reject(new Error('websocket failed')); }})",
        serde_json::to_string(&websocket_url).unwrap()
    );
    let websocket_result = mcp.call_tool(
        "browser_cdp_send",
        json!({

            "method":"Runtime.evaluate",
            "params":{
                "expression":websocket_expression,
                "awaitPromise":true,
                "returnByValue":true
            }
        }),
    );
    assert_eq!(
        websocket_result["structuredContent"]["result"]["value"],
        "ping"
    );
    let websocket_events = mcp.call_tool(
        "browser_websocket_events",
        json!({

            "subscription_id":websocket_subscription,
            "wait_ms":2000
        }),
    );
    let websocket_json = serde_json::to_string(&websocket_events["structuredContent"]).unwrap();
    assert!(websocket_json.contains("Network.webSocketCreated"));
    assert!(websocket_json.contains("Network.webSocketFrameSent"));
    assert!(websocket_json.contains("Network.webSocketFrameReceived"));
    mcp.call_tool(
        "browser_websocket_stop",
        json!({"subscription_id":websocket_subscription}),
    );
    let har = mcp.call_tool(
        "browser_har_stop",
        json!({"subscription_id":har_subscription}),
    );
    let har_name = har["structuredContent"]["relative_path"].as_str().unwrap();
    let har_json = std::fs::read_to_string(mcp.artifact_path(har_name)).unwrap();
    assert!(har_json.contains("/api/data"));
    assert!(!har_json.contains("top-secret"));
    assert!(har_json.contains("[REDACTED]"));

    let raw_subscription = mcp.call_tool(
        "browser_cdp_subscribe",
        json!({

            "methods":["Runtime.consoleAPICalled"]
        }),
    );
    let raw_subscription_id = raw_subscription["structuredContent"]["subscription_id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.call_tool(
        "browser_cdp_send",
        json!({

            "method":"Runtime.evaluate",
            "params":{"expression":"console.log('raw-event')"}
        }),
    );
    let raw_events = mcp.call_tool(
        "browser_cdp_events",
        json!({

            "subscription_id":raw_subscription_id,
            "wait_ms":2000
        }),
    );
    assert!(
        serde_json::to_string(&raw_events["structuredContent"])
            .unwrap()
            .contains("raw-event")
    );
    mcp.call_tool(
        "browser_cdp_unsubscribe",
        json!({"subscription_id":raw_subscription_id}),
    );

    let abort_route = mcp.call_tool(
        "browser_route_add",
        json!({

            "url_pattern":"*/blocked",
            "action":{"type":"abort"}
        }),
    );
    let abort_route_id = abort_route["structuredContent"]["route_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let aborted = mcp.call_tool(
        "browser_cdp_send",
        json!({

            "method":"Runtime.evaluate",
            "params":{
                "expression":"fetch('/blocked').then(() => false).catch(() => true)",
                "awaitPromise":true,
                "returnByValue":true
            }
        }),
    );
    assert_eq!(aborted["structuredContent"]["result"]["value"], true);
    let routes = mcp.call_tool("browser_route_list", json!({}));
    assert_eq!(routes["structuredContent"].as_array().unwrap().len(), 2);
    for route_id in [mock_route_id, abort_route_id] {
        mcp.call_tool("browser_route_remove", json!({"route_id":route_id}));
    }
    let cdp = mcp.call_tool(
        "browser_cdp_send",
        json!({

            "method":"Runtime.evaluate",
            "params":{"expression":"document.title","returnByValue":true}
        }),
    );
    assert_eq!(
        cdp["structuredContent"]["result"]["value"],
        "Browser MCP fixture"
    );

    let screenshot = mcp.call_tool("browser_screenshot", json!({}));
    assert_eq!(screenshot["structuredContent"]["media_type"], "image/png");
    assert!(
        screenshot["structuredContent"]["size_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );

    mcp.call_tool("browser_session_close", json!({}));
    mcp.shutdown();
    stop_server.store(true, Ordering::Relaxed);
}

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    artifact_dir: std::path::PathBuf,
    grant_dir: std::path::PathBuf,
}

impl McpProcess {
    fn spawn() -> Self {
        let artifact_dir = std::env::temp_dir().join(format!(
            "browser-cdp-e2e-artifacts-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_dir_all(&artifact_dir);
        std::fs::create_dir_all(&artifact_dir).unwrap();
        let grant_dir = std::env::temp_dir().join(format!(
            "browser-cdp-e2e-grants-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_dir_all(&grant_dir);
        std::fs::create_dir_all(&grant_dir).unwrap();
        let selected_file = grant_dir.join("selected.txt");
        std::fs::write(&selected_file, b"upload fixture").unwrap();
        let expires_at_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 60_000;
        std::fs::write(
            grant_dir.join("grant_e2e.json"),
            serde_json::to_vec(&json!({
                "path": selected_file.canonicalize().unwrap(),
                "expires_at_unix_ms": expires_at_unix_ms,
                "size": 14,
                "sha256": format!("{:x}", Sha256::digest(b"upload fixture"))
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            grant_dir.join("grant_bad.json"),
            serde_json::to_vec(&json!({
                "path": selected_file.canonicalize().unwrap(),
                "expires_at_unix_ms": expires_at_unix_ms,
                "size": 14,
                "sha256": "00"
            }))
            .unwrap(),
        )
        .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_chatos-browser-cdp"))
            .arg("mcp")
            .env("CHATOS_PLUGIN_ARTIFACT_DIR", &artifact_dir)
            .env("CHATOS_PLUGIN_FILE_GRANT_DIR", &grant_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        Self {
            stdin: child.stdin.take().unwrap(),
            stdout: BufReader::new(child.stdout.take().unwrap()),
            child,
            next_id: 1,
            artifact_dir,
            grant_dir,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert!(
            response.get("error").is_none(),
            "JSON-RPC error: {response}"
        );
        response["result"].clone()
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call_tool_result(name, arguments);
        assert_eq!(result["isError"], false, "tool failed: {result}");
        result
    }

    fn call_tool_result(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name":name,"arguments":arguments}))
    }

    fn artifact_path(&self, name: &str) -> std::path::PathBuf {
        self.artifact_dir.join(name)
    }

    fn shutdown(mut self) {
        self.request("shutdown", json!({}));
        drop(self.stdin);
        assert!(self.child.wait().unwrap().success());
        let _ = std::fs::remove_dir_all(&self.artifact_dir);
        let _ = std::fs::remove_dir_all(&self.grant_dir);
    }
}

fn serve_fixture() -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    thread::spawn(move || {
        let body = r#"<!doctype html><html><head><title>Browser MCP fixture</title></head>
<body><button id="run">Run test</button><input id="file" type="file" aria-label="Upload fixture"><a href="/download.txt" download="fixture.txt">Download fixture</a><div id="result"></div><div id="api"></div><div id="dialog-result"></div>
<script>
console.log('fixture-ready');
document.querySelector('#run').addEventListener('click', () => { document.querySelector('#result').textContent = 'clicked'; });
fetch('/api/data', {headers: {Authorization: 'Bearer top-secret'}}).then(r => r.json()).then(data => { document.querySelector('#api').textContent = data.source; });
</script>
</body></html>"#;
        while !stop_for_thread.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = [0_u8; 4096];
                    let count = stream.read(&mut request).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..count]);
                    let is_download = request.starts_with("GET /download.txt ");
                    let (content_type, extra_headers, response_body) = if is_download {
                        (
                            "text/plain; charset=utf-8",
                            "Content-Disposition: attachment; filename=\"fixture.txt\"\r\n",
                            "download fixture",
                        )
                    } else {
                        ("text/html; charset=utf-8", "", body)
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    (format!("http://{address}/"), stop)
}

fn serve_websocket_fixture(stop: Arc<AtomicBool>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    if let Ok(mut websocket) = accept(stream) {
                        if let Ok(message) = websocket.read()
                            && matches!(message, Message::Text(_) | Message::Binary(_))
                        {
                            let _ = websocket.send(message);
                        }
                        let _ = websocket.close(None);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    format!("ws://{address}/")
}
