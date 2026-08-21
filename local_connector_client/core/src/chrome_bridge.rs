// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::Notify;
use uuid::Uuid;

pub(crate) const CHROME_EXTENSION_ID: &str = "eebkndlcocijhemeddoifdchmnifcgcm";
pub(crate) const CHROME_EXTENSION_VERSION: &str = "1.3.0";
pub(crate) const CHROME_EXTENSION_ORIGIN: &str =
    "chrome-extension://eebkndlcocijhemeddoifdchmnifcgcm/";
pub(crate) const CHROME_NATIVE_HOST_NAME: &str = "com.chatos.chrome";
pub(crate) const CHROME_NATIVE_PROTOCOL_VERSION: u32 = 1;

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "reserved for the tested Chrome extension command bridge until a runtime provider is connected"
    )
)]
const MAX_PENDING_COMMANDS: usize = 64;
const MAX_BRIDGE_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_EXTENSION_VERSION_BYTES: usize = 64;
const CONNECTION_STALE_AFTER: Duration = Duration::from_secs(35);
const NEXT_COMMAND_WAIT: Duration = Duration::from_secs(15);
#[allow(
    dead_code,
    reason = "retained entry point for the Chrome Native Messaging command integration"
)]
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(12);
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "reserved for the tested Chrome extension command bridge until a runtime provider is connected"
    )
)]
const COMMAND_CANCEL_POLL: Duration = Duration::from_millis(100);
const CANCELLED_REQUEST_TTL: Duration = Duration::from_secs(60);
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "reserved for the tested Chrome extension command bridge until a runtime provider is connected"
    )
)]
const MAX_CANCELLED_REQUESTS: usize = 128;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChromeBridgeStatus {
    pub(crate) connected: bool,
    pub(crate) extension_id: String,
    pub(crate) extension_version: Option<String>,
    pub(crate) extension_compatible: bool,
    pub(crate) connected_at_ms: Option<u64>,
    pub(crate) last_seen_at_ms: Option<u64>,
    pub(crate) claimed_tab_count: usize,
    pub(crate) authorized_origin_count: usize,
    pub(crate) pending_command_count: usize,
}

#[derive(Debug, Clone)]
struct ChromeConnection {
    connection_id: String,
    extension_version: Option<String>,
    connected_at_ms: u64,
    last_seen_at_ms: u64,
    claimed_tab_count: usize,
    authorized_origin_count: usize,
}

#[derive(Debug)]
struct PendingChromeCommand {
    response: Sender<Result<Value, String>>,
}

#[derive(Debug, Default)]
struct ChromeBridgeState {
    connection: Option<ChromeConnection>,
    commands: VecDeque<Value>,
    pending: HashMap<String, PendingChromeCommand>,
    cancelled: HashMap<String, u64>,
}

#[derive(Debug, Default)]
struct ChromeBridge {
    state: Mutex<ChromeBridgeState>,
    notify: Notify,
}

static CHROME_BRIDGE: OnceLock<ChromeBridge> = OnceLock::new();

fn bridge() -> &'static ChromeBridge {
    CHROME_BRIDGE.get_or_init(ChromeBridge::default)
}

pub(crate) fn chrome_bridge_status() -> Result<ChromeBridgeStatus> {
    bridge().status()
}

pub(crate) fn connect_chrome_native_host(
    connection_id: &str,
    origin: &str,
    protocol_version: u32,
) -> Result<ChromeBridgeStatus> {
    bridge().connect(connection_id, origin, protocol_version)
}

pub(crate) fn disconnect_chrome_native_host(connection_id: &str) -> Result<bool> {
    bridge().disconnect(connection_id)
}

pub(crate) fn receive_chrome_native_event(
    connection_id: &str,
    event: Value,
) -> Result<ChromeBridgeStatus> {
    bridge().receive_event(connection_id, event)
}

pub(crate) async fn next_chrome_native_command(connection_id: &str) -> Result<Option<Value>> {
    bridge().next_command(connection_id).await
}

#[allow(
    dead_code,
    reason = "retained entry point for the Chrome Native Messaging command integration"
)]
pub(crate) fn execute_chrome_extension_command(command: &str, arguments: Value) -> Result<Value> {
    bridge().execute_command(command, arguments, DEFAULT_COMMAND_TIMEOUT, None)
}

