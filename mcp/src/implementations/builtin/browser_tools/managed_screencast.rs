// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};

use super::managed_preview::{
    active_page_target_id, read_cdp_result, send_cdp_command, validate_loopback_cdp_endpoint,
    CDP_CONNECT_TIMEOUT, CDP_RESPONSE_TIMEOUT,
};
use super::{BoundContext, BrowserSessionPreviewFrame};
use crate::browser_command_support::{
    browser_command_succeeded, parse_browser_command_eval_payload,
};
use crate::browser_runtime::{
    run_browser_command as runtime_run_browser_command, BrowserRuntimeSession,
};

const MAX_ACTIVE_SCREENCASTS: usize = 64;
const MAX_SCREENCAST_LIFETIME: Duration = Duration::from_secs(30 * 60);
const SCREENCAST_IDLE_RETENTION: Duration = Duration::from_secs(31 * 60);
const SCREENCAST_FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(3);
const SCREENCAST_NEXT_FRAME_TIMEOUT: Duration = Duration::from_millis(650);
const MAX_SCREENCAST_CDP_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SCREENCAST_JPEG_BYTES: usize = 5 * 1024 * 1024;
const MAX_SCREENCAST_ENCODED_BYTES: usize = MAX_SCREENCAST_JPEG_BYTES.div_ceil(3) * 4 + 4;
const MAX_SCREENCAST_DIMENSION: u32 = 4_096;
const MAX_SCREENCAST_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_SCREENCAST_PROTOCOL_ERRORS: u64 = 10;
const SCREENCAST_QUALITY: u64 = 70;
const SCREENCAST_MAX_WIDTH: u64 = 1_920;
const SCREENCAST_MAX_HEIGHT: u64 = 1_080;

static SCREENCASTS: Lazy<Mutex<HashMap<String, ScreencastHandle>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static SCREENCAST_START_LOCK: Lazy<tokio::sync::Mutex<()>> =
    Lazy::new(|| tokio::sync::Mutex::new(()));
static SCREENCAST_RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("chatos-browser-screencast")
        .build()
        .expect("build browser screencast runtime")
});

#[derive(Debug, Clone)]
struct CapturedScreencastFrame {
    bytes: Vec<u8>,
    sequence: u64,
    width: u32,
    height: u32,
    page_scale_factor: f64,
    offset_top: f64,
    scroll_offset_x: f64,
    scroll_offset_y: f64,
    timestamp: u64,
}

impl CapturedScreencastFrame {
    fn into_preview(self, warning: Option<String>) -> BrowserSessionPreviewFrame {
        BrowserSessionPreviewFrame {
            bytes: self.bytes,
            media_type: "image/jpeg",
            sequence: self.sequence,
            width: self.width,
            height: self.height,
            page_scale_factor: self.page_scale_factor,
            offset_top: self.offset_top,
            scroll_offset_x: self.scroll_offset_x,
            scroll_offset_y: self.scroll_offset_y,
            crop_offset_y: 0.0,
            timestamp: self.timestamp,
            source: "screencast",
            warning,
        }
    }
}

#[derive(Debug)]
struct ScreencastState {
    active: bool,
    status: &'static str,
    warning: Option<String>,
    last_accessed_at_ms: u64,
    latest_frame: Option<CapturedScreencastFrame>,
    protocol_errors: u64,
    next_sequence: u64,
}

