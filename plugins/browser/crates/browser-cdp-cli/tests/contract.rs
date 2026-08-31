use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
};

use serde_json::{Value, json};
use tungstenite::{
    Message, accept_hdr,
    handshake::server::{Request, Response},
    http::{HeaderValue, header::SEC_WEBSOCKET_PROTOCOL},
};

const BRIDGE_SUBPROTOCOL: &str = "chatos-browser-bridge.v1";
const BRIDGE_TOKEN: &str = "contract-bridge-token-0123456789";

#[test]
fn initialize_and_tools_list_use_json_lines_only() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_chatos-browser-cdp"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn MCP server");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    let initialized = receive(&mut stdout);
    assert_eq!(initialized["jsonrpc"], "2.0");
    assert_eq!(initialized["id"], 1);
    assert_eq!(
        initialized["result"]["serverInfo"]["name"],
        "chatos-browser-cdp"
    );

    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    );
    let tools = receive(&mut stdout);
    let catalog = tools["result"]["tools"].as_array().unwrap();
    assert!(
        catalog
            .iter()
            .any(|tool| tool["name"] == "browser_snapshot")
    );
    assert!(
        catalog
            .iter()
            .any(|tool| tool["name"] == "browser_cdp_send")
    );

    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
    );
    let shutdown = receive(&mut stdout);
    assert_eq!(shutdown["id"], 3);
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn tools_list_works_before_initialize_for_host_migration() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_chatos-browser-cdp"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn MCP server");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":"list","method":"tools/list","params":{}}),
    );
    let response = receive(&mut stdout);
    assert_eq!(response["id"], "list");
    assert!(response["result"]["tools"].as_array().unwrap().len() >= 20);
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn chrome_extension_mode_uses_the_authenticated_local_bridge() {
    let (endpoint, bridge) = serve_bridge();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chatos-browser-cdp"))
        .arg("mcp")
        .env("CHATOS_BROWSER_BRIDGE_ENDPOINT", endpoint)
        .env("CHATOS_BROWSER_BRIDGE_TOKEN", BRIDGE_TOKEN)
        .env_remove("CHATOS_BROWSER_BRIDGE_CREDENTIAL_FILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn MCP server");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{
                "name":"browser_session_open",
                "arguments":{"mode":"chrome_extension"}
            }
        }),
    );
    let opened = receive(&mut stdout);
    assert_eq!(opened["result"]["isError"], false, "{opened}");
    assert_eq!(
        opened["result"]["structuredContent"]["browser"]["mode"],
        "chrome_extension"
    );
    assert!(
        opened["result"]["structuredContent"]["browser_session_id"].is_null(),
        "internal session IDs must not be returned to the model"
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":"status",
            "method":"tools/call",
            "params":{
                "name":"browser_session_status",
                "arguments":{}
            }
        }),
    );
    let status = receive(&mut stdout);
    assert_eq!(status["result"]["isError"], false, "{status}");
    assert_eq!(status["result"]["structuredContent"]["state"], "open");
    assert!(status["result"]["structuredContent"]["browser_session_id"].is_null());

    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":2,
            "method":"tools/call",
            "params":{
                "name":"browser_cdp_send",
                "arguments":{
                    "method":"Runtime.evaluate",
                    "params":{"expression":"1 + 1"}
                }
            }
        }),
    );
    let evaluated = receive(&mut stdout);
    assert_eq!(evaluated["result"]["isError"], false, "{evaluated}");
    assert_eq!(
        evaluated["result"]["structuredContent"]["result"]["value"],
        2
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":"close",
            "method":"tools/call",
            "params":{
                "name":"browser_session_close",
                "arguments":{}
            }
        }),
    );
    let closed = receive(&mut stdout);
    assert_eq!(closed["result"]["isError"], false, "{closed}");
    assert_eq!(closed["result"]["structuredContent"]["closed"], true);

    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":"status-after-close",
            "method":"tools/call",
            "params":{
                "name":"browser_session_status",
                "arguments":{}
            }
        }),
    );
    let status_after_close = receive(&mut stdout);
    assert_eq!(status_after_close["result"]["isError"], true);
    assert!(
        status_after_close["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("call browser_session_open first")
    );

    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
    );
    assert_eq!(receive(&mut stdout)["id"], 3);
    drop(stdin);
    assert!(child.wait().unwrap().success());
    bridge.join().unwrap();
}

