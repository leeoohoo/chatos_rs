use std::{collections::HashMap, sync::Arc, time::Duration};

use browser_cdp_core::{BrowserRuntime, CoreError};
use browser_cdp_policy::{MAX_TOOL_RESULT_CHARS, truncate_serializable};
use browser_cdp_protocol::{
    OpenBrowserRequest, PROTOCOL_VERSION, RouteAction, RouteRule, SERVER_NAME, SERVER_VERSION,
};
use serde_json::{Map, Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, mpsc},
    task::AbortHandle,
};

type PendingRequests = Arc<Mutex<HashMap<String, AbortHandle>>>;
type ActiveBrowserSession = Arc<Mutex<Option<String>>>;

const SERVER_INSTRUCTIONS: &str = "Users describe browser goals, not tool sequences. Call browser_session_open before browser work. Browser mode fields do not exist in the model-facing input: ChatOS uses the user's paired Google Chrome and creates a native task tab group after authorization, then automatically falls back to an isolated browser before authorization. Read the actual mode from every browser_session_open result before continuing: chrome_extension means the user's Google Chrome, while managed means the isolated fallback. The Browser MCP process binds the opened session internally and automatically supplies it to later tools, so never ask the user for a session ID and never add browser_session_id to tool arguments. After navigation, tab changes, or page transitions, call browser_snapshot before interacting and use only refs from the newest snapshot. Verify meaningful actions with a fresh browser_snapshot, or browser_session_status when session health or the current mode matters. If browser_navigate times out, check session status and snapshot before concluding that navigation failed. If the process or session is unavailable, reopen once and replay from the last verified step; do not loop. Close the session when browser work is complete. Prefer high-level browser tools and use browser_cdp_send only when they are insufficient.";

pub async fn serve_stdio(runtime: Arc<BrowserRuntime>) -> Result<(), std::io::Error> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<Value>();
    let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
    let active_browser_session: ActiveBrowserSession = Arc::new(Mutex::new(None));
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = output_rx.recv().await {
            let mut bytes =
                serde_json::to_vec(&message).expect("JSON-RPC response is serializable");
            bytes.push(b'\n');
            stdout.write_all(&bytes).await?;
            stdout.flush().await?;
        }
        Ok::<_, std::io::Error>(())
    });

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break; };
                if line.trim().is_empty() { continue; }
                let message: Value = match serde_json::from_str(&line) {
                    Ok(message) => message,
                    Err(error) => {
                        let _ = output_tx.send(error_response(Value::Null, -32700, format!("parse error: {error}")));
                        continue;
                    }
                };
                let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
                if method == "notifications/cancelled" {
                    if let Some(request_id) = message.pointer("/params/requestId") {
                        let key = id_key(request_id);
                        if let Some(handle) = pending.lock().await.remove(&key) {
                            handle.abort();
                        }
                    }
                    continue;
                }
                if method == "notifications/initialized" {
                    continue;
                }
                if method == "exit" {
                    break;
                }

                let Some(id) = message.get("id").cloned() else {
                    continue;
                };
                let key = id_key(&id);
                let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
                if method == "shutdown" {
                    let _ = output_tx.send(success_response(id, Value::Null));
                    break;
                }
                let runtime = runtime.clone();
                let output_tx = output_tx.clone();
                let pending_for_task = pending.clone();
                let active_browser_session = active_browser_session.clone();
                let method = method.to_owned();
                let key_for_task = key.clone();
                let task = tokio::spawn(async move {
                    let response = dispatch(
                        runtime,
                        active_browser_session,
                        id.clone(),
                        &method,
                        params,
                    )
                    .await;
                    let _ = output_tx.send(response);
                    pending_for_task.lock().await.remove(&key_for_task);
                });
                pending.lock().await.insert(key, task.abort_handle());
            }
            _ = shutdown_signal() => break,
        }
    }

    for (_, handle) in pending.lock().await.drain() {
        handle.abort();
    }
    runtime.close_all().await;
    drop(output_tx);
    writer.await.map_err(std::io::Error::other)??;
    Ok(())
}

async fn dispatch(
    runtime: Arc<BrowserRuntime>,
    active_browser_session: ActiveBrowserSession,
    id: Value,
    method: &str,
    params: Value,
) -> Value {
    match method {
        "initialize" => success_response(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                "instructions": SERVER_INSTRUCTIONS
            }),
        ),
        "ping" => success_response(id, json!({})),
        "tools/list" => success_response(id, json!({ "tools": tool_catalog() })),
        "tools/call" => match call_tool(runtime, active_browser_session, params).await {
            Ok(result) => success_response(id, result),
            Err(error) => success_response(
                id,
                json!({
                    "content": [{ "type": "text", "text": error.to_string() }],
                    "isError": true
                }),
            ),
        },
        _ => error_response(id, -32601, format!("method not found: {method}")),
    }
}

