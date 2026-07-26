// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};

use super::actions_network::{normalize_request_id, sanitize_body_text, sanitize_network_url};
use super::actions_shared::{
    browser_result_data, copy_response_fields, enrich_response_with_page_metadata, is_success,
    run_browser_command,
};
use super::BoundContext;
use crate::browser_command_support::parse_browser_command_eval_payload;

use super::super::managed_preview::{
    active_page_target_id, read_cdp_result, send_cdp_command, validate_loopback_cdp_endpoint,
    CDP_CONNECT_TIMEOUT, CDP_RESPONSE_TIMEOUT,
};

pub(super) const DEFAULT_BROWSER_WEBSOCKET_FRAME_LIMIT: usize = 100;
pub(super) const MAX_BROWSER_WEBSOCKET_FRAME_LIMIT: usize = 200;
pub(super) const DEFAULT_BROWSER_WEBSOCKET_PAYLOAD_CHARS: usize = 1_024;
pub(super) const MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS: usize = 4_096;

const MAX_CAPTURED_WEBSOCKET_FRAMES: usize = 1_000;
const MAX_CAPTURED_PAYLOAD_CHARS: usize = 1024 * 1024;
const MAX_CAPTURED_WEBSOCKET_SOCKETS: usize = 256;
const MAX_ACTIVE_WEBSOCKET_OBSERVERS: usize = 64;
const MAX_WEBSOCKET_CDP_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_WEBSOCKET_OBSERVER_LIFETIME: Duration = Duration::from_secs(30 * 60);
const MAX_WEBSOCKET_URL_CHARS: usize = 4_096;

static LIKELY_SECRET_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:eyJ[A-Za-z0-9_-]{20,}|[A-Za-z0-9_~+/=-]{32,})")
        .expect("valid likely secret token regex")
});
static WEBSOCKET_OBSERVERS: Lazy<Mutex<HashMap<String, WebSocketObserverHandle>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static WEBSOCKET_OBSERVER_RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("chatos-browser-websocket")
        .build()
        .expect("build browser WebSocket observer runtime")
});

#[derive(Debug, Clone)]
struct CapturedWebSocketFrame {
    sequence: u64,
    request_id: String,
    url: Option<String>,
    direction: &'static str,
    opcode: u64,
    payload_bytes: usize,
    text_payload_available: bool,
    sanitized_text: Option<String>,
    payload_truncated: bool,
    redaction_applied: bool,
    cdp_timestamp_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct ObservedWebSocket {
    url: Option<String>,
    closed: bool,
}

#[derive(Debug)]
struct WebSocketObserverState {
    active: bool,
    status: &'static str,
    warning: Option<String>,
    started_at_ms: u64,
    page_url: Option<String>,
    frames: VecDeque<CapturedWebSocketFrame>,
    sockets: HashMap<String, ObservedWebSocket>,
    total_frames: u64,
    dropped_frames: u64,
    closed_sockets: u64,
    protocol_errors: u64,
    created_events: u64,
    sent_frame_events: u64,
    received_frame_events: u64,
    closed_events: u64,
    frame_error_events: u64,
    captured_payload_chars: usize,
    next_sequence: u64,
}

impl WebSocketObserverState {
    fn new(page_url: Option<String>) -> Self {
        Self {
            active: true,
            status: "active",
            warning: None,
            started_at_ms: unix_timestamp_ms(),
            page_url,
            frames: VecDeque::new(),
            sockets: HashMap::new(),
            total_frames: 0,
            dropped_frames: 0,
            closed_sockets: 0,
            protocol_errors: 0,
            created_events: 0,
            sent_frame_events: 0,
            received_frame_events: 0,
            closed_events: 0,
            frame_error_events: 0,
            captured_payload_chars: 0,
            next_sequence: 1,
        }
    }

    fn finish(&mut self, status: &'static str, warning: Option<String>) {
        self.active = false;
        self.status = status;
        if warning.is_some() {
            self.warning = warning;
        }
    }

