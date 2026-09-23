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

include!("lib_part01.rs");
include!("lib_part02.rs");