async fn call_tool(
    runtime: Arc<BrowserRuntime>,
    active_browser_session: ActiveBrowserSession,
    params: Value,
) -> Result<Value, CoreError> {
    let name = required_string(&params, "name")?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let max_chars = params
        .pointer("/_meta/chatos~1toolResultMaxChars")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(MAX_TOOL_RESULT_CHARS)
        .min(MAX_TOOL_RESULT_CHARS);
    let browser_session_id = if name == "browser_session_open" {
        None
    } else {
        Some(resolve_browser_session_id(&arguments, &active_browser_session).await?)
    };

    let result = match name.as_str() {
        "browser_session_open" => {
            let request: OpenBrowserRequest = serde_json::from_value(arguments)
                .map_err(|error| CoreError::InvalidRequest(error.to_string()))?;
            let opened = runtime.open_session(request).await?;
            *active_browser_session.lock().await = Some(opened.browser_session_id.clone());
            serde_json::to_value(opened).unwrap()
        }
        "browser_session_status" => serde_json::to_value(
            runtime
                .session_status(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                )
                .await?,
        )
        .unwrap(),
        "browser_session_close" => {
            let closing_session_id = browser_session_id
                .as_deref()
                .expect("browser session ID resolved")
                .to_owned();
            runtime.close_session(&closing_session_id).await?;
            let mut active = active_browser_session.lock().await;
            if active.as_deref() == Some(closing_session_id.as_str()) {
                *active = None;
            }
            json!({ "closed": true })
        }
        "browser_tabs" | "browser_cdp_targets" => serde_json::to_value(
            runtime
                .tabs(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                )
                .await?,
        )
        .unwrap(),
        "browser_tab_new" => serde_json::to_value(
            runtime
                .new_tab(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or("about:blank"),
                )
                .await?,
        )
        .unwrap(),
        "browser_tab_switch" => serde_json::to_value(
            runtime
                .switch_tab(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "tab_id")?,
                )
                .await?,
        )
        .unwrap(),
        "browser_tab_close" => {
            runtime
                .close_tab(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "tab_id")?,
                )
                .await?;
            json!({ "closed": true })
        }
        "browser_navigate" => {
            runtime
                .navigate(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments.get("tab_id").and_then(Value::as_str),
                    &required_string(&arguments, "url")?,
                    duration_ms(&arguments, "timeout_ms", 15_000, 60_000),
                )
                .await?
        }
        "browser_snapshot" => serde_json::to_value(
            runtime
                .snapshot(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments.get("tab_id").and_then(Value::as_str),
                )
                .await?,
        )
        .unwrap(),
        "browser_find" => serde_json::to_value(
            runtime
                .find(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "query")?,
                    arguments
                        .get("max_results")
                        .and_then(Value::as_u64)
                        .unwrap_or(20) as usize,
                )
                .await?,
        )
        .unwrap(),
        "browser_click" => {
            runtime
                .click(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "ref")?,
                )
                .await?
        }
        "browser_type" => {
            runtime
                .type_text(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "ref")?,
                    &required_string(&arguments, "text")?,
                    arguments
                        .get("clear")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                )
                .await?
        }
        "browser_fill_form" => {
            let browser_session_id = browser_session_id
                .as_deref()
                .expect("browser session ID resolved");
            let fields = arguments
                .get("fields")
                .and_then(Value::as_array)
                .ok_or_else(|| CoreError::InvalidRequest("fields must be an array".into()))?;
            let mut filled = 0;
            for field in fields {
                runtime
                    .type_text(
                        browser_session_id,
                        &required_string(field, "ref")?,
                        &required_string(field, "value")?,
                        true,
                    )
                    .await?;
                filled += 1;
            }
            json!({ "filled": filled })
        }
        "browser_upload" => {
            let file_grant_ids = arguments
                .get("file_grant_ids")
                .and_then(Value::as_array)
                .ok_or_else(|| CoreError::InvalidRequest("file_grant_ids must be an array".into()))?
                .iter()
                .map(|grant| {
                    grant.as_str().map(str::to_owned).ok_or_else(|| {
                        CoreError::InvalidRequest(
                            "every file_grant_ids entry must be a string".into(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            runtime
                .upload(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "ref")?,
                    &file_grant_ids,
                )
                .await?
        }
        "browser_press" => {
            runtime
                .press(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "key")?,
                )
                .await?
        }
        "browser_scroll" => {
            runtime
                .scroll(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments
                        .get("delta_x")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    arguments
                        .get("delta_y")
                        .and_then(Value::as_i64)
                        .unwrap_or(600),
                )
                .await?
        }
        "browser_wait" => {
            runtime
                .wait(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments.get("selector").and_then(Value::as_str),
                    arguments.get("text").and_then(Value::as_str),
                    duration_ms(&arguments, "timeout_ms", 5_000, 20_000),
                )
                .await?
        }
        "browser_handle_dialog" => {
            runtime
                .handle_dialog(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments
                        .get("accept")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            CoreError::InvalidRequest("accept must be a boolean".into())
                        })?,
                    arguments.get("prompt_text").and_then(Value::as_str),
                )
                .await?
        }
        "browser_screenshot" => serde_json::to_value(
            runtime
                .screenshot(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments
                        .get("full_page")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                )
                .await?,
        )
        .unwrap(),
        "browser_downloads" => {
            let browser_session_id = browser_session_id
                .as_deref()
                .expect("browser session ID resolved");
            match arguments
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("start")
            {
                "start" => json!({
                    "subscription_id": runtime.downloads_start(browser_session_id).await?
                }),
                "events" => serde_json::to_value(
                    runtime
                        .cdp_events(
                            browser_session_id,
                            &required_string(&arguments, "subscription_id")?,
                            arguments
                                .get("after_sequence")
                                .and_then(Value::as_u64)
                                .unwrap_or(0),
                            1_000,
                            duration_ms(&arguments, "wait_ms", 0, 5_000),
                        )
                        .await?,
                )
                .unwrap(),
                "collect" => serde_json::to_value(
                    runtime
                        .downloads_collect(
                            browser_session_id,
                            &required_string(&arguments, "subscription_id")?,
                            arguments
                                .get("after_sequence")
                                .and_then(Value::as_u64)
                                .unwrap_or(0),
                            duration_ms(&arguments, "wait_ms", 0, 5_000),
                        )
                        .await?,
                )
                .unwrap(),
                "stop" => {
                    runtime
                        .downloads_stop(
                            browser_session_id,
                            &required_string(&arguments, "subscription_id")?,
                        )
                        .await?;
                    json!({ "stopped": true })
                }
                _ => {
                    return Err(CoreError::InvalidRequest(
                        "action must be start, events, collect, or stop".into(),
                    ));
                }
            }
        }
        "browser_console" => {
            event_stream_action(
                runtime.clone(),
                browser_session_id
                    .as_deref()
                    .expect("browser session ID resolved"),
                &arguments,
                &["Runtime.consoleAPICalled", "Runtime.exceptionThrown"],
            )
            .await?
        }
        "browser_network" => {
            event_stream_action(
                runtime.clone(),
                browser_session_id
                    .as_deref()
                    .expect("browser session ID resolved"),
                &arguments,
                &[
                    "Network.requestWillBeSent",
                    "Network.responseReceived",
                    "Network.loadingFinished",
                    "Network.loadingFailed",
                ],
            )
            .await?
        }
        "browser_network_request" => {
            let mut batch = runtime
                .cdp_events(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "subscription_id")?,
                    arguments
                        .get("after_sequence")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    arguments
                        .get("max_events")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize,
                    duration_ms(&arguments, "wait_ms", 0, 5_000),
                )
                .await?;
            let request_id = required_string(&arguments, "request_id")?;
            batch.events.retain(|event| {
                event.params.get("requestId").and_then(Value::as_str) == Some(request_id.as_str())
            });
            serde_json::to_value(batch).unwrap()
        }
        "browser_har_start" => json!({
            "subscription_id": runtime.har_start(
                browser_session_id.as_deref().expect("browser session ID resolved")
            ).await?
        }),
        "browser_har_stop" => serde_json::to_value(
            runtime
                .har_stop(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "subscription_id")?,
                )
                .await?,
        )
        .unwrap(),
        "browser_websocket_start" => json!({
            "subscription_id": runtime.cdp_subscribe(
                browser_session_id.as_deref().expect("browser session ID resolved"),
                None,
                vec![
                    "Network.webSocketCreated".into(),
                    "Network.webSocketWillSendHandshakeRequest".into(),
                    "Network.webSocketHandshakeResponseReceived".into(),
                    "Network.webSocketFrameSent".into(),
                    "Network.webSocketFrameReceived".into(),
                    "Network.webSocketFrameError".into(),
                    "Network.webSocketClosed".into(),
                ],
            ).await?
        }),
        "browser_websocket_events" => serde_json::to_value(
            runtime
                .cdp_events(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "subscription_id")?,
                    arguments
                        .get("after_sequence")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    arguments
                        .get("max_events")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize,
                    duration_ms(&arguments, "wait_ms", 0, 5_000),
                )
                .await?,
        )
        .unwrap(),
        "browser_websocket_stop" => {
            runtime
                .cdp_unsubscribe(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "subscription_id")?,
                )
                .await?;
            json!({ "unsubscribed": true })
        }
        "browser_route_add" => {
            let action: RouteAction = serde_json::from_value(
                arguments
                    .get("action")
                    .cloned()
                    .ok_or_else(|| CoreError::InvalidRequest("action is required".into()))?,
            )
            .map_err(|error| CoreError::InvalidRequest(error.to_string()))?;
            serde_json::to_value(
                runtime
                    .route_add(
                        browser_session_id
                            .as_deref()
                            .expect("browser session ID resolved"),
                        arguments.get("tab_id").and_then(Value::as_str),
                        RouteRule {
                            url_pattern: required_string(&arguments, "url_pattern")?,
                            action,
                        },
                    )
                    .await?,
            )
            .unwrap()
        }
        "browser_route_list" => serde_json::to_value(
            runtime
                .route_list(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                )
                .await?,
        )
        .unwrap(),
        "browser_route_remove" => {
            runtime
                .route_remove(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "route_id")?,
                )
                .await?;
            json!({ "removed": true })
        }
        "browser_route_clear" => json!({
            "removed_count": runtime.route_clear(
                browser_session_id.as_deref().expect("browser session ID resolved")
            ).await?
        }),
        "browser_cdp_attach" => json!({
            "cdp_session_id": runtime.cdp_attach(
                browser_session_id.as_deref().expect("browser session ID resolved"),
                &required_string(&arguments, "tab_id")?,
            ).await?
        }),
        "browser_cdp_detach" => {
            runtime
                .cdp_detach(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "cdp_session_id")?,
                )
                .await?;
            json!({ "detached": true })
        }
        "browser_cdp_send" => {
            runtime
                .cdp_send(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    arguments.get("cdp_session_id").and_then(Value::as_str),
                    arguments
                        .get("target")
                        .and_then(Value::as_str)
                        .unwrap_or("page"),
                    &required_string(&arguments, "method")?,
                    arguments
                        .get("params")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                    duration_ms(&arguments, "timeout_ms", 5_000, 15_000),
                )
                .await?
        }
        "browser_cdp_subscribe" => {
            let methods = arguments
                .get("methods")
                .and_then(Value::as_array)
                .ok_or_else(|| CoreError::InvalidRequest("methods must be an array".into()))?
                .iter()
                .map(|method| {
                    method.as_str().map(str::to_owned).ok_or_else(|| {
                        CoreError::InvalidRequest("every methods entry must be a string".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            json!({
                "subscription_id": runtime.cdp_subscribe(
                    browser_session_id.as_deref().expect("browser session ID resolved"),
                    arguments.get("cdp_session_id").and_then(Value::as_str),
                    methods,
                ).await?
            })
        }
        "browser_cdp_events" => serde_json::to_value(
            runtime
                .cdp_events(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "subscription_id")?,
                    arguments
                        .get("after_sequence")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    arguments
                        .get("max_events")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize,
                    duration_ms(&arguments, "wait_ms", 0, 5_000),
                )
                .await?,
        )
        .unwrap(),
        "browser_cdp_unsubscribe" => {
            runtime
                .cdp_unsubscribe(
                    browser_session_id
                        .as_deref()
                        .expect("browser session ID resolved"),
                    &required_string(&arguments, "subscription_id")?,
                )
                .await?;
            json!({ "unsubscribed": true })
        }
        _ => return Err(CoreError::NotFound(format!("tool {name}"))),
    };
    let mut result = result;
    if matches!(
        name.as_str(),
        "browser_session_open" | "browser_session_status"
    ) {
        hide_browser_session_id_from_result(&mut result);
    }
    let artifact_candidates = artifact_registration_candidates(&result);
    let result = truncate_serializable(&result, max_chars);
    let text = serde_json::to_string(&result).unwrap_or_else(|_| "null".into());
    let mut response = json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": result,
        "isError": false
    });
    if !artifact_candidates.is_empty() {
        response["_meta"] = json!({"chatos/artifacts": artifact_candidates});
    }
    Ok(response)
}