    fn push_frame(&mut self, mut frame: CapturedWebSocketFrame) {
        frame.sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.total_frames = self.total_frames.saturating_add(1);
        let payload_chars = frame
            .sanitized_text
            .as_deref()
            .map(|value| value.chars().count())
            .unwrap_or(0);
        while self.frames.len() >= MAX_CAPTURED_WEBSOCKET_FRAMES
            || self.captured_payload_chars.saturating_add(payload_chars)
                > MAX_CAPTURED_PAYLOAD_CHARS
        {
            let Some(removed) = self.frames.pop_front() else {
                break;
            };
            self.captured_payload_chars = self.captured_payload_chars.saturating_sub(
                removed
                    .sanitized_text
                    .as_deref()
                    .map(|value| value.chars().count())
                    .unwrap_or(0),
            );
            self.dropped_frames = self.dropped_frames.saturating_add(1);
        }
        self.captured_payload_chars = self.captured_payload_chars.saturating_add(payload_chars);
        self.frames.push_back(frame);
    }
}

#[derive(Clone)]
struct WebSocketObserverHandle {
    state: Arc<Mutex<WebSocketObserverState>>,
    abort: tokio::task::AbortHandle,
}

pub(super) async fn browser_websocket_start_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let observer_key = observer_key(&ctx, conversation_key.as_str());
    {
        let mut observers = WEBSOCKET_OBSERVERS.lock();
        observers.retain(|_, handle| !handle.abort.is_finished());
        if observers.contains_key(observer_key.as_str()) {
            return Err(
                "WebSocket observation is already active for this browser session".to_string(),
            );
        }
        if observers.len() >= MAX_ACTIVE_WEBSOCKET_OBSERVERS {
            return Err("WebSocket observer capacity is temporarily exhausted".to_string());
        }
    }

    let metadata_result = run_browser_command(
        &ctx,
        conversation_key.as_str(),
        "eval",
        vec!["JSON.stringify({url:window.location.href})".to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&metadata_result) {
        return Err("browser page identity is unavailable for WebSocket observation".to_string());
    }
    let page_url = websocket_page_url(&metadata_result)?;
    let session = ctx
        .sessions
        .lock()
        .get(conversation_key.as_str())
        .cloned()
        .ok_or_else(|| "browser session is unavailable for WebSocket observation".to_string())?;
    if session.cdp_url.is_some() {
        return Err(
            "WebSocket observation is limited to managed loopback browser sessions".to_string(),
        );
    }
    let endpoint_result = run_browser_command(
        &ctx,
        conversation_key.as_str(),
        "get",
        vec!["cdp-url".to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&endpoint_result) {
        return Err("managed browser CDP endpoint is unavailable".to_string());
    }
    let endpoint = endpoint_result
        .pointer("/data/cdpUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed browser CDP endpoint is malformed".to_string())?;
    let endpoint = validate_loopback_cdp_endpoint(endpoint)?.to_string();

    let sanitized_page_url = sanitize_network_url(page_url.as_str());
    let state = Arc::new(Mutex::new(WebSocketObserverState::new(
        sanitized_page_url.clone(),
    )));
    let task_state = Arc::clone(&state);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let task = WEBSOCKET_OBSERVER_RUNTIME.spawn(run_websocket_observer_task(
        endpoint, page_url, task_state, ready_tx,
    ));
    let handle = WebSocketObserverHandle {
        state,
        abort: task.abort_handle(),
    };
    match tokio::time::timeout(Duration::from_secs(7), ready_rx).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(error))) => {
            handle.abort.abort();
            return Err(error);
        }
        Ok(Err(_)) => {
            handle.abort.abort();
            return Err("managed browser WebSocket observer setup closed".to_string());
        }
        Err(_) => {
            handle.abort.abort();
            return Err("managed browser WebSocket observer setup timed out".to_string());
        }
    }
    let mut observers = WEBSOCKET_OBSERVERS.lock();
    observers.retain(|_, existing| !existing.abort.is_finished());
    if observers.contains_key(observer_key.as_str()) {
        handle.abort.abort();
        return Err(
            "WebSocket observation was started concurrently for this browser session".to_string(),
        );
    }
    if observers.len() >= MAX_ACTIVE_WEBSOCKET_OBSERVERS {
        handle.abort.abort();
        return Err("WebSocket observer capacity is temporarily exhausted".to_string());
    }
    observers.insert(observer_key, handle);
    drop(observers);

    let mut response = json!({
        "success": true,
        "source": "managed_loopback_cdp",
        "status": "active",
        "page_url": sanitized_page_url,
        "capture_limit_frames": MAX_CAPTURED_WEBSOCKET_FRAMES,
        "capture_limit_payload_chars": MAX_CAPTURED_PAYLOAD_CHARS,
        "process_observer_limit": MAX_ACTIVE_WEBSOCKET_OBSERVERS,
        "observer_lifetime_seconds": MAX_WEBSOCKET_OBSERVER_LIFETIME.as_secs(),
        "payload_policy": "sanitized_text_only_on_explicit_read",
        "binary_payloads_included": false,
        "_summary_text": "Started bounded WebSocket frame observation for the current managed browser page. Text payloads are sanitized in memory and omitted from reads unless explicitly requested; binary payloads are never returned."
    });
    copy_response_fields(&mut response, &endpoint_result, &["browser_session"]);
    Ok(response)
}