#[allow(
    dead_code,
    reason = "retained cancellable Chrome command entry point used by browser automation integrations"
)]
pub(crate) fn execute_chrome_extension_command_cancellable(
    command: &str,
    arguments: Value,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    bridge().execute_command(
        command,
        arguments,
        DEFAULT_COMMAND_TIMEOUT,
        action_cancelled,
    )
}

#[allow(
    dead_code,
    reason = "retained timeout-aware Chrome command entry point used by browser automation integrations"
)]
pub(crate) fn execute_chrome_extension_command_cancellable_with_timeout(
    command: &str,
    arguments: Value,
    timeout: Duration,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if timeout.is_zero() || timeout > Duration::from_secs(60) {
        bail!("Chrome extension command timeout must be between 1 ms and 60 seconds");
    }
    bridge().execute_command(command, arguments, timeout, action_cancelled)
}

impl ChromeBridge {
    fn connect(
        &self,
        connection_id: &str,
        origin: &str,
        protocol_version: u32,
    ) -> Result<ChromeBridgeStatus> {
        validate_connection_id(connection_id)?;
        if origin != CHROME_EXTENSION_ORIGIN {
            bail!("Chrome Native Messaging origin is not the bundled ChatOS extension");
        }
        if protocol_version != CHROME_NATIVE_PROTOCOL_VERSION {
            bail!("Chrome Native Messaging protocol version is unsupported");
        }
        let now = timestamp_ms();
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Chrome bridge state is unavailable"))?;
        fail_all_pending(&mut state, "Chrome extension connection was replaced");
        state.connection = Some(ChromeConnection {
            connection_id: connection_id.to_string(),
            extension_version: None,
            connected_at_ms: now,
            last_seen_at_ms: now,
            claimed_tab_count: 0,
            authorized_origin_count: 0,
        });
        Ok(status_from_state(&state, now))
    }

    fn disconnect(&self, connection_id: &str) -> Result<bool> {
        validate_connection_id(connection_id)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Chrome bridge state is unavailable"))?;
        let matches = state
            .connection
            .as_ref()
            .is_some_and(|connection| connection.connection_id == connection_id);
        if matches {
            state.connection = None;
            fail_all_pending(&mut state, "Chrome extension disconnected");
        }
        Ok(matches)
    }