#[test]
fn chrome_extension_mode_fails_closed_without_owned_or_development_bridge() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_chatos-browser-cdp"))
        .arg("mcp")
        .env_remove("CHATOS_BROWSER_BRIDGE_ENDPOINT")
        .env_remove("CHATOS_BROWSER_BRIDGE_TOKEN")
        .env_remove("CHATOS_BROWSER_BRIDGE_CREDENTIAL_FILE")
        .env_remove("CHATOS_BROWSER_EXTENSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn MCP server");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{
                "name":"browser_session_open",
                "arguments":{"mode":"chrome_extension"}
            }
        }),
    );
    let response = receive(&mut stdout);
    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("browser mode ChromeExtension")
    );
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
    );
    assert_eq!(receive(&mut stdout)["id"], 2);
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

fn serve_bridge() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let bridge = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = accept_hdr(stream, |request: &Request, mut response: Response| {
            assert_eq!(
                request.headers().get(SEC_WEBSOCKET_PROTOCOL).unwrap(),
                BRIDGE_SUBPROTOCOL
            );
            response.headers_mut().insert(
                SEC_WEBSOCKET_PROTOCOL,
                HeaderValue::from_static(BRIDGE_SUBPROTOCOL),
            );
            Ok(response)
        })
        .unwrap();
        let Message::Text(authentication) = socket.read().unwrap() else {
            panic!("authentication must be a text frame");
        };
        let authentication: Value = serde_json::from_str(&authentication).unwrap();
        assert_eq!(authentication["method"], "bridge.authenticate");
        assert_eq!(authentication["params"]["token"], BRIDGE_TOKEN);
        socket
            .send(Message::text(
                json!({
                    "type":"response",
                    "id":1,
                    "result":{
                        "protocol_version":"1.0",
                        "connection_id":"contract-connection",
                        "product":"Contract Chrome/1",
                        "user_agent":"Contract Chrome",
                        "capabilities":["page_control","raw_cdp"]
                    }
                })
                .to_string(),
            ))
            .unwrap();
        while let Ok(Message::Text(text)) = socket.read() {
            let request: Value = serde_json::from_str(&text).unwrap();
            let id = request["id"].as_u64().unwrap();
            let method = request["method"].as_str().unwrap();
            let result = match method {
                "bridge.listTargets" => json!({
                    "targets":[{
                        "id":"contract-tab",
                        "title":"Existing tab",
                        "url":"https://example.test/",
                        "kind":"page"
                    }]
                }),
                "bridge.attachTarget" => json!({"session_id":"contract-session"}),
                "cdp.send" => {
                    assert_eq!(request["params"]["session_id"], "contract-session");
                    assert_eq!(request["params"]["method"], "Runtime.evaluate");
                    json!({
                        "result":{"result":{"type":"number","value":2}}
                    })
                }
                "bridge.close" => json!({}),
                other => panic!("unexpected Bridge method {other}"),
            };
            socket
                .send(Message::text(
                    json!({"type":"response","id":id,"result":result}).to_string(),
                ))
                .unwrap();
            if method == "bridge.close" {
                let _ = socket.close(None);
                break;
            }
        }
    });
    (format!("ws://{address}/v1/browser"), bridge)
}

fn send(stdin: &mut impl Write, message: Value) {
    writeln!(stdin, "{}", serde_json::to_string(&message).unwrap()).unwrap();
    stdin.flush().unwrap();
}

fn receive(stdout: &mut impl BufRead) -> Value {
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(
        !line.trim().is_empty(),
        "server closed stdout without a response"
    );
    serde_json::from_str(&line).expect("stdout line must be JSON")
}