async fn run_websocket_observer_task(
    endpoint: String,
    page_url: String,
    state: Arc<Mutex<WebSocketObserverState>>,
    ready: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    let setup = async {
        let config = WebSocketConfig::default()
            .read_buffer_size(32 * 1024)
            .write_buffer_size(8 * 1024)
            .max_write_buffer_size(64 * 1024)
            .max_message_size(Some(MAX_WEBSOCKET_CDP_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_WEBSOCKET_CDP_MESSAGE_BYTES));
        let (mut stream, _) = tokio::time::timeout(
            CDP_CONNECT_TIMEOUT,
            connect_async_with_config(endpoint.as_str(), Some(config), true),
        )
        .await
        .map_err(|_| "managed browser WebSocket observer connection timed out".to_string())?
        .map_err(|_| "managed browser WebSocket observer connection failed".to_string())?;
        let session_id = tokio::time::timeout(CDP_RESPONSE_TIMEOUT, async {
            send_cdp_command(&mut stream, 1, "Target.getTargets", json!({}), None).await?;
            let targets = read_cdp_result(&mut stream, 1).await?;
            let target_id = active_page_target_id(&targets, page_url.as_str())?;
            send_cdp_command(
                &mut stream,
                2,
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
                None,
            )
            .await?;
            let attached = read_cdp_result(&mut stream, 2).await?;
            let session_id = attached
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= 256)
                .ok_or_else(|| {
                    "managed browser WebSocket observer attachment is malformed".to_string()
                })?
                .to_string();
            send_cdp_command(
                &mut stream,
                3,
                "Network.enable",
                json!({
                    "maxTotalBufferSize": 1024 * 1024,
                    "maxResourceBufferSize": 64 * 1024,
                    "maxPostDataSize": 0,
                    "reportDirectSocketTraffic": false,
                }),
                Some(session_id.as_str()),
            )
            .await?;
            read_cdp_result(&mut stream, 3).await?;
            Ok::<_, String>(session_id)
        })
        .await
        .map_err(|_| "managed browser WebSocket observer setup timed out".to_string())??;
        Ok::<_, String>((stream, session_id))
    }
    .await;

    match setup {
        Ok((mut stream, session_id)) => {
            if ready.send(Ok(())).is_err() {
                state.lock().finish("stopped", None);
                return;
            }
            observe_websocket_cdp_stream(&mut stream, session_id.as_str(), state).await;
        }
        Err(error) => {
            state.lock().finish("error", Some(error.clone()));
            let _ = ready.send(Err(error));
        }
    }
}