fn artifact_registration_candidates(value: &Value) -> Vec<Value> {
    fn visit(value: &Value, output: &mut Vec<Value>) {
        if output.len() >= 64 {
            return;
        }
        match value {
            Value::Object(object) => {
                let candidate = (
                    object.get("artifact_id").and_then(Value::as_str),
                    object.get("relative_path").and_then(Value::as_str),
                    object.get("display_name").and_then(Value::as_str),
                    object.get("media_type").and_then(Value::as_str),
                    object.get("size_bytes").and_then(Value::as_u64),
                    object.get("sha256").and_then(Value::as_str),
                );
                if let (
                    Some(producer_artifact_id),
                    Some(relative_path),
                    Some(display_name),
                    Some(media_type),
                    Some(size_bytes),
                    Some(sha256),
                ) = candidate
                {
                    output.push(json!({
                        "producer_artifact_id": producer_artifact_id,
                        "relative_path": relative_path,
                        "display_name": display_name,
                        "media_type": media_type,
                        "size_bytes": size_bytes,
                        "sha256": sha256,
                    }));
                    return;
                }
                for child in object.values() {
                    visit(child, output);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, output);
                }
            }
            _ => {}
        }
    }

    let mut output = Vec::new();
    visit(value, &mut output);
    output
}

async fn event_stream_action(
    runtime: Arc<BrowserRuntime>,
    browser_session_id: &str,
    arguments: &Value,
    methods: &[&str],
) -> Result<Value, CoreError> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if arguments.get("subscription_id").is_some() {
                "events"
            } else {
                "start"
            }
        });
    match action {
        "start" => Ok(json!({
            "subscription_id": runtime.cdp_subscribe(
                browser_session_id,
                None,
                methods.iter().map(|method| (*method).to_owned()).collect(),
            ).await?
        })),
        "events" => Ok(serde_json::to_value(
            runtime
                .cdp_events(
                    browser_session_id,
                    &required_string(arguments, "subscription_id")?,
                    arguments
                        .get("after_sequence")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    arguments
                        .get("max_events")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize,
                    duration_ms(arguments, "wait_ms", 0, 5_000),
                )
                .await?,
        )
        .unwrap()),
        "stop" => {
            runtime
                .cdp_unsubscribe(
                    browser_session_id,
                    &required_string(arguments, "subscription_id")?,
                )
                .await?;
            Ok(json!({ "unsubscribed": true }))
        }
        _ => Err(CoreError::InvalidRequest(
            "action must be start, events, or stop".into(),
        )),
    }
}