    fn receive_event(&self, connection_id: &str, event: Value) -> Result<ChromeBridgeStatus> {
        validate_connection_id(connection_id)?;
        validate_message_size(&event)?;
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Chrome extension event is missing type"))?;
        let now = timestamp_ms();
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Chrome bridge state is unavailable"))?;
        let connection = exact_connection_mut(&mut state, connection_id)?;
        connection.last_seen_at_ms = now;
        match event_type {
            "hello" => {
                let extension_id = required_event_text(&event, "extension_id", 64)?;
                if extension_id != CHROME_EXTENSION_ID {
                    bail!("Chrome extension identity does not match the bundled extension");
                }
                let version =
                    required_event_text(&event, "extension_version", MAX_EXTENSION_VERSION_BYTES)?;
                connection.extension_version = Some(version.to_string());
            }
            "state" => {
                connection.claimed_tab_count = bounded_event_count(&event, "claimed_tab_count")?;
                connection.authorized_origin_count =
                    bounded_event_count(&event, "authorized_origin_count")?;
            }
            "command_result" => {
                let request_id = required_event_text(&event, "request_id", 128)?.to_string();
                validate_request_id(request_id.as_str())?;
                let ok = event
                    .get("ok")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| anyhow!("Chrome command result is missing ok"))?;
                if state.cancelled.remove(request_id.as_str()).is_some() {
                    cleanup_cancelled_requests(&mut state, now);
                    return Ok(status_from_state(&state, now));
                }
                let pending = state.pending.remove(request_id.as_str()).ok_or_else(|| {
                    anyhow!("Chrome command result does not match a pending request")
                })?;
                let response = if ok {
                    let result = event.get("result").cloned().unwrap_or(Value::Null);
                    validate_message_size(&result)?;
                    Ok(result)
                } else {
                    let error = event
                        .get("error")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty() && value.len() <= 512)
                        .unwrap_or("Chrome extension rejected the command")
                        .to_string();
                    Err(error)
                };
                let _ = pending.response.send(response);
            }
            _ => bail!("Chrome extension event type is unsupported"),
        }
        Ok(status_from_state(&state, now))
    }

    async fn next_command(&self, connection_id: &str) -> Result<Option<Value>> {
        validate_connection_id(connection_id)?;
        let deadline = tokio::time::Instant::now() + NEXT_COMMAND_WAIT;
        loop {
            let notified = self.notify.notified();
            {
                let now = timestamp_ms();
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| anyhow!("Chrome bridge state is unavailable"))?;
                let connection = exact_connection_mut(&mut state, connection_id)?;
                connection.last_seen_at_ms = now;
                if let Some(command) = state.commands.pop_front() {
                    return Ok(Some(command));
                }
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            if tokio::time::timeout(remaining, notified).await.is_err() {
                return Ok(None);
            }
        }
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "the command protocol is production code covered by bridge tests before provider wiring"
        )
    )]
    fn execute_command(
        &self,
        command: &str,
        arguments: Value,
        timeout: Duration,
        action_cancelled: Option<&AtomicBool>,
    ) -> Result<Value> {
        if !matches!(
            command,
            "tabs"
                | "snapshot"
                | "release_tab"
                | "navigate"
                | "click"
                | "type_text"
                | "select_option"
                | "scroll"
                | "history"
                | "activate"
                | "screenshot"
                | "upload_begin"
                | "upload_chunk"
                | "upload_finish"
                | "upload_abort"
                | "download_begin"
                | "download_chunk"
                | "download_finish"
                | "download_abort"
        ) {
            bail!("Chrome extension command is not allowed");
        }
        if !arguments.is_object() {
            bail!("Chrome extension command arguments must be an object");
        }
        validate_message_size(&arguments)?;
        let request_id = Uuid::new_v4().to_string();
        let (response_tx, response_rx) = mpsc::channel();
        {
            let now = timestamp_ms();
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Chrome bridge state is unavailable"))?;
            let connection = state
                .connection
                .as_ref()
                .filter(|connection| connection_is_fresh(connection, now))
                .ok_or_else(|| anyhow!("ChatOS Chrome extension is not connected"))?;
            let extension_version = connection
                .extension_version
                .as_deref()
                .ok_or_else(|| anyhow!("ChatOS Chrome extension handshake is incomplete"))?;
            if !matches!(command, "tabs" | "snapshot" | "release_tab")
                && extension_version != CHROME_EXTENSION_VERSION
            {
                bail!(
                    "ChatOS Chrome extension must be updated to version {CHROME_EXTENSION_VERSION}"
                );
            }
            if state.pending.len() >= MAX_PENDING_COMMANDS
                || state.commands.len() >= MAX_PENDING_COMMANDS
            {
                bail!("Chrome extension command capacity is exhausted");
            }
            state.pending.insert(
                request_id.clone(),
                PendingChromeCommand {
                    response: response_tx,
                },
            );
            state.commands.push_back(json!({
                "type": "command",
                "request_id": request_id,
                "command": command,
                "arguments": arguments,
            }));
        }
        self.notify.notify_waiters();
        let deadline = Instant::now() + timeout;
        loop {
            if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
                self.cancel_request(request_id.as_str())?;
                bail!("Chrome extension command was cancelled");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.cancel_request(request_id.as_str())?;
                bail!("Chrome extension command timed out");
            }
            match response_rx.recv_timeout(remaining.min(COMMAND_CANCEL_POLL)) {
                Ok(Ok(value)) => return Ok(value),
                Ok(Err(error)) => return Err(anyhow!(error)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.cancel_request(request_id.as_str())?;
                    bail!("Chrome extension command response channel closed");
                }
            }
        }
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "the command protocol is production code covered by bridge tests before provider wiring"
        )
    )]
    fn cancel_request(&self, request_id: &str) -> Result<()> {
        let now = timestamp_ms();
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Chrome bridge state is unavailable"))?;
        let removed_pending = state.pending.remove(request_id).is_some();
        state.commands.retain(|command| {
            command.get("request_id").and_then(Value::as_str) != Some(request_id)
        });
        if removed_pending {
            cleanup_cancelled_requests(&mut state, now);
            if state.cancelled.len() >= MAX_CANCELLED_REQUESTS {
                if let Some(oldest) = state
                    .cancelled
                    .iter()
                    .min_by_key(|(_, cancelled_at)| **cancelled_at)
                    .map(|(request_id, _)| request_id.clone())
                {
                    state.cancelled.remove(oldest.as_str());
                }
            }
            state.cancelled.insert(request_id.to_string(), now);
            state.commands.push_front(json!({
                "type": "cancel",
                "request_id": request_id,
            }));
            self.notify.notify_waiters();
        }
        Ok(())
    }

    fn status(&self) -> Result<ChromeBridgeStatus> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Chrome bridge state is unavailable"))?;
        Ok(status_from_state(&state, timestamp_ms()))
    }
}