pub(super) async fn browser_websocket_frames_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    clear: bool,
    limit: usize,
    request_id: Option<String>,
    direction: Option<String>,
    include_text_payloads: bool,
    max_payload_chars: usize,
) -> Result<Value, String> {
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let observer_key = observer_key(&ctx, conversation_key.as_str());
    let request_id = request_id
        .map(|value| normalize_request_id(value.as_str()))
        .transpose()?;
    let direction = normalize_direction(direction)?;
    let limit = limit.clamp(1, MAX_BROWSER_WEBSOCKET_FRAME_LIMIT);
    let max_payload_chars = max_payload_chars.clamp(1, MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS);
    let handle = WEBSOCKET_OBSERVERS
        .lock()
        .get(observer_key.as_str())
        .cloned()
        .ok_or_else(|| {
            "WebSocket observation is not available; call browser_websocket_start first".to_string()
        })?;

    let (
        active,
        status,
        warning,
        started_at_ms,
        page_url,
        socket_count,
        open_socket_count,
        total_frames,
        dropped_frames,
        closed_sockets,
        protocol_errors,
        created_events,
        sent_frame_events,
        received_frame_events,
        closed_events,
        frame_error_events,
        frames,
    ) = {
        let mut state = handle.state.lock();
        let mut frames = state
            .frames
            .iter()
            .rev()
            .filter(|frame| {
                request_id
                    .as_ref()
                    .is_none_or(|value| frame.request_id == *value)
                    && direction.is_none_or(|value| frame.direction == value)
            })
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        frames.reverse();
        let snapshot = (
            state.active && !handle.abort.is_finished(),
            state.status,
            state.warning.clone(),
            state.started_at_ms,
            state.page_url.clone(),
            state.sockets.len(),
            state
                .sockets
                .values()
                .filter(|socket| !socket.closed)
                .count(),
            state.total_frames,
            state.dropped_frames,
            state.closed_sockets,
            state.protocol_errors,
            state.created_events,
            state.sent_frame_events,
            state.received_frame_events,
            state.closed_events,
            state.frame_error_events,
            frames,
        );
        if clear {
            state.frames.clear();
            state.captured_payload_chars = 0;
        }
        snapshot
    };
    let returned_frames = frames
        .into_iter()
        .map(|frame| frame_json(frame, include_text_payloads, max_payload_chars))
        .collect::<Vec<_>>();
    let returned_count = returned_frames.len();
    let mut response = json!({
        "success": true,
        "source": "managed_loopback_cdp",
        "active": active,
        "status": status,
        "warning": warning,
        "started_at_ms": started_at_ms,
        "page_url": page_url,
        "socket_count": socket_count,
        "open_socket_count": open_socket_count,
        "closed_socket_count": closed_sockets,
        "total_frame_count": total_frames,
        "returned_count": returned_count,
        "dropped_frame_count": dropped_frames,
        "protocol_error_count": protocol_errors,
        "event_counts": {
            "created": created_events,
            "sent_frames": sent_frame_events,
            "received_frames": received_frame_events,
            "closed": closed_events,
            "frame_errors": frame_error_events,
        },
        "clear_requested": clear,
        "clear_applied": clear,
        "text_payloads_included": include_text_payloads,
        "binary_payloads_included": false,
        "payload_redaction_policy": "sensitive_fields_assignments_bearer_and_likely_tokens",
        "filters": {
            "request_id": request_id,
            "direction": direction,
        },
        "frames": returned_frames,
        "_summary_text": format!(
            "Observed {returned_count} bounded WebSocket frame(s) from {socket_count} socket(s). Binary payloads were omitted{}.",
            if include_text_payloads {
                " and explicitly requested text payloads were sanitized"
            } else {
                " and text payloads were omitted"
            }
        ),
    });
    enrich_response_with_page_metadata(&ctx, conversation_key.as_str(), &mut response).await;
    super::actions_network::sanitize_response_page_url(&mut response);
    Ok(response)
}

pub(super) async fn browser_websocket_stop_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let conversation_key = super::super::context::conversation_key(conversation_id);
    let observer_key = observer_key(&ctx, conversation_key.as_str());
    let handle = WEBSOCKET_OBSERVERS
        .lock()
        .remove(observer_key.as_str())
        .ok_or_else(|| {
            "WebSocket observation is not active for this browser session".to_string()
        })?;
    let (total_frames, dropped_frames, socket_count) = {
        let mut state = handle.state.lock();
        state.finish("stopped", None);
        (
            state.total_frames,
            state.dropped_frames,
            state.sockets.len(),
        )
    };
    handle.abort.abort();
    Ok(json!({
        "success": true,
        "status": "stopped",
        "socket_count": socket_count,
        "total_frame_count": total_frames,
        "dropped_frame_count": dropped_frames,
        "_summary_text": format!(
            "Stopped bounded WebSocket observation after capturing {total_frames} frame(s) from {socket_count} socket(s)."
        ),
    }))
}

pub(super) fn stop_browser_websocket_observer(ctx: &BoundContext, conversation_key: &str) {
    let observer_key = observer_key(ctx, conversation_key);
    let Some(handle) = WEBSOCKET_OBSERVERS.lock().remove(observer_key.as_str()) else {
        return;
    };
    handle.state.lock().finish("stopped", None);
    handle.abort.abort();
}