fn tool_catalog() -> Vec<Value> {
    let mut tools = vec![
        tool_with_permission_rules(
            "browser_session_open",
            "Open the browser selected by ChatOS from verified local authorization: paired Google Chrome with a task-named native tab group when available, otherwise an isolated managed browser. The input has no browser-mode fields. Read the returned mode: chrome_extension is the user's Chrome; managed is the isolated fallback.",
            object_schema(
                vec![(
                    "session_name",
                    json!({
                        "type":"string",
                        "minLength":1,
                        "maxLength":80,
                        "description":"Human-readable task name used for the native Chrome tab group in chrome_extension mode."
                    }),
                )],
                &[],
            ),
            &[],
            "high",
            "per_call",
            false,
            20_000,
            json!([
                {
                    "argumentPointer": "/mode",
                    "equals": "managed",
                    "requiredPermissions": ["browser.managed.launch"]
                },
                {
                    "argumentPointer": "/mode",
                    "equals": "chrome_extension",
                    "matchWhenMissing": true,
                    "requiredPermissions": ["browser.chrome.attach"]
                }
            ]),
        ),
        tool(
            "browser_session_status",
            "Get browser session status, actual current mode, and capabilities.",
            session_schema(),
            &["browser.page.read"],
            "low",
            "none",
            true,
            5_000,
        ),
        tool(
            "browser_session_close",
            "Close a browser session and owned browser process.",
            session_schema(),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_tabs",
            "List tabs without exposing backend target identifiers.",
            session_schema(),
            &["browser.page.read"],
            "low",
            "none",
            true,
            5_000,
        ),
        tool(
            "browser_tab_new",
            "Open a new tab.",
            object_schema(
                vec![
                    session_prop(),
                    ("url", json!({"type":"string","default":"about:blank"})),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            15_000,
        ),
        tool(
            "browser_tab_switch",
            "Select the active tab.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id", "tab_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            5_000,
        ),
        tool(
            "browser_tab_close",
            "Close a tab.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id", "tab_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_navigate",
            "Navigate a tab to an HTTP, HTTPS, or about URL and wait for readiness.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("tab_id"),
                    string_prop("url"),
                    timeout_prop(60_000),
                ],
                &["browser_session_id", "url"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            60_000,
        ),
        tool(
            "browser_snapshot",
            "Return a compact accessibility-oriented snapshot with opaque element refs.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id"],
            ),
            &["browser.page.read"],
            "low",
            "none",
            true,
            20_000,
        ),
        tool(
            "browser_find",
            "Find elements by accessible role, name, text, or value.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("query"),
                    (
                        "max_results",
                        json!({"type":"integer","minimum":1,"maximum":100,"default":20}),
                    ),
                ],
                &["browser_session_id", "query"],
            ),
            &["browser.page.read"],
            "low",
            "none",
            true,
            20_000,
        ),
        tool(
            "browser_click",
            "Click an element from the latest snapshot.",
            ref_schema(),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_type",
            "Type into an element from the latest snapshot.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("ref"),
                    string_prop("text"),
                    ("clear", json!({"type":"boolean","default":false})),
                ],
                &["browser_session_id", "ref", "text"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_fill_form",
            "Fill several fields identified by refs.",
            object_schema(
                vec![
                    session_prop(),
                    (
                        "fields",
                        json!({"type":"array","maxItems":50,"items":{"type":"object","properties":{"ref":{"type":"string"},"value":{"type":"string"}},"required":["ref","value"],"additionalProperties":false}}),
                    ),
                ],
                &["browser_session_id", "fields"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            15_000,
        ),
        tool(
            "browser_upload",
            "Upload files to an input using short-lived Local Connector file grants only.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("ref"),
                    (
                        "file_grant_ids",
                        json!({"type":"array","minItems":1,"maxItems":20,"items":{"type":"string"}}),
                    ),
                ],
                &["browser_session_id", "ref", "file_grant_ids"],
            ),
            &["browser.file.transfer"],
            "critical",
            "per_call",
            false,
            15_000,
        ),
        tool(
            "browser_press",
            "Dispatch a key to the focused page element.",
            object_schema(
                vec![session_prop(), string_prop("key")],
                &["browser_session_id", "key"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_scroll",
            "Scroll the active page.",
            object_schema(
                vec![
                    session_prop(),
                    ("delta_x", json!({"type":"integer","default":0})),
                    ("delta_y", json!({"type":"integer","default":600})),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_wait",
            "Wait for a selector, visible text, or document readiness.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("selector"),
                    string_prop("text"),
                    timeout_prop(20_000),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.read"],
            "low",
            "none",
            true,
            20_000,
        ),
        tool(
            "browser_handle_dialog",
            "Accept or dismiss the currently open JavaScript dialog.",
            object_schema(
                vec![
                    session_prop(),
                    ("accept", json!({"type":"boolean"})),
                    string_prop("prompt_text"),
                ],
                &["browser_session_id", "accept"],
            ),
            &["browser.page.control"],
            "high",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_screenshot",
            "Capture a PNG into the session artifact directory.",
            object_schema(
                vec![
                    session_prop(),
                    ("full_page", json!({"type":"boolean","default":false})),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.read", "browser.file.transfer"],
            "high",
            "per_call",
            false,
            20_000,
        ),
        tool(
            "browser_downloads",
            "Start, poll, collect, or stop downloads confined to the plugin artifact directory.",
            object_schema(
                vec![
                    session_prop(),
                    (
                        "action",
                        json!({"type":"string","enum":["start","events","collect","stop"],"default":"start"}),
                    ),
                    string_prop("subscription_id"),
                    event_cursor_prop(),
                    wait_ms_prop(),
                ],
                &["browser_session_id"],
            ),
            &["browser.file.transfer"],
            "high",
            "per_call",
            false,
            20_000,
        ),
        tool(
            "browser_console",
            "Start, poll, or stop a bounded console and page-exception event stream.",
            event_stream_schema(),
            &["browser.network.observe"],
            "low",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_network",
            "Start, poll, or stop a bounded redacted network event stream.",
            event_stream_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_network_request",
            "Poll network events for one CDP request identifier.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("subscription_id"),
                    string_prop("request_id"),
                    event_cursor_prop(),
                    event_limit_prop(),
                    wait_ms_prop(),
                ],
                &["browser_session_id", "subscription_id", "request_id"],
            ),
            &["browser.network.observe"],
            "medium",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_har_start",
            "Start bounded HAR capture with sensitive request fields redacted.",
            session_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_har_stop",
            "Stop HAR capture and write a HAR 1.2 artifact.",
            subscription_schema(),
            &["browser.network.observe", "browser.file.transfer"],
            "high",
            "per_call",
            false,
            20_000,
        ),
        tool(
            "browser_websocket_start",
            "Start a bounded redacted WebSocket lifecycle and frame event stream.",
            session_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_websocket_events",
            "Poll a WebSocket event stream by sequence.",
            event_poll_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_websocket_stop",
            "Stop a WebSocket event stream.",
            subscription_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            false,
            5_000,
        ),
        tool(
            "browser_route_add",
            "Add an approved URL-pattern route that only aborts or returns fixed JSON.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("tab_id"),
                    string_prop("url_pattern"),
                    (
                        "action",
                        json!({
                            "oneOf": [
                                {
                                    "type": "object",
                                    "properties": { "type": { "const": "abort" } },
                                    "required": ["type"],
                                    "additionalProperties": false
                                },
                                {
                                    "type": "object",
                                    "properties": {
                                        "type": { "const": "mock_json" },
                                        "status": { "type": "integer", "minimum": 100, "maximum": 599, "default": 200 },
                                        "body": {}
                                    },
                                    "required": ["type", "body"],
                                    "additionalProperties": false
                                }
                            ]
                        }),
                    ),
                ],
                &["browser_session_id", "url_pattern", "action"],
            ),
            &["browser.network.intercept"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_route_list",
            "List active request routes without backend identifiers.",
            session_schema(),
            &["browser.network.intercept"],
            "medium",
            "none",
            true,
            5_000,
        ),
        tool(
            "browser_route_remove",
            "Remove one request route.",
            object_schema(
                vec![session_prop(), string_prop("route_id")],
                &["browser_session_id", "route_id"],
            ),
            &["browser.network.intercept"],
            "high",
            "none",
            false,
            5_000,
        ),
        tool(
            "browser_route_clear",
            "Remove all request routes in a browser session.",
            session_schema(),
            &["browser.network.intercept"],
            "high",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_targets",
            "List page targets using opaque tab IDs.",
            session_schema(),
            &["browser.cdp.raw"],
            "high",
            "per_call",
            true,
            5_000,
        ),
        tool(
            "browser_cdp_attach",
            "Create an opaque raw CDP session for a tab.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id", "tab_id"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_detach",
            "Detach an opaque raw CDP session.",
            object_schema(
                vec![session_prop(), string_prop("cdp_session_id")],
                &["browser_session_id", "cdp_session_id"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_send",
            "Execute a raw Chrome DevTools Protocol command without logging params or results.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("cdp_session_id"),
                    (
                        "target",
                        json!({"type":"string","enum":["page","browser"],"default":"page"}),
                    ),
                    string_prop("method"),
                    ("params", json!({"type":"object","default":{}})),
                    timeout_prop(15_000),
                ],
                &["browser_session_id", "method"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            15_000,
        ),
        tool(
            "browser_cdp_subscribe",
            "Subscribe to supported raw CDP events using a bounded local queue.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("cdp_session_id"),
                    (
                        "methods",
                        json!({"type":"array","minItems":1,"maxItems":32,"items":{"type":"string"}}),
                    ),
                ],
                &["browser_session_id", "methods"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_events",
            "Poll raw CDP events by sequence without relying on MCP notifications.",
            event_poll_schema(),
            &["browser.cdp.raw"],
            "high",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_cdp_unsubscribe",
            "Stop a raw CDP event subscription.",
            subscription_schema(),
            &["browser.cdp.raw"],
            "high",
            "none",
            false,
            5_000,
        ),
    ];
    for tool in &mut tools {
        hide_browser_session_id_from_schema(tool);
    }
    tools
}

fn hide_browser_session_id_from_schema(tool: &mut Value) {
    if let Some(properties) = tool
        .pointer_mut("/inputSchema/properties")
        .and_then(Value::as_object_mut)
    {
        properties.remove("browser_session_id");
    }
    if let Some(required) = tool
        .pointer_mut("/inputSchema/required")
        .and_then(Value::as_array_mut)
    {
        required.retain(|name| name.as_str() != Some("browser_session_id"));
    }
}

fn hide_browser_session_id_from_result(result: &mut Value) {
    if let Some(object) = result.as_object_mut() {
        object.remove("browser_session_id");
    }
}

#[allow(clippy::too_many_arguments)]
fn tool(
    name: &str,
    description: &str,
    mut input_schema: Value,
    permissions: &[&str],
    risk: &str,
    approval: &str,
    parallel_safe: bool,
    timeout_ms: u64,
) -> Value {
    if let Some(properties) = input_schema
        .pointer_mut("/properties")
        .and_then(Value::as_object_mut)
    {
        properties.insert(
            "skillEvidence".to_string(),
            json!({
                "type": "array",
                "minItems": 2,
                "maxItems": 8,
                "items": {"type": "string", "minLength": 1},
                "description": "Platform-issued activation evidence for browser-cdp and this tool's specialist Browser Skill. ChatOS validates and removes this field before local execution."
            }),
        );
    }
    if let Some(required) = input_schema
        .pointer_mut("/required")
        .and_then(Value::as_array_mut)
    {
        required.push(json!("skillEvidence"));
    }
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "_meta": {
            "chatos/policyVersion": 1,
            "chatos/requiredPermissions": permissions,
            "chatos/riskLevel": risk,
            "chatos/approvalMode": approval,
            "chatos/parallelSafe": parallel_safe,
            "chatos/timeoutMs": timeout_ms,
            "chatos/toolResultMaxChars": MAX_TOOL_RESULT_CHARS,
            "chatos/skillGate": {
                "evidenceArgument": "skillEvidence",
                "allOf": ["browser-cdp", browser_skill_for_tool(name)]
            }
        }
    })
}

fn browser_skill_for_tool(name: &str) -> &'static str {
    if name == "browser_upload" || name == "browser_downloads" {
        return "browser-file-transfer";
    }
    if name.starts_with("browser_console")
        || name.starts_with("browser_network")
        || name.starts_with("browser_har")
        || name.starts_with("browser_websocket")
        || name.starts_with("browser_route")
        || name.starts_with("browser_cdp")
    {
        return "browser-network-debugging";
    }
    if name == "browser_snapshot" || name == "browser_find" || name == "browser_screenshot" {
        return "browser-observation-verification";
    }
    if name == "browser_click"
        || name == "browser_type"
        || name == "browser_fill_form"
        || name == "browser_press"
        || name == "browser_scroll"
        || name == "browser_wait"
        || name == "browser_handle_dialog"
    {
        return "browser-interaction";
    }
    "browser-navigation"
}