impl ScreencastState {
    fn new() -> Self {
        let now = unix_timestamp_ms();
        Self {
            active: true,
            status: "starting",
            warning: None,
            last_accessed_at_ms: now,
            latest_frame: None,
            protocol_errors: 0,
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

    fn record_frame(&mut self, mut frame: CapturedScreencastFrame) {
        frame.sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.latest_frame = Some(frame);
        self.status = "active";
    }
}

#[derive(Clone)]
struct ScreencastHandle {
    state: Arc<Mutex<ScreencastState>>,
    notify: Arc<tokio::sync::Notify>,
    abort: tokio::task::AbortHandle,
}

pub(super) async fn capture_browser_screencast_frame(
    ctx: &BoundContext,
    conversation_key: &str,
    after_sequence: u64,
) -> Result<Option<BrowserSessionPreviewFrame>, String> {
    let key = screencast_key(ctx, conversation_key);
    let handle = match existing_handle(key.as_str()) {
        Some(handle) => handle,
        None => start_screencast(ctx, conversation_key, key.as_str()).await?,
    };
    wait_for_frame(handle, after_sequence).await
}

pub(super) fn stop_browser_screencast(ctx: &BoundContext, conversation_key: &str) -> bool {
    let key = screencast_key(ctx, conversation_key);
    let Some(handle) = SCREENCASTS.lock().remove(key.as_str()) else {
        return false;
    };
    handle.state.lock().finish("stopped", None);
    handle.notify.notify_waiters();
    handle.abort.abort();
    true
}

fn existing_handle(key: &str) -> Option<ScreencastHandle> {
    let now = unix_timestamp_ms();
    let retention_ms = SCREENCAST_IDLE_RETENTION.as_millis() as u64;
    let mut streams = SCREENCASTS.lock();
    let stale_keys = streams
        .iter()
        .filter(|(_, handle)| {
            now.saturating_sub(handle.state.lock().last_accessed_at_ms) > retention_ms
        })
        .map(|(stream_key, _)| stream_key.clone())
        .collect::<Vec<_>>();
    for stale_key in stale_keys {
        if let Some(stale) = streams.remove(stale_key.as_str()) {
            stale.state.lock().finish("expired", None);
            stale.notify.notify_waiters();
            stale.abort.abort();
        }
    }
    let handle = streams.get(key)?.clone();
    handle.state.lock().last_accessed_at_ms = now;
    Some(handle)
}

async fn start_screencast(
    ctx: &BoundContext,
    conversation_key: &str,
    key: &str,
) -> Result<ScreencastHandle, String> {
    let _start_guard = SCREENCAST_START_LOCK.lock().await;
    if let Some(handle) = existing_handle(key) {
        return Ok(handle);
    }
    {
        let streams = SCREENCASTS.lock();
        let active_count = streams
            .values()
            .filter(|handle| handle.state.lock().active && !handle.abort.is_finished())
            .count();
        if active_count >= MAX_ACTIVE_SCREENCASTS {
            return Err("browser screencast capacity is temporarily exhausted".to_string());
        }
    }

    let session = ctx
        .sessions
        .lock()
        .get(conversation_key)
        .cloned()
        .ok_or_else(|| "managed browser session is unavailable for screencast".to_string())?;
    if session.cdp_url.is_some() {
        return Err("screencast is limited to managed loopback browser sessions".to_string());
    }
    let page_url = current_page_url(ctx, &session).await?;
    let endpoint = managed_cdp_endpoint(ctx, &session).await?;
    let state = Arc::new(Mutex::new(ScreencastState::new()));
    let notify = Arc::new(tokio::sync::Notify::new());
    let task_state = Arc::clone(&state);
    let task_notify = Arc::clone(&notify);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let task = SCREENCAST_RUNTIME.spawn(run_screencast_task(
        endpoint,
        page_url,
        task_state,
        task_notify,
        ready_tx,
    ));
    let handle = ScreencastHandle {
        state,
        notify,
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
            return Err("managed browser screencast setup closed".to_string());
        }
        Err(_) => {
            handle.abort.abort();
            return Err("managed browser screencast setup timed out".to_string());
        }
    }
    let mut streams = SCREENCASTS.lock();
    if streams.contains_key(key) {
        handle.abort.abort();
        return streams
            .get(key)
            .cloned()
            .ok_or_else(|| "browser screencast changed concurrently".to_string());
    }
    streams.insert(key.to_string(), handle.clone());
    Ok(handle)
}

async fn current_page_url(
    ctx: &BoundContext,
    session: &BrowserRuntimeSession,
) -> Result<String, String> {
    let response = runtime_run_browser_command(
        ctx.workspace_dir.as_path(),
        session,
        "eval",
        vec!["JSON.stringify({url:window.location.href})".to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !browser_command_succeeded(&response) {
        return Err("browser page identity is unavailable for screencast".to_string());
    }
    let raw = response
        .pointer("/data/result")
        .cloned()
        .ok_or_else(|| "browser page identity is malformed".to_string())?;
    parse_browser_command_eval_payload(raw)
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 8_192
                && !value.chars().any(|character| character.is_control())
        })
        .map(ToOwned::to_owned)
        .ok_or_else(|| "browser page identity is malformed".to_string())
}

async fn managed_cdp_endpoint(
    ctx: &BoundContext,
    session: &BrowserRuntimeSession,
) -> Result<String, String> {
    let response = runtime_run_browser_command(
        ctx.workspace_dir.as_path(),
        session,
        "get",
        vec!["cdp-url".to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !browser_command_succeeded(&response) {
        return Err("managed browser CDP endpoint is unavailable".to_string());
    }
    let endpoint = response
        .pointer("/data/cdpUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed browser CDP endpoint is malformed".to_string())?;
    Ok(validate_loopback_cdp_endpoint(endpoint)?.to_string())
}

async fn run_screencast_task(
    endpoint: String,
    page_url: String,
    state: Arc<Mutex<ScreencastState>>,
    notify: Arc<tokio::sync::Notify>,
    ready: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    let setup = setup_screencast(endpoint.as_str(), page_url.as_str()).await;
    match setup {
        Ok((mut stream, session_id)) => {
            if ready.send(Ok(())).is_err() {
                state.lock().finish("stopped", None);
                return;
            }
            observe_screencast_stream(&mut stream, session_id.as_str(), state, notify).await;
        }
        Err(error) => {
            state.lock().finish("error", Some(error.clone()));
            notify.notify_waiters();
            let _ = ready.send(Err(error));
        }
    }
}

async fn setup_screencast(
    endpoint: &str,
    page_url: &str,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        String,
    ),
    String,
> {
    let config = WebSocketConfig::default()
        .read_buffer_size(64 * 1024)
        .write_buffer_size(8 * 1024)
        .max_write_buffer_size(64 * 1024)
        .max_message_size(Some(MAX_SCREENCAST_CDP_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_SCREENCAST_CDP_MESSAGE_BYTES));
    let (mut stream, _) = tokio::time::timeout(
        CDP_CONNECT_TIMEOUT,
        connect_async_with_config(endpoint, Some(config), true),
    )
    .await
    .map_err(|_| "managed browser screencast connection timed out".to_string())?
    .map_err(|_| "managed browser screencast connection failed".to_string())?;
    let session_id = tokio::time::timeout(CDP_RESPONSE_TIMEOUT, async {
        send_cdp_command(&mut stream, 1, "Target.getTargets", json!({}), None).await?;
        let targets = read_cdp_result(&mut stream, 1).await?;
        let target_id = active_page_target_id(&targets, page_url)?;
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
            .ok_or_else(|| "managed browser screencast attachment is malformed".to_string())?
            .to_string();
        send_cdp_command(
            &mut stream,
            3,
            "Page.enable",
            json!({}),
            Some(session_id.as_str()),
        )
        .await?;
        read_cdp_result(&mut stream, 3).await?;
        send_cdp_command(
            &mut stream,
            4,
            "Page.startScreencast",
            json!({
                "format": "jpeg",
                "quality": SCREENCAST_QUALITY,
                "maxWidth": SCREENCAST_MAX_WIDTH,
                "maxHeight": SCREENCAST_MAX_HEIGHT,
                "everyNthFrame": 1,
            }),
            Some(session_id.as_str()),
        )
        .await?;
        Ok::<_, String>(session_id)
    })
    .await
    .map_err(|_| "managed browser screencast setup timed out".to_string())??;
    Ok((stream, session_id))
}

async fn observe_screencast_stream<S>(
    stream: &mut tokio_tungstenite::WebSocketStream<S>,
    session_id: &str,
    state: Arc<Mutex<ScreencastState>>,
    notify: Arc<tokio::sync::Notify>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let deadline = tokio::time::sleep(MAX_SCREENCAST_LIFETIME);
    tokio::pin!(deadline);
    let mut next_command_id = 10_u64;
    loop {
        tokio::select! {
            _ = &mut deadline => {
                state.lock().finish(
                    "expired",
                    Some("browser screencast reached the 30 minute safety limit".to_string()),
                );
                notify.notify_waiters();
                break;
            }
            message = stream.next() => {
                let Some(message) = message else {
                    state.lock().finish("closed", None);
                    notify.notify_waiters();
                    break;
                };
                let message = match message {
                    Ok(message) => message,
                    Err(_) => {
                        state.lock().finish(
                            "error",
                            Some("managed browser screencast closed after a bounded read error".to_string()),
                        );
                        notify.notify_waiters();
                        break;
                    }
                };
                match message {
                    Message::Text(text) => {
                        if text.len() > MAX_SCREENCAST_CDP_MESSAGE_BYTES {
                            state.lock().finish(
                                "error",
                                Some("managed browser screencast message exceeded the safety limit".to_string()),
                            );
                            notify.notify_waiters();
                            break;
                        }
                        let event = match serde_json::from_str::<Value>(text.as_ref()) {
                            Ok(event) => event,
                            Err(_) => {
                                if record_protocol_error(&state, "managed browser screencast emitted malformed JSON") {
                                    notify.notify_waiters();
                                    break;
                                }
                                continue;
                            }
                        };
                        if event.get("method").and_then(Value::as_str) != Some("Page.screencastFrame") {
                            continue;
                        }
                        if event.get("sessionId").and_then(Value::as_str) != Some(session_id) {
                            if record_protocol_error(&state, "managed browser screencast emitted a mismatched session") {
                                notify.notify_waiters();
                                break;
                            }
                            continue;
                        }
                        let frame_session_id = event
                            .pointer("/params/sessionId")
                            .and_then(Value::as_u64)
                            .filter(|value| *value <= u64::from(u32::MAX));
                        let Some(frame_session_id) = frame_session_id else {
                            if record_protocol_error(&state, "managed browser screencast frame is missing its acknowledgement ID") {
                                notify.notify_waiters();
                                break;
                            }
                            continue;
                        };
                        if send_cdp_command(
                            stream,
                            next_command_id,
                            "Page.screencastFrameAck",
                            json!({"sessionId": frame_session_id}),
                            Some(session_id),
                        )
                        .await
                        .is_err()
                        {
                            state.lock().finish(
                                "error",
                                Some("managed browser screencast frame acknowledgement failed".to_string()),
                            );
                            notify.notify_waiters();
                            break;
                        }
                        next_command_id = next_command_id.saturating_add(1);
                        match parse_screencast_frame_event(&event) {
                            Ok(frame) => {
                                state.lock().record_frame(frame);
                                notify.notify_waiters();
                            }
                            Err(error) => {
                                if record_protocol_error(&state, error.as_str()) {
                                    notify.notify_waiters();
                                    break;
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        if stream.send(Message::Pong(payload)).await.is_err() {
                            state.lock().finish("closed", None);
                            notify.notify_waiters();
                            break;
                        }
                    }
                    Message::Close(_) => {
                        state.lock().finish("closed", None);
                        notify.notify_waiters();
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    let _ = send_cdp_command(
        stream,
        next_command_id,
        "Page.stopScreencast",
        json!({}),
        Some(session_id),
    )
    .await;
}

fn record_protocol_error(state: &Arc<Mutex<ScreencastState>>, warning: &str) -> bool {
    let mut state = state.lock();
    state.protocol_errors = state.protocol_errors.saturating_add(1);
    if state.protocol_errors > MAX_SCREENCAST_PROTOCOL_ERRORS {
        state.finish("error", Some(warning.to_string()));
        true
    } else {
        false
    }
}

async fn wait_for_frame(
    handle: ScreencastHandle,
    after_sequence: u64,
) -> Result<Option<BrowserSessionPreviewFrame>, String> {
    let timeout = if after_sequence == 0 {
        SCREENCAST_FIRST_FRAME_TIMEOUT
    } else {
        SCREENCAST_NEXT_FRAME_TIMEOUT
    };
    let wait = async {
        loop {
            let snapshot = {
                let mut state = handle.state.lock();
                state.last_accessed_at_ms = unix_timestamp_ms();
                if let Some(frame) = state
                    .latest_frame
                    .as_ref()
                    .filter(|frame| frame.sequence > after_sequence)
                    .cloned()
                {
                    return Ok(Some(frame.into_preview(state.warning.clone())));
                }
                if !state.active || handle.abort.is_finished() {
                    let warning = state.warning.clone().unwrap_or_else(|| {
                        format!("managed browser screencast is {}", state.status)
                    });
                    return Err(warning);
                }
                handle.notify.notified()
            };
            snapshot.await;
        }
    };
    match tokio::time::timeout(timeout, wait).await {
        Ok(result) => result,
        Err(_) if after_sequence > 0 => Ok(None),
        Err(_) => Err("managed browser screencast did not produce its first frame".to_string()),
    }
}

fn parse_screencast_frame_event(event: &Value) -> Result<CapturedScreencastFrame, String> {
    let encoded = event
        .pointer("/params/data")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed browser screencast frame is missing JPEG data".to_string())?;
    if encoded.len() > MAX_SCREENCAST_ENCODED_BYTES {
        return Err("managed browser screencast frame exceeded the encoded limit".to_string());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "managed browser screencast frame contains invalid base64".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_SCREENCAST_JPEG_BYTES {
        return Err("managed browser screencast frame exceeded the decoded limit".to_string());
    }
    let (width, height) = jpeg_dimensions(bytes.as_slice())?;
    if width == 0
        || height == 0
        || width > MAX_SCREENCAST_DIMENSION
        || height > MAX_SCREENCAST_DIMENSION
        || u64::from(width).saturating_mul(u64::from(height)) > MAX_SCREENCAST_PIXELS
    {
        return Err("managed browser screencast frame exceeded the pixel limit".to_string());
    }
    let metadata = event
        .pointer("/params/metadata")
        .and_then(Value::as_object)
        .ok_or_else(|| "managed browser screencast frame metadata is malformed".to_string())?;
    Ok(CapturedScreencastFrame {
        bytes,
        sequence: 0,
        width,
        height,
        page_scale_factor: bounded_number(metadata.get("pageScaleFactor"), 0.01, 100.0)?,
        offset_top: bounded_number(metadata.get("offsetTop"), -1_000_000_000.0, 1_000_000_000.0)?,
        scroll_offset_x: bounded_number(metadata.get("scrollOffsetX"), 0.0, 1_000_000_000.0)?,
        scroll_offset_y: bounded_number(metadata.get("scrollOffsetY"), 0.0, 1_000_000_000.0)?,
        timestamp: unix_timestamp_ms(),
    })
}

fn bounded_number(value: Option<&Value>, minimum: f64, maximum: f64) -> Result<f64, String> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number >= minimum && *number <= maximum)
        .ok_or_else(|| "managed browser screencast frame metadata is out of bounds".to_string())
}

fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() < 4 || !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err("managed browser screencast frame is not a complete JPEG".to_string());
    }
    let mut cursor = 2usize;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor] != 0xff {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let marker = bytes[cursor];
        cursor += 1;
        if matches!(marker, 0x01 | 0xd0..=0xd9) {
            continue;
        }
        if cursor + 2 > bytes.len() {
            break;
        }
        let segment_length = usize::from(u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]));
        if segment_length < 2 || cursor.saturating_add(segment_length) > bytes.len() {
            return Err("managed browser screencast JPEG contains an invalid segment".to_string());
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if segment_length < 7 {
                return Err("managed browser screencast JPEG frame header is invalid".to_string());
            }
            let height = u32::from(u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]));
            return Ok((width, height));
        }
        if marker == 0xda {
            break;
        }
        cursor += segment_length;
    }
    Err("managed browser screencast JPEG is missing dimensions".to_string())
}

fn screencast_key(ctx: &BoundContext, conversation_key: &str) -> String {
    let workspace = ctx.workspace_dir.to_string_lossy();
    format!(
        "{}:{}:{}:{}",
        workspace.len(),
        workspace,
        conversation_key.len(),
        conversation_key
    )
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
    use std::sync::Arc;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use parking_lot::Mutex;
    use serde_json::json;

    use super::{
        jpeg_dimensions, parse_screencast_frame_event, wait_for_frame, CapturedScreencastFrame,
        ScreencastHandle, ScreencastState,
    };

    fn tiny_jpeg(width: u16, height: u16) -> Vec<u8> {
        vec![
            0xff,
            0xd8,
            0xff,
            0xc0,
            0x00,
            0x0b,
            0x08,
            (height >> 8) as u8,
            height as u8,
            (width >> 8) as u8,
            width as u8,
            0x01,
            0x01,
            0x11,
            0x00,
            0xff,
            0xd9,
        ]
    }

    #[test]
    fn screencast_frames_require_bounded_complete_jpeg_and_metadata() {
        let bytes = tiny_jpeg(1_280, 720);
        assert_eq!(jpeg_dimensions(bytes.as_slice()).unwrap(), (1_280, 720));
        let frame = parse_screencast_frame_event(&json!({
            "params": {
                "data": STANDARD.encode(bytes),
                "metadata": {
                    "pageScaleFactor": 1.0,
                    "offsetTop": 0.0,
                    "scrollOffsetX": 0.0,
                    "scrollOffsetY": 12.0
                },
                "sessionId": 7
            }
        }))
        .expect("bounded screencast frame");
        assert_eq!((frame.width, frame.height), (1_280, 720));
        assert_eq!(frame.scroll_offset_y, 12.0);
    }

    #[test]
    fn screencast_frames_reject_oversized_dimensions_and_invalid_metadata() {
        assert!(parse_screencast_frame_event(&json!({
            "params": {
                "data": STANDARD.encode(tiny_jpeg(4_097, 720)),
                "metadata": {
                    "pageScaleFactor": 1.0,
                    "offsetTop": 0.0,
                    "scrollOffsetX": 0.0,
                    "scrollOffsetY": 0.0
                }
            }
        }))
        .is_err());
        assert!(parse_screencast_frame_event(&json!({
            "params": {
                "data": STANDARD.encode(tiny_jpeg(640, 480)),
                "metadata": {
                    "pageScaleFactor": 0.0,
                    "offsetTop": 0.0,
                    "scrollOffsetX": 0.0,
                    "scrollOffsetY": 0.0
                }
            }
        }))
        .is_err());
    }

    #[tokio::test]
    async fn long_poll_returns_only_a_newer_latest_frame() {
        let state = Arc::new(Mutex::new(ScreencastState::new()));
        state.lock().record_frame(CapturedScreencastFrame {
            bytes: tiny_jpeg(640, 480),
            sequence: 0,
            width: 640,
            height: 480,
            page_scale_factor: 1.0,
            offset_top: 0.0,
            scroll_offset_x: 0.0,
            scroll_offset_y: 0.0,
            timestamp: 1,
        });
        let notify = Arc::new(tokio::sync::Notify::new());
        let task = tokio::spawn(std::future::pending::<()>());
        let handle = ScreencastHandle {
            state,
            notify,
            abort: task.abort_handle(),
        };

        let first = wait_for_frame(handle.clone(), 0)
            .await
            .expect("first frame read")
            .expect("first frame");
        assert_eq!(first.sequence, 1);
        assert!(wait_for_frame(handle, first.sequence)
            .await
            .expect("unchanged frame read")
            .is_none());
        task.abort();
    }
}