fn observer_key(ctx: &BoundContext, conversation_key: &str) -> String {
    let workspace = ctx.workspace_dir.to_string_lossy();
    format!(
        "{}:{}:{}:{}",
        workspace.len(),
        workspace,
        conversation_key.len(),
        conversation_key
    )
}

async fn observe_websocket_cdp_stream<S>(
    stream: &mut tokio_tungstenite::WebSocketStream<S>,
    session_id: &str,
    state: Arc<Mutex<WebSocketObserverState>>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let deadline = tokio::time::sleep(MAX_WEBSOCKET_OBSERVER_LIFETIME);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => {
                state.lock().finish(
                    "expired",
                    Some("WebSocket observation reached the 30 minute safety limit".to_string()),
                );
                break;
            }
            message = stream.next() => {
                let Some(message) = message else {
                    state.lock().finish("closed", None);
                    break;
                };
                let message = match message {
                    Ok(message) => message,
                    Err(_) => {
                        state.lock().finish(
                            "error",
                            Some("managed browser CDP observer closed after a bounded read error".to_string()),
                        );
                        break;
                    }
                };
                match message {
                    Message::Text(text) => {
                        if text.len() > MAX_WEBSOCKET_CDP_MESSAGE_BYTES {
                            state.lock().finish(
                                "error",
                                Some("managed browser CDP observer message exceeded the safety limit".to_string()),
                            );
                            break;
                        }
                        match serde_json::from_str::<Value>(text.as_ref()) {
                            Ok(event) => record_cdp_websocket_event(&mut state.lock(), session_id, &event),
                            Err(_) => {
                                let mut state = state.lock();
                                state.protocol_errors = state.protocol_errors.saturating_add(1);
                                if state.protocol_errors > 10 {
                                    state.finish(
                                        "error",
                                        Some("managed browser CDP observer emitted malformed JSON".to_string()),
                                    );
                                    break;
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        if stream.send(Message::Pong(payload)).await.is_err() {
                            state.lock().finish("closed", None);
                            break;
                        }
                    }
                    Message::Close(_) => {
                        state.lock().finish("closed", None);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn record_cdp_websocket_event(state: &mut WebSocketObserverState, session_id: &str, event: &Value) {
    if event.get("sessionId").and_then(Value::as_str) != Some(session_id) {
        return;
    }
    let method = event
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = event.get("params").unwrap_or(&Value::Null);
    match method {
        "Network.webSocketCreated" => {
            state.created_events = state.created_events.saturating_add(1);
            let Some(request_id) = params
                .get("requestId")
                .and_then(Value::as_str)
                .and_then(|value| normalize_request_id(value).ok())
            else {
                state.protocol_errors = state.protocol_errors.saturating_add(1);
                return;
            };
            if state.sockets.len() >= MAX_CAPTURED_WEBSOCKET_SOCKETS
                && !state.sockets.contains_key(request_id.as_str())
            {
                return;
            }
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .and_then(sanitize_network_url)
                .map(|value| value.chars().take(MAX_WEBSOCKET_URL_CHARS).collect());
            state
                .sockets
                .insert(request_id, ObservedWebSocket { url, closed: false });
        }
        "Network.webSocketClosed" => {
            state.closed_events = state.closed_events.saturating_add(1);
            let Some(request_id) = params
                .get("requestId")
                .and_then(Value::as_str)
                .and_then(|value| normalize_request_id(value).ok())
            else {
                return;
            };
            if let Some(socket) = state.sockets.get_mut(request_id.as_str()) {
                if !socket.closed {
                    socket.closed = true;
                    state.closed_sockets = state.closed_sockets.saturating_add(1);
                }
            }
        }
        "Network.webSocketFrameSent" | "Network.webSocketFrameReceived" => {
            if method.ends_with("Sent") {
                state.sent_frame_events = state.sent_frame_events.saturating_add(1);
            } else {
                state.received_frame_events = state.received_frame_events.saturating_add(1);
            }
            let Some(request_id) = params
                .get("requestId")
                .and_then(Value::as_str)
                .and_then(|value| normalize_request_id(value).ok())
            else {
                state.protocol_errors = state.protocol_errors.saturating_add(1);
                return;
            };
            if !state.sockets.contains_key(request_id.as_str()) {
                if state.sockets.len() >= MAX_CAPTURED_WEBSOCKET_SOCKETS {
                    state.dropped_frames = state.dropped_frames.saturating_add(1);
                    return;
                }
                state.sockets.insert(
                    request_id.clone(),
                    ObservedWebSocket {
                        url: None,
                        closed: false,
                    },
                );
            }
            let frame = params.get("response").unwrap_or(&Value::Null);
            let opcode = frame
                .get("opcode")
                .and_then(Value::as_u64)
                .filter(|value| *value <= 255)
                .unwrap_or(0);
            let payload = frame
                .get("payloadData")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let (sanitized_text, payload_truncated, redaction_applied) =
                sanitized_websocket_text_payload(opcode, payload);
            state.push_frame(CapturedWebSocketFrame {
                sequence: 0,
                request_id: request_id.clone(),
                url: state
                    .sockets
                    .get(request_id.as_str())
                    .and_then(|socket| socket.url.clone()),
                direction: if method.ends_with("Sent") {
                    "sent"
                } else {
                    "received"
                },
                opcode,
                payload_bytes: if opcode == 2 {
                    payload.len().saturating_mul(3) / 4
                } else {
                    payload.len()
                },
                text_payload_available: opcode == 1 && !payload.is_empty(),
                sanitized_text,
                payload_truncated,
                redaction_applied,
                cdp_timestamp_ms: bounded_cdp_timestamp_ms(params.get("timestamp")),
            });
        }
        "Network.webSocketFrameError" => {
            state.frame_error_events = state.frame_error_events.saturating_add(1);
            state.protocol_errors = state.protocol_errors.saturating_add(1);
        }
        _ => {}
    }
}

fn sanitized_websocket_text_payload(opcode: u64, payload: &str) -> (Option<String>, bool, bool) {
    if opcode != 1 || payload.is_empty() {
        return (None, false, false);
    }
    let original_chars = payload.chars().count();
    let (mut sanitized, mut redacted) = if let Some(url) = sanitize_network_url(payload) {
        let redacted = url != payload;
        (url, redacted)
    } else {
        sanitize_body_text(payload, "")
    };
    if LIKELY_SECRET_TOKEN_RE.is_match(sanitized.as_str()) {
        sanitized = LIKELY_SECRET_TOKEN_RE
            .replace_all(sanitized.as_str(), "[REDACTED]")
            .into_owned();
        redacted = true;
    }
    sanitized = sanitized
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS)
        .collect();
    (
        Some(sanitized),
        original_chars > MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS,
        redacted,
    )
}

fn frame_json(
    frame: CapturedWebSocketFrame,
    include_text_payloads: bool,
    max_payload_chars: usize,
) -> Value {
    let text = include_text_payloads
        .then_some(frame.sanitized_text.as_deref())
        .flatten()
        .map(|value| value.chars().take(max_payload_chars).collect::<String>());
    let response_truncated = frame
        .sanitized_text
        .as_deref()
        .is_some_and(|value| value.chars().count() > max_payload_chars);
    json!({
        "sequence": frame.sequence,
        "request_id": frame.request_id,
        "url": frame.url,
        "direction": frame.direction,
        "opcode": frame.opcode,
        "frame_type": websocket_frame_type(frame.opcode),
        "payload_bytes": frame.payload_bytes,
        "text_payload_available": frame.text_payload_available,
        "text_payload": text,
        "payload_truncated": frame.payload_truncated || response_truncated,
        "redaction_applied": frame.redaction_applied,
        "cdp_timestamp_ms": frame.cdp_timestamp_ms,
    })
}

fn websocket_frame_type(opcode: u64) -> &'static str {
    match opcode {
        1 => "text",
        2 => "binary",
        8 => "close",
        9 => "ping",
        10 => "pong",
        _ => "other",
    }
}

fn websocket_page_url(response: &Value) -> Result<String, String> {
    let data = browser_result_data(response);
    let raw = data
        .get("result")
        .cloned()
        .ok_or_else(|| "browser page identity is malformed".to_string())?;
    let parsed = parse_browser_command_eval_payload(raw);
    let url = parsed
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= MAX_WEBSOCKET_URL_CHARS
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "browser page identity is malformed".to_string())?;
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("chrome://")
        || lower.starts_with("chrome-extension://")
        || lower.starts_with("devtools://")
    {
        return Err("browser internal pages cannot be observed".to_string());
    }
    Ok(url.to_string())
}

fn normalize_direction(value: Option<String>) -> Result<Option<&'static str>, String> {
    value
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "sent" => Ok("sent"),
            "received" => Ok("received"),
            _ => Err("WebSocket frame direction must be sent or received".to_string()),
        })
        .transpose()
}

fn bounded_cdp_timestamp_ms(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 10_000_000_000.0)
        .map(|value| (value * 1_000.0).round() as u64)
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        frame_json, record_cdp_websocket_event, sanitized_websocket_text_payload,
        WebSocketObserverState, MAX_CAPTURED_WEBSOCKET_FRAMES, MAX_CAPTURED_WEBSOCKET_SOCKETS,
    };

    #[test]
    fn websocket_frames_redact_urls_json_credentials_and_likely_tokens() {
        let mut state = WebSocketObserverState::new(None);
        record_cdp_websocket_event(
            &mut state,
            "session-1",
            &json!({
                "sessionId":"session-1",
                "method":"Network.webSocketCreated",
                "params":{"requestId":"42.1","url":"wss://example.com/socket?token=secret"}
            }),
        );
        record_cdp_websocket_event(
            &mut state,
            "session-1",
            &json!({
                "sessionId":"session-1",
                "method":"Network.webSocketFrameReceived",
                "params":{
                    "requestId":"42.1",
                    "timestamp":123.5,
                    "response":{"opcode":1,"payloadData":"{\"type\":\"message\",\"password\":\"secret\",\"tokenish\":\"abcdefghijklmnopqrstuvwxyz0123456789\"}"}
                }
            }),
        );
        let frame = frame_json(state.frames.front().cloned().unwrap(), true, 4096);
        assert_eq!(
            frame["url"],
            "wss://example.com/socket?token=%5BREDACTED%5D"
        );
        let text = frame["text_payload"].as_str().unwrap();
        assert!(!text.contains("secret"));
        assert!(!text.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
        assert!(text.contains("[REDACTED]"));
        assert_eq!(frame["direction"], "received");
        assert_eq!(frame["frame_type"], "text");
    }

    #[test]
    fn websocket_capture_is_bounded_and_binary_payloads_are_omitted() {
        let mut state = WebSocketObserverState::new(None);
        for index in 0..=MAX_CAPTURED_WEBSOCKET_FRAMES {
            record_cdp_websocket_event(
                &mut state,
                "session-1",
                &json!({
                    "sessionId":"session-1",
                    "method":"Network.webSocketFrameSent",
                    "params":{
                        "requestId":"42.1",
                        "response":{"opcode":2,"payloadData":format!("binary-{index}")}
                    }
                }),
            );
        }
        assert_eq!(state.frames.len(), MAX_CAPTURED_WEBSOCKET_FRAMES);
        assert_eq!(state.dropped_frames, 1);
        assert!(state
            .frames
            .iter()
            .all(|frame| frame.sanitized_text.is_none()));
    }

    #[test]
    fn websocket_plain_tokens_are_redacted_before_storage() {
        let (text, _, redacted) =
            sanitized_websocket_text_payload(1, "token abcdefghijklmnopqrstuvwxyz0123456789");
        assert!(redacted);
        assert_eq!(text.as_deref(), Some("token [REDACTED]"));
    }

    #[test]
    fn websocket_capture_rejects_frames_beyond_the_socket_identity_limit() {
        let mut state = WebSocketObserverState::new(None);
        for index in 0..=MAX_CAPTURED_WEBSOCKET_SOCKETS {
            record_cdp_websocket_event(
                &mut state,
                "session-1",
                &json!({
                    "sessionId":"session-1",
                    "method":"Network.webSocketFrameReceived",
                    "params":{
                        "requestId":format!("42.{index}"),
                        "response":{"opcode":1,"payloadData":"safe"}
                    }
                }),
            );
        }
        assert_eq!(state.sockets.len(), MAX_CAPTURED_WEBSOCKET_SOCKETS);
        assert_eq!(state.frames.len(), MAX_CAPTURED_WEBSOCKET_SOCKETS);
        assert_eq!(state.dropped_frames, 1);
    }
}