#[allow(clippy::too_many_arguments)]
fn tool_with_permission_rules(
    name: &str,
    description: &str,
    input_schema: Value,
    permissions: &[&str],
    risk: &str,
    approval: &str,
    parallel_safe: bool,
    timeout_ms: u64,
    permission_rules: Value,
) -> Value {
    let mut definition = tool(
        name,
        description,
        input_schema,
        permissions,
        risk,
        approval,
        parallel_safe,
        timeout_ms,
    );
    definition["_meta"]["chatos/permissionRules"] = permission_rules;
    definition
}

fn object_schema(properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let properties = properties
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect::<Map<_, _>>();
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn session_prop() -> (&'static str, Value) {
    (
        "browser_session_id",
        json!({
            "type": "string",
            "description": "Opaque browser_session_id returned by browser_session_open. Copy it exactly; never derive it from or combine it with a tab_id."
        }),
    )
}
fn string_prop(name: &'static str) -> (&'static str, Value) {
    (name, json!({ "type": "string" }))
}
fn timeout_prop(maximum: u64) -> (&'static str, Value) {
    (
        "timeout_ms",
        json!({ "type": "integer", "minimum": 1, "maximum": maximum }),
    )
}
fn session_schema() -> Value {
    object_schema(vec![session_prop()], &["browser_session_id"])
}
fn ref_schema() -> Value {
    object_schema(
        vec![session_prop(), string_prop("ref")],
        &["browser_session_id", "ref"],
    )
}

fn event_stream_schema() -> Value {
    object_schema(
        vec![
            session_prop(),
            (
                "action",
                json!({"type":"string","enum":["start","events","stop"]}),
            ),
            string_prop("subscription_id"),
            event_cursor_prop(),
            event_limit_prop(),
            wait_ms_prop(),
        ],
        &["browser_session_id"],
    )
}

fn event_poll_schema() -> Value {
    object_schema(
        vec![
            session_prop(),
            string_prop("subscription_id"),
            event_cursor_prop(),
            event_limit_prop(),
            wait_ms_prop(),
        ],
        &["browser_session_id", "subscription_id"],
    )
}

fn subscription_schema() -> Value {
    object_schema(
        vec![session_prop(), string_prop("subscription_id")],
        &["browser_session_id", "subscription_id"],
    )
}

fn event_cursor_prop() -> (&'static str, Value) {
    (
        "after_sequence",
        json!({"type":"integer","minimum":0,"default":0}),
    )
}

fn event_limit_prop() -> (&'static str, Value) {
    (
        "max_events",
        json!({"type":"integer","minimum":1,"maximum":1000,"default":100}),
    )
}

fn wait_ms_prop() -> (&'static str, Value) {
    (
        "wait_ms",
        json!({"type":"integer","minimum":0,"maximum":5000,"default":0}),
    )
}

fn required_string(value: &Value, name: &str) -> Result<String, CoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CoreError::InvalidRequest(format!("{name} must be a non-empty string")))
}

