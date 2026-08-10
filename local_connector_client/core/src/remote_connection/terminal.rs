// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, Weak};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use crate::relay::{relay_error_response, terminal_event, RelayRequest, RelayResponse};

use super::relay::error_response;
use super::runtime::{connect_session, is_ssh_would_block, is_would_block, RemoteConnectionSpec};

const SNAPSHOT_LIMIT_BYTES: usize = 512 * 1024;

#[derive(Clone, Default)]
pub(crate) struct RemoteTerminalManager {
    sessions: Arc<Mutex<BTreeMap<String, Arc<RemoteTerminalSession>>>>,
}

enum TerminalControl {
    Input(String),
    Resize { cols: u16, rows: u16 },
    Close,
}

struct RemoteTerminalSession {
    control_tx: std::sync::mpsc::Sender<TerminalControl>,
    history: StdMutex<VecDeque<String>>,
    history_bytes: StdMutex<usize>,
    busy: AtomicBool,
    alive: AtomicBool,
    last_activity_ms: std::sync::atomic::AtomicU64,
}

impl RemoteTerminalSession {
    fn start(
        id: String,
        connection: RemoteConnectionSpec,
        verification_code: Option<String>,
        cols: u16,
        rows: u16,
        outbound: mpsc::UnboundedSender<Value>,
    ) -> Result<Arc<Self>, String> {
        let ssh_session = connect_session(
            &connection,
            Duration::from_secs(180),
            verification_code.as_deref(),
        )?;
        let mut channel = ssh_session
            .channel_session()
            .map_err(|error| format!("open channel failed: {error}"))?;
        channel
            .request_pty(
                "xterm-256color",
                None,
                Some((cols.max(1) as u32, rows.max(1) as u32, 0, 0)),
            )
            .map_err(|error| format!("request pty failed: {error}"))?;
        channel
            .handle_extended_data(ssh2::ExtendedData::Merge)
            .map_err(|error| format!("merge remote stderr failed: {error}"))?;
        channel
            .shell()
            .map_err(|error| format!("start shell failed: {error}"))?;
        ssh_session.set_blocking(false);

        let (control_tx, control_rx) = std::sync::mpsc::channel();
        let session = Arc::new(Self {
            control_tx,
            history: StdMutex::new(VecDeque::new()),
            history_bytes: StdMutex::new(0),
            busy: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            last_activity_ms: std::sync::atomic::AtomicU64::new(now_millis()),
        });
        let worker_session = Arc::downgrade(&session);
        std::thread::spawn(move || {
            let _ssh_session = ssh_session;
            let mut buffer = [0u8; 4096];
            loop {
                let Some(worker_session) = Weak::upgrade(&worker_session) else {
                    let _ = channel.close();
                    return;
                };
                while let Ok(control) = control_rx.try_recv() {
                    match control {
                        TerminalControl::Input(data) => {
                            worker_session.touch();
                            if input_triggers_busy(data.as_str()) {
                                worker_session.set_busy(true, id.as_str(), &outbound);
                            }
                            if let Err(error) = write_channel(&mut channel, data.as_bytes()) {
                                worker_session.emit_error(id.as_str(), &outbound, error.as_str());
                                worker_session.finish(id.as_str(), &outbound, 1);
                                return;
                            }
                        }
                        TerminalControl::Resize { cols, rows } => {
                            worker_session.touch();
                            if let Err(error) = resize_channel(&mut channel, cols, rows) {
                                worker_session.emit_error(id.as_str(), &outbound, error.as_str());
                            }
                        }
                        TerminalControl::Close => {
                            let _ = channel.close();
                            worker_session.finish(id.as_str(), &outbound, 0);
                            return;
                        }
                    }
                }
                match channel.read(&mut buffer) {
                    Ok(0) if channel.eof() => {
                        let _ = channel.wait_close();
                        let code = channel.exit_status().unwrap_or(0);
                        worker_session.finish(id.as_str(), &outbound, code);
                        return;
                    }
                    Ok(0) => {}
                    Ok(size) => {
                        worker_session.touch();
                        let data = String::from_utf8_lossy(&buffer[..size]).into_owned();
                        worker_session.append_history(data.clone());
                        worker_session.set_busy(false, id.as_str(), &outbound);
                        let _ = outbound.send(terminal_event(
                            "terminal_output",
                            id.as_str(),
                            json!({ "data": data }),
                        ));
                    }
                    Err(error) if is_would_block(&error) => {}
                    Err(error) => {
                        worker_session.emit_error(
                            id.as_str(),
                            &outbound,
                            format!("remote read failed: {error}").as_str(),
                        );
                        worker_session.finish(id.as_str(), &outbound, 1);
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(8));
            }
        });
        Ok(session)
    }

    fn append_history(&self, data: String) {
        let Ok(mut history) = self.history.lock() else {
            return;
        };
        let Ok(mut total_bytes) = self.history_bytes.lock() else {
            return;
        };
        *total_bytes += data.len();
        history.push_back(data);
        while *total_bytes > SNAPSHOT_LIMIT_BYTES {
            let Some(removed) = history.pop_front() else {
                *total_bytes = 0;
                break;
            };
            *total_bytes = total_bytes.saturating_sub(removed.len());
        }
    }

    fn snapshot(&self) -> String {
        self.touch();
        self.history
            .lock()
            .map(|history| history.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn set_busy(&self, busy: bool, id: &str, outbound: &mpsc::UnboundedSender<Value>) {
        let previous = self.busy.swap(busy, Ordering::SeqCst);
        if previous != busy {
            let _ = outbound.send(terminal_event(
                "terminal_state",
                id,
                json!({ "busy": busy }),
            ));
        }
    }

    fn emit_error(&self, id: &str, outbound: &mpsc::UnboundedSender<Value>, error: &str) {
        let _ = outbound.send(terminal_event(
            "terminal_error",
            id,
            json!({ "error": error }),
        ));
    }

    fn finish(&self, id: &str, outbound: &mpsc::UnboundedSender<Value>, code: i32) {
        if !self.alive.swap(false, Ordering::SeqCst) {
            return;
        }
        self.set_busy(false, id, outbound);
        let _ = outbound.send(terminal_event("terminal_exit", id, json!({ "code": code })));
    }

    fn touch(&self) {
        self.last_activity_ms.store(now_millis(), Ordering::SeqCst);
    }

    fn is_idle_for(&self, duration: Duration) -> bool {
        now_millis().saturating_sub(self.last_activity_ms.load(Ordering::SeqCst))
            >= duration.as_millis() as u64
    }
}

impl RemoteTerminalManager {
    async fn ensure_session(
        &self,
        id: String,
        connection: RemoteConnectionSpec,
        verification_code: Option<String>,
        cols: u16,
        rows: u16,
        outbound: mpsc::UnboundedSender<Value>,
    ) -> Result<Arc<RemoteTerminalSession>, String> {
        if let Some(session) = self.sessions.lock().await.get(id.as_str()).cloned() {
            if session.alive.load(Ordering::SeqCst) {
                session.touch();
                let _ = session
                    .control_tx
                    .send(TerminalControl::Resize { cols, rows });
                return Ok(session);
            }
        }
        let session = tokio::task::spawn_blocking(move || {
            RemoteTerminalSession::start(
                id.clone(),
                connection,
                verification_code,
                cols,
                rows,
                outbound,
            )
            .map(|session| (id, session))
        })
        .await
        .map_err(|error| format!("remote terminal worker failed: {error}"))??;
        self.sessions
            .lock()
            .await
            .insert(session.0, session.1.clone());
        Ok(session.1)
    }

    async fn get(&self, id: &str) -> Option<Arc<RemoteTerminalSession>> {
        self.sessions.lock().await.get(id).cloned()
    }

    async fn close(&self, id: &str) {
        if let Some(session) = self.sessions.lock().await.remove(id) {
            let _ = session.control_tx.send(TerminalControl::Close);
        }
    }

    pub(crate) async fn close_idle(&self, max_idle: Duration) {
        let mut sessions = self.sessions.lock().await;
        let stale_ids = sessions
            .iter()
            .filter_map(|(id, session)| {
                (!session.alive.load(Ordering::SeqCst) || session.is_idle_for(max_idle))
                    .then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for id in stale_ids {
            if let Some(session) = sessions.remove(id.as_str()) {
                let _ = session.control_tx.send(TerminalControl::Close);
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    terminal_session_id: String,
    connection: RemoteConnectionSpec,
    verification_code: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct InputBody {
    terminal_session_id: String,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResizeBody {
    terminal_session_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct CloseBody {
    terminal_session_id: String,
}

pub(crate) async fn handle_remote_terminal_session_create(
    value: Value,
    manager: &RemoteTerminalManager,
    outbound: mpsc::UnboundedSender<Value>,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(error) => {
            return relay_error_response(
                "remote_terminal_session_create_response",
                "",
                400,
                error.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<CreateBody>(request.body) {
        Ok(body) => body,
        Err(error) => {
            return relay_error_response(
                "remote_terminal_session_create_response",
                request.request_id.as_str(),
                400,
                error.to_string(),
            );
        }
    };
    match manager
        .ensure_session(
            body.terminal_session_id.clone(),
            body.connection,
            body.verification_code,
            body.cols.unwrap_or(80).max(1),
            body.rows.unwrap_or(24).max(1),
            outbound,
        )
        .await
    {
        Ok(session) => RelayResponse {
            message_type: "remote_terminal_session_create_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body: json!({
                "terminal_session_id": body.terminal_session_id,
                "snapshot": session.snapshot(),
                "busy": session.busy.load(Ordering::SeqCst),
            }),
        }
        .into_value(),
        Err(error) => error_response(
            "remote_terminal_session_create_response",
            request.request_id,
            error,
        ),
    }
}

pub(crate) async fn handle_remote_terminal_input(value: Value, manager: &RemoteTerminalManager) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(body) = serde_json::from_value::<InputBody>(request.body) else {
        return;
    };
    if let Some(session) = manager.get(body.terminal_session_id.as_str()).await {
        let _ = session
            .control_tx
            .send(TerminalControl::Input(body.data.unwrap_or_default()));
    }
}

pub(crate) async fn handle_remote_terminal_resize(value: Value, manager: &RemoteTerminalManager) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(body) = serde_json::from_value::<ResizeBody>(request.body) else {
        return;
    };
    if let Some(session) = manager.get(body.terminal_session_id.as_str()).await {
        let _ = session.control_tx.send(TerminalControl::Resize {
            cols: body.cols.unwrap_or(80).max(1),
            rows: body.rows.unwrap_or(24).max(1),
        });
    }
}

pub(crate) async fn handle_remote_terminal_close(value: Value, manager: &RemoteTerminalManager) {
    let Ok(request) = serde_json::from_value::<RelayRequest>(value) else {
        return;
    };
    let Ok(body) = serde_json::from_value::<CloseBody>(request.body) else {
        return;
    };
    manager.close(body.terminal_session_id.as_str()).await;
}

fn write_channel(channel: &mut ssh2::Channel, mut data: &[u8]) -> Result<(), String> {
    while !data.is_empty() {
        match channel.write(data) {
            Ok(0) => return Err("remote channel closed".to_string()),
            Ok(size) => data = &data[size..],
            Err(error) if is_would_block(&error) => {
                std::thread::sleep(Duration::from_millis(6));
            }
            Err(error) => return Err(format!("remote write failed: {error}")),
        }
    }
    for _ in 0..60 {
        match channel.flush() {
            Ok(()) => return Ok(()),
            Err(error) if is_would_block(&error) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(format!("remote flush failed: {error}")),
        }
    }
    Err("remote flush timed out".to_string())
}

fn resize_channel(channel: &mut ssh2::Channel, cols: u16, rows: u16) -> Result<(), String> {
    for _ in 0..60 {
        match channel.request_pty_size(cols.max(1) as u32, rows.max(1) as u32, None, None) {
            Ok(()) => return Ok(()),
            Err(error) if is_ssh_would_block(&error) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(format!("remote resize failed: {error}")),
        }
    }
    Err("remote resize timed out".to_string())
}

fn input_triggers_busy(data: &str) -> bool {
    data.contains('\r')
        || data.contains('\n')
        || data
            .as_bytes()
            .iter()
            .any(|byte| matches!(*byte, 0x03 | 0x04 | 0x1a))
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::input_triggers_busy;

    #[test]
    fn busy_state_only_starts_for_submitted_or_control_input() {
        assert!(!input_triggers_busy("ls"));
        assert!(input_triggers_busy("ls\n"));
        assert!(input_triggers_busy("\u{3}"));
    }
}