fn exact_connection_mut<'a>(
    state: &'a mut ChromeBridgeState,
    connection_id: &str,
) -> Result<&'a mut ChromeConnection> {
    state
        .connection
        .as_mut()
        .filter(|connection| connection.connection_id == connection_id)
        .ok_or_else(|| anyhow!("Chrome Native Messaging connection is not active"))
}

fn fail_all_pending(state: &mut ChromeBridgeState, message: &str) {
    state.commands.clear();
    state.cancelled.clear();
    for (_, pending) in state.pending.drain() {
        let _ = pending.response.send(Err(message.to_string()));
    }
}

fn cleanup_cancelled_requests(state: &mut ChromeBridgeState, now: u64) {
    let ttl_ms = CANCELLED_REQUEST_TTL.as_millis() as u64;
    state
        .cancelled
        .retain(|_, cancelled_at| now.saturating_sub(*cancelled_at) <= ttl_ms);
}

fn status_from_state(state: &ChromeBridgeState, now: u64) -> ChromeBridgeStatus {
    let connection = state
        .connection
        .as_ref()
        .filter(|connection| connection_is_fresh(connection, now));
    ChromeBridgeStatus {
        connected: connection.is_some(),
        extension_id: CHROME_EXTENSION_ID.to_string(),
        extension_version: connection.and_then(|value| value.extension_version.clone()),
        extension_compatible: connection
            .and_then(|value| value.extension_version.as_deref())
            .is_some_and(|version| version == CHROME_EXTENSION_VERSION),
        connected_at_ms: connection.map(|value| value.connected_at_ms),
        last_seen_at_ms: connection.map(|value| value.last_seen_at_ms),
        claimed_tab_count: connection.map_or(0, |value| value.claimed_tab_count),
        authorized_origin_count: connection.map_or(0, |value| value.authorized_origin_count),
        pending_command_count: state.pending.len(),
    }
}

fn connection_is_fresh(connection: &ChromeConnection, now: u64) -> bool {
    now.saturating_sub(connection.last_seen_at_ms) <= CONNECTION_STALE_AFTER.as_millis() as u64
}

fn validate_connection_id(value: &str) -> Result<()> {
    if value.len() < 16
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("Chrome Native Messaging connection ID is invalid");
    }
    Ok(())
}

fn validate_request_id(value: &str) -> Result<()> {
    if value.len() != 36
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        bail!("Chrome command request ID is invalid");
    }
    Ok(())
}

fn required_event_text<'a>(event: &'a Value, field: &str, max_bytes: usize) -> Result<&'a str> {
    event
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
        })
        .with_context(|| format!("Chrome extension event has invalid {field}"))
}

fn bounded_event_count(event: &Value, field: &str) -> Result<usize> {
    event
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value <= 10_000)
        .and_then(|value| usize::try_from(value).ok())
        .with_context(|| format!("Chrome extension event has invalid {field}"))
}

fn validate_message_size(value: &Value) -> Result<()> {
    if serde_json::to_vec(value)?.len() > MAX_BRIDGE_MESSAGE_BYTES {
        bail!("Chrome bridge message exceeded the 1 MiB safety limit");
    }
    Ok(())
}