async fn resolve_browser_session_id(
    arguments: &Value,
    active_browser_session: &ActiveBrowserSession,
) -> Result<String, CoreError> {
    if let Some(browser_session_id) = arguments
        .get("browser_session_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        return Ok(browser_session_id.to_owned());
    }
    active_browser_session.lock().await.clone().ok_or_else(|| {
        CoreError::InvalidRequest(
            "no browser session is bound; call browser_session_open first".into(),
        )
    })
}

fn duration_ms(value: &Value, name: &str, default: u64, maximum: u64) -> Duration {
    Duration::from_millis(
        value
            .get(name)
            .and_then(Value::as_u64)
            .unwrap_or(default)
            .clamp(1, maximum),
    )
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn id_key(id: &Value) -> String {
    serde_json::to_string(id).unwrap_or_else(|_| "null".into())
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_bounded_and_stable() {
        let catalog = tool_catalog();
        assert!(catalog.len() < 200);
        assert!(serde_json::to_vec(&catalog).unwrap().len() < 512 * 1024);
        let names = catalog
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len());
    }

    #[test]
    fn every_tool_has_policy_metadata() {
        for tool in tool_catalog() {
            assert!(tool.pointer("/_meta/chatos~1requiredPermissions").is_some());
            assert!(tool.pointer("/_meta/chatos~1riskLevel").is_some());
            assert!(tool.pointer("/_meta/chatos~1approvalMode").is_some());
            assert!(tool.pointer("/_meta/chatos~1timeoutMs").is_some());
            assert!(tool.pointer("/_meta/chatos~1toolResultMaxChars").is_some());
            assert_eq!(
                tool.pointer("/_meta/chatos~1skillGate/evidenceArgument"),
                Some(&json!("skillEvidence"))
            );
            assert_eq!(
                tool.pointer("/inputSchema/properties/skillEvidence/type"),
                Some(&json!("array"))
            );
            assert!(tool
                .pointer("/inputSchema/required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.contains(&json!("skillEvidence"))));
            assert!(matches!(
                tool.pointer("/_meta/chatos~1approvalMode")
                    .and_then(Value::as_str),
                Some("none" | "per_call")
            ));
        }
    }

    #[test]
    fn browser_session_id_is_not_exposed_in_tool_schemas() {
        for tool in tool_catalog() {
            assert!(
                tool.pointer("/inputSchema/properties/browser_session_id")
                    .is_none(),
                "{} still exposes browser_session_id",
                tool["name"]
            );
            assert!(
                tool.pointer("/inputSchema/required")
                    .and_then(Value::as_array)
                    .is_none_or(|required| {
                        required
                            .iter()
                            .all(|name| name.as_str() != Some("browser_session_id"))
                    }),
                "{} still requires browser_session_id",
                tool["name"]
            );
        }
    }

    #[tokio::test]
    async fn browser_session_resolution_prefers_legacy_argument_then_bound_session() {
        let active: ActiveBrowserSession = Arc::new(Mutex::new(Some("bound-session".into())));
        assert_eq!(
            resolve_browser_session_id(&json!({}), &active)
                .await
                .unwrap(),
            "bound-session"
        );
        assert_eq!(
            resolve_browser_session_id(&json!({"browser_session_id":"legacy-session"}), &active)
                .await
                .unwrap(),
            "legacy-session"
        );
    }

    #[tokio::test]
    async fn browser_session_resolution_requires_open_when_unbound() {
        let active: ActiveBrowserSession = Arc::new(Mutex::new(None));
        let error = resolve_browser_session_id(&json!({}), &active)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("call browser_session_open first")
        );
    }

    #[test]
    fn public_session_results_hide_internal_id() {
        let mut result = json!({
            "browser_session_id": "bs_private",
            "state": "open",
            "mode": "managed",
            "browser": {"mode": "managed"}
        });
        hide_browser_session_id_from_result(&mut result);
        assert!(result.get("browser_session_id").is_none());
        assert_eq!(result["state"], "open");
        assert_eq!(result["mode"], "managed");
    }

    #[test]
    fn session_open_hides_mode_and_keeps_internal_auto_selection_rules() {
        let tools = tool_catalog();
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == "browser_session_open")
            .unwrap();
        assert_eq!(
            tool.pointer("/_meta/chatos~1approvalMode"),
            Some(&json!("per_call"))
        );
        assert_eq!(
            tool.pointer("/_meta/chatos~1requiredPermissions"),
            Some(&json!([]))
        );
        assert!(tool.pointer("/inputSchema/properties/mode").is_none());
        assert!(tool.pointer("/inputSchema/properties/headless").is_none());
        assert!(
            tool.pointer("/inputSchema/properties/persistent_profile")
                .is_none()
        );
        assert!(
            tools
                .iter()
                .all(|tool| tool["name"] != "browser_session_open_managed")
        );
        assert_eq!(
            tool.pointer("/_meta/chatos~1permissionRules/0/requiredPermissions/0"),
            Some(&json!("browser.managed.launch"))
        );
        assert_eq!(
            tool.pointer("/_meta/chatos~1permissionRules/1/requiredPermissions/0"),
            Some(&json!("browser.chrome.attach"))
        );
        assert!(
            tool.pointer("/inputSchema/properties/executable_path")
                .is_none()
        );
    }

    #[test]
    fn artifact_candidates_are_explicit_and_path_relative() {
        let candidates = artifact_registration_candidates(&json!({
            "artifacts": [{
                "artifact_id": "artifact_local",
                "relative_path": "artifact_local-report.har",
                "display_name": "report.har",
                "media_type": "application/json",
                "size_bytes": 42,
                "sha256": "a".repeat(64)
            }]
        }));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["producer_artifact_id"], "artifact_local");
        assert!(candidates[0].get("absolute_path").is_none());
    }
}