fn timestamp_ms() -> u64 {
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

    use super::*;

    #[test]
    fn rejects_untrusted_extension_origin() {
        let bridge = ChromeBridge::default();
        assert!(bridge
            .connect(
                "connection-1234567890",
                "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
                CHROME_NATIVE_PROTOCOL_VERSION,
            )
            .is_err());
    }

    #[tokio::test]
    async fn command_round_trip_is_bound_to_exact_connection() {
        let bridge = Arc::new(ChromeBridge::default());
        bridge
            .connect(
                "connection-1234567890",
                CHROME_EXTENSION_ORIGIN,
                CHROME_NATIVE_PROTOCOL_VERSION,
            )
            .expect("connect");
        bridge
            .receive_event(
                "connection-1234567890",
                json!({
                    "type": "hello",
                    "extension_id": CHROME_EXTENSION_ID,
                    "extension_version": "1.0.0",
                }),
            )
            .expect("hello");

        let executing = Arc::clone(&bridge);
        let worker = std::thread::spawn(move || {
            executing.execute_command("tabs", json!({"limit": 5}), Duration::from_secs(2), None)
        });
        let command = bridge
            .next_command("connection-1234567890")
            .await
            .expect("next command")
            .expect("queued command");
        assert_eq!(command.get("command").and_then(Value::as_str), Some("tabs"));
        let request_id = command
            .get("request_id")
            .and_then(Value::as_str)
            .expect("request ID");
        bridge
            .receive_event(
                "connection-1234567890",
                json!({
                    "type": "command_result",
                    "request_id": request_id,
                    "ok": true,
                    "result": {"tabs": []},
                }),
            )
            .expect("command result");
        assert_eq!(
            worker.join().expect("worker").expect("result"),
            json!({"tabs": []})
        );
    }

    #[test]
    fn writable_commands_require_the_exact_bundled_extension_version() {
        let bridge = ChromeBridge::default();
        bridge
            .connect(
                "connection-1234567890",
                CHROME_EXTENSION_ORIGIN,
                CHROME_NATIVE_PROTOCOL_VERSION,
            )
            .expect("connect");
        bridge
            .receive_event(
                "connection-1234567890",
                json!({
                    "type": "hello",
                    "extension_id": CHROME_EXTENSION_ID,
                    "extension_version": "1.0.0",
                }),
            )
            .expect("hello");
        assert!(!bridge.status().expect("status").extension_compatible);
        let error = bridge
            .execute_command(
                "click",
                json!({"tab_id":"ct1","target_id":"cr0123456789abcdef-1"}),
                Duration::from_secs(1),
                None,
            )
            .expect_err("old extension must reject writes");
        assert!(error.to_string().contains(CHROME_EXTENSION_VERSION));
    }

    #[tokio::test]
    async fn cancelled_command_emits_cancel_and_ignores_late_result() {
        let bridge = Arc::new(ChromeBridge::default());
        bridge
            .connect(
                "connection-1234567890",
                CHROME_EXTENSION_ORIGIN,
                CHROME_NATIVE_PROTOCOL_VERSION,
            )
            .expect("connect");
        bridge
            .receive_event(
                "connection-1234567890",
                json!({
                    "type": "hello",
                    "extension_id": CHROME_EXTENSION_ID,
                    "extension_version": CHROME_EXTENSION_VERSION,
                }),
            )
            .expect("hello");
        let cancelled = Arc::new(AtomicBool::new(false));
        let executing = Arc::clone(&bridge);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = std::thread::spawn(move || {
            executing.execute_command(
                "navigate",
                json!({"tab_id":"ct1","url":"https://example.com/next"}),
                Duration::from_secs(2),
                Some(worker_cancelled.as_ref()),
            )
        });
        let command = bridge
            .next_command("connection-1234567890")
            .await
            .expect("next command")
            .expect("queued command");
        let request_id = command
            .get("request_id")
            .and_then(Value::as_str)
            .expect("request ID")
            .to_string();
        cancelled.store(true, Ordering::SeqCst);
        assert!(worker.join().expect("worker").is_err());
        let cancel = bridge
            .next_command("connection-1234567890")
            .await
            .expect("next cancel")
            .expect("cancel command");
        assert_eq!(cancel.get("type").and_then(Value::as_str), Some("cancel"));
        assert_eq!(
            cancel.get("request_id").and_then(Value::as_str),
            Some(request_id.as_str())
        );
        bridge
            .receive_event(
                "connection-1234567890",
                json!({
                    "type": "command_result",
                    "request_id": request_id,
                    "ok": false,
                    "error": "cancelled",
                }),
            )
            .expect("late result is ignored");
    }
}
