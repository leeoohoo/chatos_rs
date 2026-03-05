use axum::extract::ws::{Message, WebSocket};
use axum::http::StatusCode;
use axum::{
    extract::{Path, Query, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use once_cell::sync::OnceCell;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ssh2::{CheckResult, KnownHostFileKind, KnownHostKeyFormat, OpenFlags, OpenType, Session};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{timeout, Duration};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::normalize_non_empty;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

#[derive(Debug, Deserialize)]
struct RemoteConnectionQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateRemoteConnectionRequest {
    name: Option<String>,
    host: Option<String>,
    port: Option<i64>,
    username: Option<String>,
    auth_type: Option<String>,
    password: Option<String>,
    private_key_path: Option<String>,
    certificate_path: Option<String>,
    default_remote_path: Option<String>,
    host_key_policy: Option<String>,
    jump_enabled: Option<bool>,
    jump_host: Option<String>,
    jump_port: Option<i64>,
    jump_username: Option<String>,
    jump_private_key_path: Option<String>,
    jump_password: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateRemoteConnectionRequest {
    name: Option<String>,
    host: Option<String>,
    port: Option<i64>,
    username: Option<String>,
    auth_type: Option<String>,
    password: Option<String>,
    private_key_path: Option<String>,
    certificate_path: Option<String>,
    default_remote_path: Option<String>,
    host_key_policy: Option<String>,
    jump_enabled: Option<bool>,
    jump_host: Option<String>,
    jump_port: Option<i64>,
    jump_username: Option<String>,
    jump_private_key_path: Option<String>,
    jump_password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpListQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpUploadRequest {
    local_path: Option<String>,
    remote_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpDownloadRequest {
    remote_path: Option<String>,
    local_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpTransferStartRequest {
    direction: Option<String>,
    local_path: Option<String>,
    remote_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpMkdirRequest {
    parent_path: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpRenameRequest {
    from_path: Option<String>,
    to_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpDeleteRequest {
    path: Option<String>,
    recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WsInput {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "command")]
    Command { command: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum WsOutput {
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "snapshot")]
    Snapshot { data: String },
    #[serde(rename = "exit")]
    Exit { code: i32 },
    #[serde(rename = "state")]
    State { busy: bool },
    #[serde(rename = "error")]
    Error { error: String },
    #[serde(rename = "pong")]
    Pong { timestamp: String },
}

#[derive(Debug, Serialize)]
struct RemoteEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: Option<u64>,
    modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SftpTransferStatus {
    id: String,
    connection_id: String,
    direction: String,
    state: String,
    total_bytes: Option<u64>,
    transferred_bytes: u64,
    percent: Option<f64>,
    current_path: Option<String>,
    message: Option<String>,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
enum RemoteTerminalEvent {
    Output(String),
    Exit(i32),
    State(bool),
}

enum NativeTerminalControl {
    Input(String),
    Resize { cols: u16, rows: u16 },
}

const REMOTE_TERMINAL_SNAPSHOT_LIMIT_BYTES: usize = 512 * 1024;
const REMOTE_TERMINAL_IDLE_TIMEOUT: StdDuration = StdDuration::from_secs(20 * 60);
const REMOTE_TERMINAL_IDLE_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
enum DisconnectReason {
    Manual,
    IdleTimeout,
    ConnectionDeleted,
}

impl DisconnectReason {
    fn notice(self) -> &'static str {
        match self {
            DisconnectReason::Manual => "已手动断开连接",
            DisconnectReason::IdleTimeout => "20 分钟无操作，连接已自动断开",
            DisconnectReason::ConnectionDeleted => "连接配置已删除，终端已断开",
        }
    }
}

#[derive(Debug, Default)]
struct OutputHistory {
    chunks: VecDeque<String>,
    total_bytes: usize,
}

impl OutputHistory {
    fn push(&mut self, chunk: String) {
        if chunk.is_empty() {
            return;
        }
        self.total_bytes += chunk.len();
        self.chunks.push_back(chunk);

        while self.total_bytes > REMOTE_TERMINAL_SNAPSHOT_LIMIT_BYTES {
            let Some(removed) = self.chunks.pop_front() else {
                self.total_bytes = 0;
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(removed.len());
        }
    }

    fn snapshot(&self) -> String {
        if self.chunks.is_empty() {
            return String::new();
        }
        let mut output = String::with_capacity(self.total_bytes);
        for chunk in self.chunks.iter() {
            output.push_str(chunk.as_str());
        }
        output
    }
}

struct RemoteTerminalSession {
    sender: broadcast::Sender<RemoteTerminalEvent>,
    writer: Option<Mutex<Box<dyn Write + Send>>>,
    master: Option<Mutex<Box<dyn MasterPty + Send>>>,
    native_tx: Option<std::sync::mpsc::Sender<NativeTerminalControl>>,
    output_history: Mutex<OutputHistory>,
    last_activity_at: Mutex<Instant>,
    busy: AtomicBool,
    alive: AtomicBool,
}

impl RemoteTerminalSession {
    fn new(
        connection: &RemoteConnection,
    ) -> Result<
        (
            Arc<Self>,
            Option<Box<dyn portable_pty::Child + Send + Sync>>,
        ),
        String,
    > {
        if should_use_native_ssh(connection) {
            Self::new_native(connection)
        } else {
            Self::new_legacy(connection)
        }
    }

    fn new_legacy(
        connection: &RemoteConnection,
    ) -> Result<
        (
            Arc<Self>,
            Option<Box<dyn portable_pty::Child + Send + Sync>>,
        ),
        String,
    > {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("open pty failed: {e}"))?;

        let child = spawn_remote_shell(connection, pair.slave)?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("clone reader failed: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("take writer failed: {e}"))?;

        let (sender, _) = broadcast::channel(4096);

        let session = Arc::new(Self {
            sender,
            writer: Some(Mutex::new(writer)),
            master: Some(Mutex::new(pair.master)),
            native_tx: None,
            output_history: Mutex::new(OutputHistory::default()),
            last_activity_at: Mutex::new(Instant::now()),
            busy: AtomicBool::new(false),
            alive: AtomicBool::new(true),
        });

        let session_clone = session.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        session_clone.append_and_emit_output(text);
                    }
                    Err(_) => break,
                }
            }
        });

        Ok((session, Some(child)))
    }

    fn new_native(
        connection: &RemoteConnection,
    ) -> Result<
        (
            Arc<Self>,
            Option<Box<dyn portable_pty::Child + Send + Sync>>,
        ),
        String,
    > {
        let connected = connect_ssh2_session(connection, Duration::from_secs(12))?;
        let mut channel = connected
            .session
            .channel_session()
            .map_err(|e| format!("open channel failed: {e}"))?;
        channel
            .request_pty("xterm-256color", None, Some((80, 24, 0, 0)))
            .map_err(|e| format!("request pty failed: {e}"))?;
        channel
            .shell()
            .map_err(|e| format!("start shell failed: {e}"))?;
        connected.session.set_blocking(false);

        let (sender, _) = broadcast::channel(4096);
        let (control_tx, control_rx) = std::sync::mpsc::channel::<NativeTerminalControl>();
        let session = Arc::new(Self {
            sender,
            writer: None,
            master: None,
            native_tx: Some(control_tx),
            output_history: Mutex::new(OutputHistory::default()),
            last_activity_at: Mutex::new(Instant::now()),
            busy: AtomicBool::new(false),
            alive: AtomicBool::new(true),
        });

        let session_clone = session.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                while let Ok(control) = control_rx.try_recv() {
                    match control {
                        NativeTerminalControl::Input(data) => {
                            if let Err(err) =
                                write_channel_nonblocking(&mut channel, data.as_bytes())
                            {
                                session_clone.append_and_emit_output(format!(
                                    "\r\n[remote error] {err}\r\n"
                                ));
                                session_clone.mark_exited(1);
                                return;
                            }
                        }
                        NativeTerminalControl::Resize { cols, rows } => {
                            if let Err(err) = request_pty_resize_nonblocking(
                                &mut channel,
                                cols as u32,
                                rows as u32,
                            ) {
                                session_clone.append_and_emit_output(format!(
                                    "\r\n[remote resize error] {err}\r\n"
                                ));
                            }
                        }
                    }
                }

                match channel.read(&mut buf) {
                    Ok(0) => {
                        if channel.eof() {
                            let _ = channel.wait_close();
                            let code = channel.exit_status().unwrap_or(0);
                            session_clone.mark_exited(code);
                            return;
                        }
                    }
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        session_clone.append_and_emit_output(text);
                    }
                    Err(err) => {
                        if is_io_would_block(&err) {
                            std::thread::sleep(std::time::Duration::from_millis(8));
                        } else {
                            session_clone.append_and_emit_output(format!(
                                "\r\n[remote read error] {err}\r\n"
                            ));
                            session_clone.mark_exited(1);
                            return;
                        }
                    }
                }
            }
        });

        Ok((session, None))
    }

    fn subscribe(&self) -> broadcast::Receiver<RemoteTerminalEvent> {
        self.sender.subscribe()
    }

    fn write_input(&self, data: &str) -> Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }
        self.touch_activity();
        self.set_busy(input_triggers_busy(data));
        self.send_control_input(data)
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.touch_activity();
        if let Some(tx) = self.native_tx.as_ref() {
            return tx
                .send(NativeTerminalControl::Resize { cols, rows })
                .map_err(|_| "terminal resize channel closed".to_string());
        }
        let master_lock = self
            .master
            .as_ref()
            .ok_or_else(|| "master missing".to_string())?;
        let master = master_lock
            .lock()
            .map_err(|_| "master lock failed".to_string())?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("resize failed: {e}"))?;
        Ok(())
    }

    fn send_control_input(&self, data: &str) -> Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }
        if let Some(tx) = self.native_tx.as_ref() {
            return tx
                .send(NativeTerminalControl::Input(data.to_string()))
                .map_err(|_| "terminal input channel closed".to_string());
        }
        let writer_lock = self
            .writer
            .as_ref()
            .ok_or_else(|| "writer missing".to_string())?;
        let mut writer = writer_lock
            .lock()
            .map_err(|_| "writer lock failed".to_string())?;
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("write failed: {e}"))?;
        writer.flush().map_err(|e| format!("flush failed: {e}"))?;
        Ok(())
    }

    fn touch_activity(&self) {
        if let Ok(mut last_activity_at) = self.last_activity_at.lock() {
            *last_activity_at = Instant::now();
        }
    }

    fn set_busy(&self, busy: bool) {
        let prev = self.busy.swap(busy, Ordering::Relaxed);
        if prev != busy {
            let _ = self.sender.send(RemoteTerminalEvent::State(busy));
        }
    }

    fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    fn is_idle_timed_out(&self, now: Instant) -> bool {
        match self.last_activity_at.lock() {
            Ok(last_activity_at) => {
                now.saturating_duration_since(*last_activity_at) >= REMOTE_TERMINAL_IDLE_TIMEOUT
            }
            Err(_) => false,
        }
    }

    fn append_and_emit_output(&self, output: String) {
        if let Ok(mut history) = self.output_history.lock() {
            history.push(output.clone());
        }
        let _ = self.sender.send(RemoteTerminalEvent::Output(output));
        self.set_busy(false);
    }

    fn output_snapshot(&self) -> String {
        match self.output_history.lock() {
            Ok(history) => history.snapshot(),
            Err(_) => String::new(),
        }
    }

    fn disconnect(&self, reason: DisconnectReason) {
        self.append_and_emit_output(format!("\r\n[remote] {}\r\n", reason.notice()));
        let _ = self.send_control_input("\u{3}exit\n");
        self.mark_exited(0);
    }

    fn mark_exited(&self, code: i32) {
        self.alive.store(false, Ordering::Relaxed);
        self.set_busy(false);
        let _ = self.sender.send(RemoteTerminalEvent::Exit(code));
    }
}

struct ConnectedSshSession {
    session: Session,
}

fn is_ssh_would_block(err: &ssh2::Error) -> bool {
    matches!(err.code(), ssh2::ErrorCode::Session(code) if code == -37)
}

fn is_io_would_block(err: &std::io::Error) -> bool {
    matches!(err.kind(), std::io::ErrorKind::WouldBlock)
}

fn write_channel_nonblocking(channel: &mut ssh2::Channel, mut data: &[u8]) -> Result<(), String> {
    while !data.is_empty() {
        match channel.write(data) {
            Ok(0) => return Err("remote channel closed".to_string()),
            Ok(n) => {
                data = &data[n..];
            }
            Err(err) => {
                if is_io_would_block(&err) {
                    std::thread::sleep(StdDuration::from_millis(6));
                    continue;
                }
                return Err(format!("write channel failed: {err}"));
            }
        }
    }
    Ok(())
}

fn request_pty_resize_nonblocking(
    channel: &mut ssh2::Channel,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    for _ in 0..60 {
        match channel.request_pty_size(cols, rows, None, None) {
            Ok(_) => return Ok(()),
            Err(err) => {
                if is_ssh_would_block(&err) {
                    std::thread::sleep(StdDuration::from_millis(5));
                    continue;
                }
                return Err(format!("request pty resize failed: {err}"));
            }
        }
    }
    Err("request pty resize timed out".to_string())
}

struct RemoteTerminalManager {
    sessions: DashMap<String, Arc<RemoteTerminalSession>>,
}

impl RemoteTerminalManager {
    fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    fn spawn_idle_sweeper(manager: Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let mut ticker = tokio::time::interval(REMOTE_TERMINAL_IDLE_SWEEP_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                manager.close_idle_sessions();
            }
        });
    }

    fn get(&self, connection_id: &str) -> Option<Arc<RemoteTerminalSession>> {
        self.sessions.get(connection_id).map(|s| s.clone())
    }

    async fn ensure_running(
        &self,
        connection: &RemoteConnection,
    ) -> Result<Arc<RemoteTerminalSession>, String> {
        self.close_idle_sessions();

        if let Some(existing) = self.get(&connection.id) {
            if existing.is_alive() {
                if existing.is_idle_timed_out(Instant::now()) {
                    self.close_with_reason(&connection.id, DisconnectReason::IdleTimeout);
                } else {
                    return Ok(existing);
                }
            } else {
                self.sessions.remove(&connection.id);
            }
        }

        let (session, child) = RemoteTerminalSession::new(connection)?;
        if let Some(mut child) = child {
            let manager = get_remote_terminal_manager();
            let id = connection.id.clone();
            let session_for_wait = session.clone();
            std::thread::spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code()).unwrap_or(0) as i32;
                session_for_wait.mark_exited(code);
                manager.sessions.remove(&id);
            });
        }

        self.sessions.insert(connection.id.clone(), session.clone());
        let _ = RemoteConnectionService::touch(&connection.id).await;
        Ok(session)
    }

    fn close_with_reason(&self, connection_id: &str, reason: DisconnectReason) -> bool {
        if let Some((_, session)) = self.sessions.remove(connection_id) {
            session.disconnect(reason);
            true
        } else {
            false
        }
    }

    fn close_idle_sessions(&self) {
        let now = Instant::now();
        let stale_ids: Vec<String> = self
            .sessions
            .iter()
            .filter_map(|entry| {
                if entry.value().is_idle_timed_out(now) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();
        for id in stale_ids {
            let _ = self.close_with_reason(id.as_str(), DisconnectReason::IdleTimeout);
        }
    }
}

struct SftpTransferManager {
    transfers: DashMap<String, SftpTransferStatus>,
    cancel_flags: DashMap<String, bool>,
}

impl SftpTransferManager {
    fn new() -> Self {
        Self {
            transfers: DashMap::new(),
            cancel_flags: DashMap::new(),
        }
    }

    fn create(
        &self,
        connection_id: &str,
        direction: &str,
        total_bytes: Option<u64>,
        current_path: Option<String>,
    ) -> SftpTransferStatus {
        let id = Uuid::new_v4().to_string();
        let now = crate::core::time::now_rfc3339();
        let status = SftpTransferStatus {
            id: id.clone(),
            connection_id: connection_id.to_string(),
            direction: direction.to_string(),
            state: "pending".to_string(),
            total_bytes,
            transferred_bytes: 0,
            percent: total_bytes.and_then(|total| if total == 0 { Some(100.0) } else { Some(0.0) }),
            current_path,
            message: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.transfers.insert(id, status.clone());
        self.cancel_flags.insert(status.id.clone(), false);
        status
    }

    fn get_for_connection(
        &self,
        transfer_id: &str,
        connection_id: &str,
    ) -> Option<SftpTransferStatus> {
        self.transfers.get(transfer_id).and_then(|entry| {
            if entry.connection_id == connection_id {
                Some(entry.clone())
            } else {
                None
            }
        })
    }

    fn set_running(&self, transfer_id: &str) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            if self.is_cancel_requested(transfer_id) {
                entry.state = "cancelling".to_string();
                entry.message = Some("正在取消传输...".to_string());
                entry.updated_at = crate::core::time::now_rfc3339();
                return;
            }
            entry.state = "running".to_string();
            entry.updated_at = crate::core::time::now_rfc3339();
            entry.error = None;
            entry.message = None;
        }
    }

    fn set_progress(
        &self,
        transfer_id: &str,
        transferred_bytes: u64,
        total_bytes: Option<u64>,
        current_path: Option<String>,
    ) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.transferred_bytes = transferred_bytes;
            if total_bytes.is_some() {
                entry.total_bytes = total_bytes;
            }
            entry.current_path = current_path;
            entry.percent = entry.total_bytes.and_then(|total| {
                if total == 0 {
                    Some(100.0)
                } else {
                    Some(
                        ((entry.transferred_bytes as f64 * 100.0) / total as f64).clamp(0.0, 100.0),
                    )
                }
            });
            entry.updated_at = crate::core::time::now_rfc3339();
        }
    }

    fn set_done(&self, transfer_id: &str, message: String) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.state = "success".to_string();
            if let Some(total) = entry.total_bytes {
                entry.transferred_bytes = total;
                entry.percent = Some(100.0);
            } else if entry.transferred_bytes > 0 {
                entry.percent = Some(100.0);
            }
            entry.message = Some(message);
            entry.error = None;
            entry.updated_at = crate::core::time::now_rfc3339();
        }
        self.cancel_flags.remove(transfer_id);
    }

    fn set_error(&self, transfer_id: &str, error: String) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.state = "error".to_string();
            entry.error = Some(error);
            entry.message = None;
            entry.updated_at = crate::core::time::now_rfc3339();
        }
        self.cancel_flags.remove(transfer_id);
    }

    fn set_cancelled(&self, transfer_id: &str) {
        if let Some(mut entry) = self.transfers.get_mut(transfer_id) {
            entry.state = "cancelled".to_string();
            entry.message = Some("传输已取消".to_string());
            entry.error = None;
            entry.updated_at = crate::core::time::now_rfc3339();
        }
        self.cancel_flags.remove(transfer_id);
    }

    fn request_cancel_for_connection(&self, transfer_id: &str, connection_id: &str) -> bool {
        let Some(mut entry) = self.transfers.get_mut(transfer_id) else {
            return false;
        };
        if entry.connection_id != connection_id {
            return false;
        }
        match entry.state.as_str() {
            "success" | "error" | "cancelled" => false,
            _ => {
                entry.state = "cancelling".to_string();
                entry.message = Some("正在取消传输...".to_string());
                entry.updated_at = crate::core::time::now_rfc3339();
                self.cancel_flags.insert(transfer_id.to_string(), true);
                true
            }
        }
    }

    fn is_cancel_requested(&self, transfer_id: &str) -> bool {
        self.cancel_flags
            .get(transfer_id)
            .map(|v| *v)
            .unwrap_or(false)
    }
}

static REMOTE_TERMINAL_MANAGER: OnceCell<Arc<RemoteTerminalManager>> = OnceCell::new();
static SFTP_TRANSFER_MANAGER: OnceCell<Arc<SftpTransferManager>> = OnceCell::new();

fn get_remote_terminal_manager() -> Arc<RemoteTerminalManager> {
    REMOTE_TERMINAL_MANAGER
        .get_or_init(|| {
            let manager = Arc::new(RemoteTerminalManager::new());
            RemoteTerminalManager::spawn_idle_sweeper(manager.clone());
            manager
        })
        .clone()
}

fn get_sftp_transfer_manager() -> Arc<SftpTransferManager> {
    SFTP_TRANSFER_MANAGER
        .get_or_init(|| Arc::new(SftpTransferManager::new()))
        .clone()
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/remote-connections",
            get(list_remote_connections).post(create_remote_connection),
        )
        .route(
            "/api/remote-connections/test",
            axum::routing::post(test_remote_connection_draft),
        )
        .route(
            "/api/remote-connections/:id",
            get(get_remote_connection)
                .put(update_remote_connection)
                .delete(delete_remote_connection),
        )
        .route(
            "/api/remote-connections/:id/test",
            axum::routing::post(test_remote_connection_saved),
        )
        .route(
            "/api/remote-connections/:id/disconnect",
            axum::routing::post(disconnect_remote_terminal),
        )
        .route("/api/remote-connections/:id/ws", get(remote_terminal_ws))
        .route(
            "/api/remote-connections/:id/sftp/list",
            get(list_remote_sftp_entries),
        )
        .route(
            "/api/remote-connections/:id/sftp/upload",
            axum::routing::post(upload_file_to_remote),
        )
        .route(
            "/api/remote-connections/:id/sftp/download",
            axum::routing::post(download_file_from_remote),
        )
        .route(
            "/api/remote-connections/:id/sftp/transfer/start",
            axum::routing::post(start_sftp_transfer),
        )
        .route(
            "/api/remote-connections/:id/sftp/transfer/:transfer_id",
            get(get_sftp_transfer_status),
        )
        .route(
            "/api/remote-connections/:id/sftp/transfer/:transfer_id/cancel",
            axum::routing::post(cancel_sftp_transfer),
        )
        .route(
            "/api/remote-connections/:id/sftp/mkdir",
            axum::routing::post(create_remote_directory),
        )
        .route(
            "/api/remote-connections/:id/sftp/rename",
            axum::routing::post(rename_remote_entry),
        )
        .route(
            "/api/remote-connections/:id/sftp/delete",
            axum::routing::post(delete_remote_entry),
        )
}

async fn list_remote_connections(
    auth: AuthUser,
    Query(query): Query<RemoteConnectionQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    match RemoteConnectionService::list(Some(user_id)).await {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn create_remote_connection(
    auth: AuthUser,
    Json(req): Json<CreateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let normalized = match normalize_create_request(req, Some(user_id)) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err })),
            )
        }
    };

    if let Err(err) = RemoteConnectionService::create(normalized.clone()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        );
    }

    let saved = RemoteConnectionService::get_by_id(&normalized.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(normalized);

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
    )
}

async fn test_remote_connection_draft(
    auth: AuthUser,
    Json(req): Json<CreateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let connection = match normalize_create_request(req, Some(user_id)) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err })),
            );
        }
    };

    match run_remote_connectivity_test(&connection).await {
        Ok(result) => (StatusCode::OK, Json(result)),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn get_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => (
            StatusCode::OK,
            Json(serde_json::to_value(connection).unwrap_or(Value::Null)),
        ),
        Err(err) => map_remote_connection_access_error(err),
    }
}

async fn update_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let normalized = match normalize_update_request(req, existing.clone()) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err })),
            )
        }
    };

    if let Err(err) = RemoteConnectionService::update(&id, &normalized).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        );
    }

    match RemoteConnectionService::get_by_id(&id).await {
        Ok(Some(connection)) => (
            StatusCode::OK,
            Json(serde_json::to_value(connection).unwrap_or(Value::Null)),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "远端连接不存在" })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn delete_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_remote_connection(&id, &auth).await {
        return map_remote_connection_access_error(err);
    }

    let manager = get_remote_terminal_manager();
    manager.close_with_reason(&id, DisconnectReason::ConnectionDeleted);

    match RemoteConnectionService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true, "message": "远端连接已删除" })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn disconnect_remote_terminal(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_remote_connection(&id, &auth).await {
        return map_remote_connection_access_error(err);
    }

    let manager = get_remote_terminal_manager();
    let closed = manager.close_with_reason(&id, DisconnectReason::Manual);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "disconnected": closed,
            "message": if closed { "远端终端已断开" } else { "远端终端当前未连接" }
        })),
    )
}

async fn test_remote_connection_saved(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    match run_remote_connectivity_test(&connection).await {
        Ok(result) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(result))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn remote_terminal_ws(
    auth: AuthUser,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err).into_response(),
    };

    ws.on_upgrade(move |socket| handle_remote_terminal_socket(connection, socket))
}

async fn handle_remote_terminal_socket(connection: RemoteConnection, socket: WebSocket) {
    let manager = get_remote_terminal_manager();
    let session = match manager.ensure_running(&connection).await {
        Ok(session) => session,
        Err(err) => {
            let mut socket = socket;
            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&WsOutput::Error { error: err }).unwrap_or_default(),
                ))
                .await;
            return;
        }
    };

    session.touch_activity();
    let _ = RemoteConnectionService::touch(&connection.id).await;

    let mut receiver = session.subscribe();
    let (mut sender, mut receiver_ws) = socket.split();

    let snapshot = session.output_snapshot();
    if !snapshot.is_empty() {
        let payload = serde_json::to_string(&WsOutput::Snapshot { data: snapshot })
            .unwrap_or_else(|_| "{}".to_string());
        if sender.send(Message::Text(payload)).await.is_err() {
            return;
        }
    }
    let payload = serde_json::to_string(&WsOutput::State {
        busy: session.is_busy(),
    })
    .unwrap_or_else(|_| "{}".to_string());
    if sender.send(Message::Text(payload)).await.is_err() {
        return;
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let tx_events = tx.clone();
    let event_task = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(RemoteTerminalEvent::Output(data)) => {
                    let text = serde_json::to_string(&WsOutput::Output { data })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Ok(RemoteTerminalEvent::Exit(code)) => {
                    let text = serde_json::to_string(&WsOutput::Exit { code })
                        .unwrap_or_else(|_| "{}".to_string());
                    let _ = tx_events.send(Message::Text(text));
                    break;
                }
                Ok(RemoteTerminalEvent::State(busy)) => {
                    let text = serde_json::to_string(&WsOutput::State { busy })
                        .unwrap_or_else(|_| "{}".to_string());
                    if tx_events.send(Message::Text(text)).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(Ok(msg)) = receiver_ws.next().await {
        match msg {
            Message::Text(text) => {
                let parsed = serde_json::from_str::<WsInput>(&text);
                match parsed {
                    Ok(WsInput::Input { data }) => {
                        if let Err(err) = session.write_input(data.as_str()) {
                            let payload = serde_json::to_string(&WsOutput::Error { error: err })
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Command { command }) => {
                        let mut cmd = command;
                        if !cmd.ends_with('\n') {
                            cmd.push('\n');
                        }
                        if let Err(err) = session.write_input(cmd.as_str()) {
                            let payload = serde_json::to_string(&WsOutput::Error { error: err })
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        } else {
                            let _ = RemoteConnectionService::touch(&connection.id).await;
                        }
                    }
                    Ok(WsInput::Resize { cols, rows }) => {
                        if let Err(err) = session.resize(cols, rows) {
                            let payload = serde_json::to_string(&WsOutput::Error { error: err })
                                .unwrap_or_else(|_| "{}".to_string());
                            let _ = tx.send(Message::Text(payload));
                        }
                    }
                    Ok(WsInput::Ping) => {
                        session.touch_activity();
                        let timestamp = crate::core::time::now_rfc3339();
                        let payload = serde_json::to_string(&WsOutput::Pong { timestamp })
                            .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                    Err(err) => {
                        let payload = serde_json::to_string(&WsOutput::Error {
                            error: format!("invalid ws message: {err}"),
                        })
                        .unwrap_or_else(|_| "{}".to_string());
                        let _ = tx.send(Message::Text(payload));
                    }
                }
            }
            Message::Binary(data) => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = session.write_input(text.as_str());
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    drop(tx);
    event_task.abort();
    forward_task.abort();
    let _ = event_task.await;
    let _ = forward_task.await;
}

async fn list_remote_sftp_entries(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<SftpListQuery>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let path = normalize_non_empty(query.path)
        .or(connection.default_remote_path.clone())
        .unwrap_or_else(|| ".".to_string());

    match fetch_remote_entries(&connection, path.as_str()).await {
        Ok(entries) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "path": normalize_remote_path(path.as_str()),
                    "parent": remote_parent_path(path.as_str()),
                    "entries": entries
                })),
            )
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn upload_file_to_remote(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SftpUploadRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let local_path = match normalize_non_empty(req.local_path) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "local_path 不能为空" })),
            )
        }
    };
    let remote_path = match normalize_non_empty(req.remote_path) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "remote_path 不能为空" })),
            )
        }
    };

    let local = FsPath::new(&local_path);
    if !local.exists() || !local.is_file() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "本地文件不存在或不是文件" })),
        );
    }

    match run_scp_upload(&connection, local_path.as_str(), remote_path.as_str()).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn download_file_from_remote(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SftpDownloadRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let remote_path = match normalize_non_empty(req.remote_path) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "remote_path 不能为空" })),
            )
        }
    };
    let local_path = match normalize_non_empty(req.local_path) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "local_path 不能为空" })),
            )
        }
    };

    if let Some(parent) = FsPath::new(&local_path).parent() {
        if !parent.exists() || !parent.is_dir() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "本地目标目录不存在" })),
            );
        }
    }

    match run_scp_download(&connection, remote_path.as_str(), local_path.as_str()).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

fn normalize_transfer_direction(direction: Option<String>) -> Result<String, String> {
    match normalize_non_empty(direction)
        .unwrap_or_else(|| "upload".to_string())
        .to_lowercase()
        .as_str()
    {
        "upload" => Ok("upload".to_string()),
        "download" => Ok("download".to_string()),
        _ => Err("direction 仅支持 upload 或 download".to_string()),
    }
}

async fn start_sftp_transfer(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SftpTransferStartRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let direction = match normalize_transfer_direction(req.direction) {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err })),
            )
        }
    };

    let local_path = match normalize_non_empty(req.local_path) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "local_path 不能为空" })),
            )
        }
    };
    let remote_path = match normalize_non_empty(req.remote_path) {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "remote_path 不能为空" })),
            )
        }
    };

    if direction == "upload" {
        let source = FsPath::new(local_path.as_str());
        if !source.exists() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "本地路径不存在" })),
            );
        }
        if !source.is_file() && !source.is_dir() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "本地路径必须是文件或目录" })),
            );
        }
    } else if let Some(parent) = FsPath::new(local_path.as_str()).parent() {
        if !parent.exists() || !parent.is_dir() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "本地目标目录不存在" })),
            );
        }
    }

    let total_estimated = if direction == "upload" {
        estimate_local_total_bytes(FsPath::new(local_path.as_str())).ok()
    } else {
        None
    };
    let current_path = if direction == "upload" {
        Some(local_path.clone())
    } else {
        Some(remote_path.clone())
    };
    let transfer_manager = get_sftp_transfer_manager();
    let status = transfer_manager.create(
        connection.id.as_str(),
        direction.as_str(),
        total_estimated,
        current_path,
    );

    let connection_for_task = connection.clone();
    let transfer_id_for_task = status.id.clone();
    let direction_for_task = direction.clone();
    let local_for_task = local_path.clone();
    let remote_for_task = remote_path.clone();
    tokio::spawn(async move {
        run_sftp_transfer_task(
            connection_for_task,
            transfer_id_for_task,
            direction_for_task,
            local_for_task,
            remote_for_task,
        )
        .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(status).unwrap_or(Value::Null)),
    )
}

async fn get_sftp_transfer_status(
    auth: AuthUser,
    Path((id, transfer_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let transfer_manager = get_sftp_transfer_manager();
    match transfer_manager.get_for_connection(transfer_id.as_str(), connection.id.as_str()) {
        Some(status) => (
            StatusCode::OK,
            Json(serde_json::to_value(status).unwrap_or(Value::Null)),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "传输任务不存在" })),
        ),
    }
}

async fn cancel_sftp_transfer(
    auth: AuthUser,
    Path((id, transfer_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let transfer_manager = get_sftp_transfer_manager();
    let accepted = transfer_manager
        .request_cancel_for_connection(transfer_id.as_str(), connection.id.as_str());
    if !accepted {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "传输任务不存在或已结束" })),
        );
    }

    match transfer_manager.get_for_connection(transfer_id.as_str(), connection.id.as_str()) {
        Some(status) => (
            StatusCode::OK,
            Json(serde_json::to_value(status).unwrap_or(Value::Null)),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "传输任务不存在" })),
        ),
    }
}

async fn run_sftp_transfer_task(
    connection: RemoteConnection,
    transfer_id: String,
    direction: String,
    local_path: String,
    remote_path: String,
) {
    let transfer_manager = get_sftp_transfer_manager();
    transfer_manager.set_running(transfer_id.as_str());

    let transfer_manager_for_blocking = transfer_manager.clone();
    let connection_for_blocking = connection.clone();
    let direction_for_blocking = direction.clone();
    let local_for_blocking = local_path.clone();
    let remote_for_blocking = remote_path.clone();
    let transfer_id_for_blocking = transfer_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        run_sftp_transfer_job(
            &connection_for_blocking,
            transfer_id_for_blocking.as_str(),
            direction_for_blocking.as_str(),
            local_for_blocking.as_str(),
            remote_for_blocking.as_str(),
            transfer_manager_for_blocking.as_ref(),
        )
    })
    .await;

    match result {
        Ok(Ok(message)) => transfer_manager.set_done(transfer_id.as_str(), message),
        Ok(Err(err)) if is_transfer_cancelled_error(err.as_str()) => {
            transfer_manager.set_cancelled(transfer_id.as_str())
        }
        Ok(Err(err)) => transfer_manager.set_error(transfer_id.as_str(), err),
        Err(err) => {
            transfer_manager.set_error(transfer_id.as_str(), format!("传输线程执行失败: {err}"))
        }
    }

    let _ = RemoteConnectionService::touch(&connection.id).await;
}

async fn create_remote_directory(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SftpMkdirRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let parent = normalize_non_empty(req.parent_path).unwrap_or_else(|| ".".to_string());
    let name = match normalize_non_empty(req.name) {
        Some(name) => name,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "name 不能为空" })),
            )
        }
    };

    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "目录名不合法" })),
        );
    }

    let target_path = join_remote_path(parent.as_str(), name.as_str());
    let script = format!("mkdir -p {}", shell_quote(target_path.as_str()));
    match run_ssh_command(&connection, script.as_str(), Duration::from_secs(20)).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "success": true, "path": target_path })),
            )
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn rename_remote_entry(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SftpRenameRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let from_path = match normalize_non_empty(req.from_path) {
        Some(path) => path,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "from_path 不能为空" })),
            )
        }
    };
    let to_path = match normalize_non_empty(req.to_path) {
        Some(path) => path,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "to_path 不能为空" })),
            )
        }
    };

    let script = format!(
        "mv {} {}",
        shell_quote(from_path.as_str()),
        shell_quote(to_path.as_str())
    );
    match run_ssh_command(&connection, script.as_str(), Duration::from_secs(20)).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn delete_remote_entry(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SftpDeleteRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let path = match normalize_non_empty(req.path) {
        Some(path) => path,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "path 不能为空" })),
            )
        }
    };
    let recursive = req.recursive.unwrap_or(false);

    let quoted = shell_quote(path.as_str());
    let script = if recursive {
        format!("rm -rf {}", quoted)
    } else {
        format!(
            "if [ -d {p} ]; then rmdir {p}; else rm -f {p}; fi",
            p = quoted
        )
    };

    match run_ssh_command(&connection, script.as_str(), Duration::from_secs(20)).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

fn normalize_create_request(
    req: CreateRemoteConnectionRequest,
    user_id: Option<String>,
) -> Result<RemoteConnection, String> {
    let host = normalize_non_empty(req.host).ok_or_else(|| "host 不能为空".to_string())?;
    let username =
        normalize_non_empty(req.username).ok_or_else(|| "username 不能为空".to_string())?;
    let port = normalize_port(req.port.unwrap_or(22))?;
    let auth_type = normalize_auth_type(req.auth_type)?;
    let host_key_policy = normalize_host_key_policy(req.host_key_policy)?;
    let jump_enabled = req.jump_enabled.unwrap_or(false);

    let raw_password = normalize_non_empty(req.password);
    let raw_private_key_path = normalize_non_empty(req.private_key_path);
    let raw_certificate_path = normalize_non_empty(req.certificate_path);
    let jump_private_key_path = normalize_non_empty(req.jump_private_key_path);
    let jump_password = normalize_non_empty(req.jump_password);
    let (password, private_key_path, certificate_path) = match auth_type.as_str() {
        "password" => (raw_password, None, None),
        "private_key" => (None, raw_private_key_path, None),
        "private_key_cert" => (None, raw_private_key_path, raw_certificate_path),
        _ => return Err("不支持的 auth_type".to_string()),
    };

    validate_auth_fields(
        auth_type.as_str(),
        password.as_deref(),
        private_key_path.as_deref(),
        certificate_path.as_deref(),
    )?;
    validate_file_path_if_present(private_key_path.as_deref(), "private_key_path 文件不存在")?;
    validate_file_path_if_present(certificate_path.as_deref(), "certificate_path 文件不存在")?;
    validate_file_path_if_present(
        jump_private_key_path.as_deref(),
        "jump_private_key_path 文件不存在",
    )?;

    let jump_host = normalize_non_empty(req.jump_host);
    let jump_username = normalize_non_empty(req.jump_username);
    let jump_port = req.jump_port.map(normalize_port).transpose()?.or(Some(22));

    if jump_enabled && (jump_host.is_none() || jump_username.is_none()) {
        return Err("启用跳板机时 jump_host 和 jump_username 为必填".to_string());
    }

    let name = normalize_non_empty(req.name).unwrap_or_else(|| format!("{username}@{host}"));

    Ok(RemoteConnection::new(
        name,
        host,
        port,
        username,
        auth_type,
        password,
        private_key_path,
        certificate_path,
        normalize_non_empty(req.default_remote_path),
        host_key_policy,
        jump_enabled,
        jump_host,
        if jump_enabled { jump_port } else { None },
        jump_username,
        if jump_enabled {
            jump_private_key_path
        } else {
            None
        },
        if jump_enabled { jump_password } else { None },
        user_id,
    ))
}

fn normalize_update_request(
    req: UpdateRemoteConnectionRequest,
    existing: RemoteConnection,
) -> Result<RemoteConnection, String> {
    let host = req
        .host
        .and_then(|v| normalize_non_empty(Some(v)))
        .unwrap_or(existing.host.clone());
    let username = req
        .username
        .and_then(|v| normalize_non_empty(Some(v)))
        .unwrap_or(existing.username.clone());
    let port = match req.port {
        Some(p) => normalize_port(p)?,
        None => existing.port,
    };
    let auth_type = normalize_auth_type(req.auth_type.or(Some(existing.auth_type.clone())))?;
    let host_key_policy = normalize_host_key_policy(
        req.host_key_policy
            .or(Some(existing.host_key_policy.clone())),
    )?;
    let password_candidate = merge_optional_text(req.password, existing.password.clone());
    let private_key_candidate =
        merge_optional_text(req.private_key_path, existing.private_key_path.clone());
    let certificate_candidate =
        merge_optional_text(req.certificate_path, existing.certificate_path.clone());
    let (password, private_key_path, certificate_path) = match auth_type.as_str() {
        "password" => (password_candidate, None, None),
        "private_key" => (None, private_key_candidate, None),
        "private_key_cert" => (None, private_key_candidate, certificate_candidate),
        _ => return Err("不支持的 auth_type".to_string()),
    };

    let jump_enabled = req.jump_enabled.unwrap_or(existing.jump_enabled);

    let jump_host = merge_optional_text(req.jump_host, existing.jump_host.clone());
    let jump_username = merge_optional_text(req.jump_username, existing.jump_username.clone());
    let jump_private_key_path = merge_optional_text(
        req.jump_private_key_path,
        existing.jump_private_key_path.clone(),
    );
    let jump_password = merge_optional_text(req.jump_password, existing.jump_password.clone());

    let jump_port = match req.jump_port {
        Some(v) => Some(normalize_port(v)?),
        None => existing.jump_port.or(Some(22)),
    };

    validate_auth_fields(
        auth_type.as_str(),
        password.as_deref(),
        private_key_path.as_deref(),
        certificate_path.as_deref(),
    )?;
    validate_file_path_if_present(private_key_path.as_deref(), "private_key_path 文件不存在")?;
    validate_file_path_if_present(certificate_path.as_deref(), "certificate_path 文件不存在")?;
    validate_file_path_if_present(
        jump_private_key_path.as_deref(),
        "jump_private_key_path 文件不存在",
    )?;

    if jump_enabled && (jump_host.is_none() || jump_username.is_none()) {
        return Err("启用跳板机时 jump_host 和 jump_username 为必填".to_string());
    }

    let name = req
        .name
        .and_then(|v| normalize_non_empty(Some(v)))
        .unwrap_or(existing.name.clone());

    Ok(RemoteConnection {
        id: existing.id,
        name,
        host,
        port,
        username,
        auth_type,
        password,
        private_key_path,
        certificate_path,
        default_remote_path: merge_optional_text(
            req.default_remote_path,
            existing.default_remote_path,
        ),
        host_key_policy,
        jump_enabled,
        jump_host: if jump_enabled { jump_host } else { None },
        jump_port: if jump_enabled { jump_port } else { None },
        jump_username: if jump_enabled { jump_username } else { None },
        jump_private_key_path: if jump_enabled {
            jump_private_key_path
        } else {
            None
        },
        jump_password: if jump_enabled { jump_password } else { None },
        user_id: existing.user_id,
        created_at: existing.created_at,
        updated_at: existing.updated_at,
        last_active_at: existing.last_active_at,
    })
}

fn merge_optional_text(value: Option<String>, fallback: Option<String>) -> Option<String> {
    match value {
        Some(v) => normalize_non_empty(Some(v)),
        None => fallback,
    }
}

fn normalize_port(port: i64) -> Result<i64, String> {
    if !(1..=65535).contains(&port) {
        return Err("端口范围必须在 1-65535".to_string());
    }
    Ok(port)
}

fn normalize_auth_type(value: Option<String>) -> Result<String, String> {
    let raw = normalize_non_empty(value).unwrap_or_else(|| "private_key".to_string());
    match raw.as_str() {
        "private_key" | "private_key_cert" | "password" => Ok(raw),
        _ => Err("auth_type 仅支持 private_key、private_key_cert 或 password".to_string()),
    }
}

fn normalize_host_key_policy(value: Option<String>) -> Result<String, String> {
    let raw = normalize_non_empty(value).unwrap_or_else(|| "strict".to_string());
    match raw.as_str() {
        "strict" | "accept_new" => Ok(raw),
        _ => Err("host_key_policy 仅支持 strict 或 accept_new".to_string()),
    }
}

fn validate_auth_fields(
    auth_type: &str,
    password: Option<&str>,
    private_key_path: Option<&str>,
    certificate_path: Option<&str>,
) -> Result<(), String> {
    match auth_type {
        "password" => {
            if password.is_none() {
                return Err("password 模式需要提供 password".to_string());
            }
        }
        "private_key" => {
            if private_key_path.is_none() {
                return Err("private_key 模式需要提供 private_key_path".to_string());
            }
        }
        "private_key_cert" => {
            if private_key_path.is_none() {
                return Err("private_key_cert 模式需要提供 private_key_path".to_string());
            }
            if certificate_path.is_none() {
                return Err("private_key_cert 模式需要提供 certificate_path".to_string());
            }
        }
        _ => return Err("不支持的 auth_type".to_string()),
    }
    Ok(())
}

fn validate_file_path_if_present(path: Option<&str>, error_message: &str) -> Result<(), String> {
    if let Some(path) = path {
        if !FsPath::new(path).is_file() {
            return Err(error_message.to_string());
        }
    }
    Ok(())
}

fn should_use_native_ssh(_connection: &RemoteConnection) -> bool {
    true
}

fn known_hosts_file_path() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法定位用户 home 目录".to_string())?;
    Ok(home.join(".ssh").join("known_hosts"))
}

fn host_key_format_from_ssh2(key_type: ssh2::HostKeyType) -> Result<KnownHostKeyFormat, String> {
    match key_type {
        ssh2::HostKeyType::Rsa => Ok(KnownHostKeyFormat::SshRsa),
        ssh2::HostKeyType::Dss => Ok(KnownHostKeyFormat::SshDss),
        ssh2::HostKeyType::Ecdsa256 => Ok(KnownHostKeyFormat::Ecdsa256),
        ssh2::HostKeyType::Ecdsa384 => Ok(KnownHostKeyFormat::Ecdsa384),
        ssh2::HostKeyType::Ecdsa521 => Ok(KnownHostKeyFormat::Ecdsa521),
        ssh2::HostKeyType::Ed25519 => Ok(KnownHostKeyFormat::Ed25519),
        ssh2::HostKeyType::Unknown => Err("不支持的主机公钥类型".to_string()),
    }
}

fn host_key_record_name(host: &str, port: i64) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{}]:{}", host, port)
    }
}

fn replace_known_host_entry(
    known_hosts: &mut ssh2::KnownHosts,
    known_hosts_path: &FsPath,
    host: &str,
    port: i64,
    host_key: &[u8],
    host_key_type: ssh2::HostKeyType,
) -> Result<(), String> {
    let mut aliases = vec![host_key_record_name(host, port)];
    let plain_host = host.to_string();
    if !aliases.iter().any(|item| item == &plain_host) {
        aliases.push(plain_host);
    }

    for entry in known_hosts
        .hosts()
        .map_err(|e| format!("读取 known_hosts 条目失败: {e}"))?
    {
        if let Some(name) = entry.name() {
            if aliases.iter().any(|alias| alias == name) {
                known_hosts
                    .remove(&entry)
                    .map_err(|e| format!("更新 known_hosts 失败: {e}"))?;
            }
        }
    }

    let key_format = host_key_format_from_ssh2(host_key_type)?;
    let host_for_add = host_key_record_name(host, port);
    known_hosts
        .add(host_for_add.as_str(), host_key, "", key_format)
        .map_err(|e| format!("写入 known_hosts 失败: {e}"))?;
    known_hosts
        .write_file(known_hosts_path, KnownHostFileKind::OpenSSH)
        .map_err(|e| format!("保存 known_hosts 失败: {e}"))?;
    Ok(())
}

fn apply_host_key_policy(
    session: &Session,
    host: &str,
    port: i64,
    host_key_policy: &str,
) -> Result<(), String> {
    let (host_key, host_key_type) = session
        .host_key()
        .ok_or_else(|| "远端未返回主机公钥".to_string())?;
    let mut known_hosts = session
        .known_hosts()
        .map_err(|e| format!("读取 known_hosts 失败: {e}"))?;

    let known_hosts_path = known_hosts_file_path()?;
    if known_hosts_path.exists() {
        known_hosts
            .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
            .map_err(|e| format!("加载 known_hosts 失败: {e}"))?;
    }

    let check_result = known_hosts.check_port(host, port as u16, host_key);
    match check_result {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch if host_key_policy == "accept_new" => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建 ~/.ssh 目录失败: {e}"))?;
            }
            replace_known_host_entry(
                &mut known_hosts,
                &known_hosts_path,
                host,
                port,
                host_key,
                host_key_type,
            )
        }
        CheckResult::Mismatch => Err(
            "主机指纹与 known_hosts 记录不匹配，请核对服务器或切换 accept_new 后重试".to_string(),
        ),
        CheckResult::NotFound if host_key_policy == "accept_new" => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建 ~/.ssh 目录失败: {e}"))?;
            }
            replace_known_host_entry(
                &mut known_hosts,
                &known_hosts_path,
                host,
                port,
                host_key,
                host_key_type,
            )
        }
        CheckResult::NotFound => {
            Err("主机指纹未受信任，请先加入 known_hosts 或使用 accept_new".to_string())
        }
        CheckResult::Failure => Err("主机指纹校验失败".to_string()),
    }
}

fn connect_tcp_stream(
    host: &str,
    port: i64,
    timeout: StdDuration,
    label: &str,
) -> Result<TcpStream, String> {
    let addr = format!("{host}:{port}");
    let mut last_error = None;
    let mut stream_opt = None;
    let addrs = addr
        .to_socket_addrs()
        .map_err(|e| format!("解析{label}地址失败: {e}"))?;
    for socket in addrs {
        match TcpStream::connect_timeout(&socket, timeout) {
            Ok(stream) => {
                stream_opt = Some(stream);
                break;
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }
    }
    stream_opt.ok_or_else(|| {
        format!(
            "连接{label}失败: {}",
            last_error.unwrap_or_else(|| "无可用地址".to_string())
        )
    })
}

fn configure_stream_timeout(
    stream: &TcpStream,
    timeout: StdDuration,
    label: &str,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("设置{label}读超时失败: {e}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| format!("设置{label}写超时失败: {e}"))?;
    Ok(())
}

fn authenticate_target_session(
    session: &Session,
    connection: &RemoteConnection,
) -> Result<(), String> {
    match connection.auth_type.as_str() {
        "password" => {
            let password = connection
                .password
                .as_ref()
                .ok_or_else(|| "password 模式需要提供 password".to_string())?;
            session
                .userauth_password(connection.username.as_str(), password.as_str())
                .map_err(|e| format!("密码认证失败: {e}"))?;
        }
        "private_key" | "private_key_cert" => {
            let private_key = connection
                .private_key_path
                .as_ref()
                .ok_or_else(|| "私钥路径不能为空".to_string())?;
            let cert_path = connection.certificate_path.as_ref().map(FsPath::new);
            session
                .userauth_pubkey_file(
                    connection.username.as_str(),
                    cert_path,
                    FsPath::new(private_key),
                    None,
                )
                .map_err(|e| format!("密钥认证失败: {e}"))?;
        }
        _ => return Err("不支持的认证方式".to_string()),
    }
    Ok(())
}

fn authenticate_jump_session(
    session: &Session,
    connection: &RemoteConnection,
    jump_username: &str,
) -> Result<(), String> {
    let mut failures = Vec::new();

    if let Some(jump_key_path) = connection.jump_private_key_path.as_ref() {
        match session.userauth_pubkey_file(jump_username, None, FsPath::new(jump_key_path), None) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("jump_private_key_path 认证失败: {err}")),
        }
    }

    if let Some(jump_password) = connection.jump_password.as_ref() {
        match session.userauth_password(jump_username, jump_password.as_str()) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("jump_password 认证失败: {err}")),
        }
    }

    if connection.auth_type != "password" {
        if let Some(private_key_path) = connection.private_key_path.as_ref() {
            let cert_path = connection.certificate_path.as_ref().map(FsPath::new);
            match session.userauth_pubkey_file(
                jump_username,
                cert_path,
                FsPath::new(private_key_path),
                None,
            ) {
                Ok(_) => return Ok(()),
                Err(err) => failures.push(format!("复用目标密钥认证失败: {err}")),
            }
        }
    }

    match session.userauth_agent(jump_username) {
        Ok(_) => return Ok(()),
        Err(err) => failures.push(format!("SSH Agent 认证失败: {err}")),
    }

    if let Some(password) = connection.password.as_ref() {
        match session.userauth_password(jump_username, password.as_str()) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("使用同密码认证失败: {err}")),
        }
    }

    if failures.is_empty() {
        return Err("跳板机认证失败".to_string());
    }

    Err(format!(
        "跳板机认证失败：{}。请配置 jump_private_key_path、jump_password 或 SSH Agent",
        failures.join("；")
    ))
}

fn forward_jump_tunnel(
    local_stream: &mut TcpStream,
    jump_channel: &mut ssh2::Channel,
) -> Result<(), String> {
    const BUFFER_SIZE: usize = 8192;
    const MAX_PENDING: usize = 256 * 1024;

    let mut from_local = [0u8; BUFFER_SIZE];
    let mut from_remote = [0u8; BUFFER_SIZE];
    let mut pending_to_remote = Vec::<u8>::new();
    let mut pending_to_local = Vec::<u8>::new();
    let mut local_eof = false;
    let mut remote_eof = false;
    let mut remote_eof_sent = false;
    let mut local_shutdown = false;

    loop {
        let mut progressed = false;

        if !local_eof && pending_to_remote.len() < MAX_PENDING {
            match local_stream.read(&mut from_local) {
                Ok(0) => {
                    local_eof = true;
                    progressed = true;
                }
                Ok(n) => {
                    pending_to_remote.extend_from_slice(&from_local[..n]);
                    progressed = true;
                }
                Err(err) => {
                    if !is_io_would_block(&err) {
                        return Err(format!("读取本地隧道失败: {err}"));
                    }
                }
            }
        }

        while !pending_to_remote.is_empty() {
            match jump_channel.write(pending_to_remote.as_slice()) {
                Ok(0) => return Err("跳板机隧道已关闭".to_string()),
                Ok(n) => {
                    pending_to_remote.drain(..n);
                    progressed = true;
                }
                Err(err) => {
                    if is_io_would_block(&err) {
                        break;
                    }
                    return Err(format!("写入跳板机隧道失败: {err}"));
                }
            }
        }

        if !remote_eof && pending_to_local.len() < MAX_PENDING {
            match jump_channel.read(&mut from_remote) {
                Ok(0) => {
                    if jump_channel.eof() {
                        remote_eof = true;
                        progressed = true;
                    }
                }
                Ok(n) => {
                    pending_to_local.extend_from_slice(&from_remote[..n]);
                    progressed = true;
                }
                Err(err) => {
                    if !is_io_would_block(&err) {
                        return Err(format!("读取跳板机隧道失败: {err}"));
                    }
                }
            }
        }

        while !pending_to_local.is_empty() {
            match local_stream.write(pending_to_local.as_slice()) {
                Ok(0) => return Err("本地隧道已关闭".to_string()),
                Ok(n) => {
                    pending_to_local.drain(..n);
                    progressed = true;
                }
                Err(err) => {
                    if is_io_would_block(&err) {
                        break;
                    }
                    return Err(format!("写入本地隧道失败: {err}"));
                }
            }
        }

        if local_eof && pending_to_remote.is_empty() && !remote_eof_sent {
            match jump_channel.send_eof() {
                Ok(_) => {
                    remote_eof_sent = true;
                    progressed = true;
                }
                Err(err) => {
                    if !is_ssh_would_block(&err) {
                        return Err(format!("关闭跳板机发送流失败: {err}"));
                    }
                }
            }
        }

        if remote_eof && pending_to_local.is_empty() && !local_shutdown {
            let _ = local_stream.shutdown(Shutdown::Write);
            local_shutdown = true;
            progressed = true;
        }

        if local_eof && remote_eof && pending_to_remote.is_empty() && pending_to_local.is_empty() {
            let _ = jump_channel.close();
            let _ = jump_channel.wait_close();
            return Ok(());
        }

        if !progressed {
            std::thread::sleep(StdDuration::from_millis(5));
        }
    }
}

fn run_jump_tunnel_bridge(
    listener: TcpListener,
    jump_session: Session,
    mut jump_channel: ssh2::Channel,
    timeout: StdDuration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut local_stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(err) => {
                if is_io_would_block(&err) {
                    if Instant::now() >= deadline {
                        return Err("等待本地跳板连接超时".to_string());
                    }
                    std::thread::sleep(StdDuration::from_millis(5));
                    continue;
                }
                return Err(format!("接受本地跳板连接失败: {err}"));
            }
        }
    };

    local_stream
        .set_nonblocking(true)
        .map_err(|e| format!("设置本地跳板非阻塞失败: {e}"))?;
    jump_session.set_blocking(false);

    forward_jump_tunnel(&mut local_stream, &mut jump_channel)
}

fn create_jump_tunnel_stream(
    connection: &RemoteConnection,
    timeout: StdDuration,
    timeout_ms: u32,
) -> Result<TcpStream, String> {
    let jump_host = connection
        .jump_host
        .as_deref()
        .ok_or_else(|| "启用跳板机时 jump_host 不能为空".to_string())?;
    let jump_username = connection
        .jump_username
        .as_deref()
        .ok_or_else(|| "启用跳板机时 jump_username 不能为空".to_string())?;
    let jump_port = connection.jump_port.unwrap_or(22);

    let jump_stream = connect_tcp_stream(jump_host, jump_port, timeout, "跳板机")?;
    configure_stream_timeout(&jump_stream, timeout, "跳板机")?;

    let mut jump_session = Session::new().map_err(|e| format!("创建跳板机 SSH 会话失败: {e}"))?;
    jump_session.set_tcp_stream(jump_stream);
    jump_session.set_timeout(timeout_ms);
    jump_session
        .handshake()
        .map_err(|e| format!("跳板机 SSH 握手失败: {e}"))?;
    apply_host_key_policy(
        &jump_session,
        jump_host,
        jump_port,
        connection.host_key_policy.as_str(),
    )?;
    authenticate_jump_session(&jump_session, connection, jump_username)?;
    if !jump_session.authenticated() {
        return Err("跳板机 SSH 认证失败".to_string());
    }

    let target_port = u16::try_from(connection.port).map_err(|_| "目标端口无效".to_string())?;
    let jump_channel = jump_session
        .channel_direct_tcpip(connection.host.as_str(), target_port, None)
        .map_err(|e| format!("建立跳板机转发通道失败: {e}"))?;

    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("创建本地跳板通道失败: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("设置本地跳板通道失败: {e}"))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| format!("获取本地跳板地址失败: {e}"))?;

    std::thread::spawn(move || {
        let _ = run_jump_tunnel_bridge(listener, jump_session, jump_channel, timeout);
    });

    let local_stream = TcpStream::connect_timeout(&local_addr, timeout)
        .map_err(|e| format!("连接本地跳板通道失败: {e}"))?;
    configure_stream_timeout(&local_stream, timeout, "本地跳板通道")?;
    Ok(local_stream)
}

fn connect_ssh2_session(
    connection: &RemoteConnection,
    timeout_duration: Duration,
) -> Result<ConnectedSshSession, String> {
    let timeout = StdDuration::from_millis(timeout_duration.as_millis().max(1) as u64);
    let timeout_ms = timeout_duration.as_millis().clamp(1000, u32::MAX as u128) as u32;
    let stream = if connection.jump_enabled {
        create_jump_tunnel_stream(connection, timeout, timeout_ms)?
    } else {
        let stream =
            connect_tcp_stream(connection.host.as_str(), connection.port, timeout, "远端")?;
        configure_stream_timeout(&stream, timeout, "远端")?;
        stream
    };

    let mut session = Session::new().map_err(|e| format!("创建 SSH 会话失败: {e}"))?;
    session.set_tcp_stream(stream);
    session.set_timeout(timeout_ms);
    session
        .handshake()
        .map_err(|e| format!("SSH 握手失败: {e}"))?;
    apply_host_key_policy(
        &session,
        connection.host.as_str(),
        connection.port,
        connection.host_key_policy.as_str(),
    )?;
    authenticate_target_session(&session, connection)?;

    if !session.authenticated() {
        return Err("SSH 认证失败".to_string());
    }

    Ok(ConnectedSshSession { session })
}

fn spawn_remote_shell(
    connection: &RemoteConnection,
    slave: Box<dyn portable_pty::SlavePty + Send>,
) -> Result<Box<dyn portable_pty::Child + Send + Sync>, String> {
    let mut cmd = if is_password_auth(connection) {
        let password = connection
            .password
            .as_ref()
            .ok_or_else(|| "password 模式需要提供 password".to_string())?;
        let mut builder = CommandBuilder::new("sshpass");
        builder.arg("-p");
        builder.arg(password.as_str());
        builder.arg("ssh");
        builder
    } else {
        CommandBuilder::new("ssh")
    };
    let args = build_ssh_args(connection, true, connection.default_remote_path.as_deref());
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    slave.spawn_command(cmd).map_err(|e| {
        let text = e.to_string();
        if is_password_auth(connection) && text.contains("No such file") {
            "ssh spawn failed: 未找到 sshpass，请先安装 sshpass 后再使用密码登录".to_string()
        } else {
            format!("ssh spawn failed: {e}")
        }
    })
}

fn build_ssh_args(
    connection: &RemoteConnection,
    interactive: bool,
    default_remote_path: Option<&str>,
) -> Vec<String> {
    let mut args = Vec::new();

    if interactive {
        args.push("-tt".to_string());
    }

    args.push("-o".to_string());
    args.push(format!(
        "BatchMode={}",
        if is_password_auth(connection) {
            "no"
        } else {
            "yes"
        }
    ));

    args.push("-o".to_string());
    args.push("ConnectTimeout=10".to_string());

    args.push("-o".to_string());
    args.push(format!(
        "StrictHostKeyChecking={}",
        if connection.host_key_policy == "accept_new" {
            "accept-new"
        } else {
            "yes"
        }
    ));

    if !is_password_auth(connection) {
        if let Some(path) = connection.private_key_path.as_ref() {
            args.push("-i".to_string());
            args.push(path.clone());
        }

        if let Some(path) = connection.certificate_path.as_ref() {
            args.push("-o".to_string());
            args.push(format!("CertificateFile={path}"));
        }
    }

    if connection.jump_enabled {
        if let (Some(host), Some(username)) = (
            connection.jump_host.as_ref(),
            connection.jump_username.as_ref(),
        ) {
            if let Some(jump_key) = connection.jump_private_key_path.as_ref() {
                let jump_port = connection.jump_port.unwrap_or(22);
                let proxy = format!(
                    "ssh -i {} -p {} -W %h:%p {}@{}",
                    shell_quote(jump_key),
                    jump_port,
                    shell_quote(username),
                    shell_quote(host)
                );
                args.push("-o".to_string());
                args.push(format!("ProxyCommand={proxy}"));
            } else {
                let mut target = format!("{username}@{host}");
                if let Some(port) = connection.jump_port {
                    target.push(':');
                    target.push_str(port.to_string().as_str());
                }
                args.push("-J".to_string());
                args.push(target);
            }
        }
    }

    args.push("-p".to_string());
    args.push(connection.port.to_string());

    args.push(format!("{}@{}", connection.username, connection.host));

    if let Some(path) = default_remote_path {
        args.push(build_remote_login_command(path));
    }

    args
}

fn build_remote_login_command(path: &str) -> String {
    let quoted = shell_quote(path);
    format!("cd {quoted} 2>/dev/null || true; exec \"${{SHELL:-/bin/bash}}\" -l")
}

fn build_scp_args(connection: &RemoteConnection) -> Vec<String> {
    let mut args = Vec::new();

    args.push("-q".to_string());

    args.push("-o".to_string());
    args.push(format!(
        "BatchMode={}",
        if is_password_auth(connection) {
            "no"
        } else {
            "yes"
        }
    ));

    args.push("-o".to_string());
    args.push("ConnectTimeout=15".to_string());

    args.push("-o".to_string());
    args.push(format!(
        "StrictHostKeyChecking={}",
        if connection.host_key_policy == "accept_new" {
            "accept-new"
        } else {
            "yes"
        }
    ));

    if !is_password_auth(connection) {
        if let Some(path) = connection.private_key_path.as_ref() {
            args.push("-i".to_string());
            args.push(path.clone());
        }

        if let Some(path) = connection.certificate_path.as_ref() {
            args.push("-o".to_string());
            args.push(format!("CertificateFile={path}"));
        }
    }

    if connection.jump_enabled {
        if let (Some(host), Some(username)) = (
            connection.jump_host.as_ref(),
            connection.jump_username.as_ref(),
        ) {
            if let Some(jump_key) = connection.jump_private_key_path.as_ref() {
                let jump_port = connection.jump_port.unwrap_or(22);
                let proxy = format!(
                    "ssh -i {} -p {} -W %h:%p {}@{}",
                    shell_quote(jump_key),
                    jump_port,
                    shell_quote(username),
                    shell_quote(host)
                );
                args.push("-o".to_string());
                args.push(format!("ProxyCommand={proxy}"));
            } else {
                let mut target = format!("{username}@{host}");
                if let Some(port) = connection.jump_port {
                    target.push(':');
                    target.push_str(port.to_string().as_str());
                }
                args.push("-J".to_string());
                args.push(target);
            }
        }
    }

    args.push("-P".to_string());
    args.push(connection.port.to_string());

    args
}

fn is_password_auth(connection: &RemoteConnection) -> bool {
    connection.auth_type == "password"
}

fn build_ssh_process_command(
    connection: &RemoteConnection,
) -> Result<tokio::process::Command, String> {
    if is_password_auth(connection) {
        let password = connection
            .password
            .as_ref()
            .ok_or_else(|| "password 模式需要提供 password".to_string())?;
        let mut cmd = tokio::process::Command::new("sshpass");
        cmd.arg("-p");
        cmd.arg(password);
        cmd.arg("ssh");
        Ok(cmd)
    } else {
        Ok(tokio::process::Command::new("ssh"))
    }
}

fn build_scp_process_command(
    connection: &RemoteConnection,
) -> Result<tokio::process::Command, String> {
    if is_password_auth(connection) {
        let password = connection
            .password
            .as_ref()
            .ok_or_else(|| "password 模式需要提供 password".to_string())?;
        let mut cmd = tokio::process::Command::new("sshpass");
        cmd.arg("-p");
        cmd.arg(password);
        cmd.arg("scp");
        Ok(cmd)
    } else {
        Ok(tokio::process::Command::new("scp"))
    }
}

fn map_command_spawn_error(prefix: &str, error: std::io::Error, password_auth: bool) -> String {
    if password_auth && error.kind() == std::io::ErrorKind::NotFound {
        return format!("{prefix}: 未找到 sshpass，请先安装 sshpass 后再使用密码登录");
    }
    format!("{prefix}: {error}")
}

async fn fetch_remote_entries(
    connection: &RemoteConnection,
    path: &str,
) -> Result<Vec<RemoteEntry>, String> {
    let normalized = normalize_remote_path(path);
    let quoted = shell_quote(normalized.as_str());
    let script = format!(
        "set -e; P={quoted}; if [ ! -d \"$P\" ]; then echo __CHATOS_DIR_NOT_FOUND__; exit 52; fi; cd \"$P\"; find . -mindepth 1 -maxdepth 1 -printf '%P\\t%y\\t%s\\t%T@\\n'"
    );

    let output = run_ssh_command(connection, script.as_str(), Duration::from_secs(20)).await?;
    if output.contains("__CHATOS_DIR_NOT_FOUND__") {
        return Err("远端目录不存在".to_string());
    }

    let mut entries = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split('\t');
        let name = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }

        let kind = parts.next().unwrap_or("f");
        let size = parts.next().and_then(|s| s.parse::<u64>().ok());
        let modified_at = parts.next().map(|s| s.to_string());
        let is_dir = kind == "d";

        entries.push(RemoteEntry {
            path: join_remote_path(normalized.as_str(), name.as_str()),
            name,
            is_dir,
            size,
            modified_at,
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

async fn run_ssh_command(
    connection: &RemoteConnection,
    remote_command: &str,
    timeout_duration: Duration,
) -> Result<String, String> {
    if should_use_native_ssh(connection) {
        let connection = connection.clone();
        let command = remote_command.to_string();
        let timeout_duration_copy = timeout_duration;
        return tokio::task::spawn_blocking(move || {
            let connected = connect_ssh2_session(&connection, timeout_duration_copy)?;
            let mut channel = connected
                .session
                .channel_session()
                .map_err(|e| format!("创建命令通道失败: {e}"))?;
            channel
                .exec(command.as_str())
                .map_err(|e| format!("执行远端命令失败: {e}"))?;

            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            channel
                .read_to_end(&mut stdout)
                .map_err(|e| format!("读取标准输出失败: {e}"))?;
            channel
                .stderr()
                .read_to_end(&mut stderr)
                .map_err(|e| format!("读取标准错误失败: {e}"))?;
            let _ = channel.wait_close();
            let code = channel.exit_status().unwrap_or(0);

            if code == 0 {
                Ok(String::from_utf8_lossy(&stdout).to_string())
            } else {
                let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
                let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
                if !stderr_text.is_empty() {
                    Err(stderr_text)
                } else if !stdout_text.is_empty() {
                    Err(stdout_text)
                } else {
                    Err(format!("SSH 命令失败，exit={code}"))
                }
            }
        })
        .await
        .map_err(|e| format!("命令线程执行失败: {e}"))?;
    }

    let mut cmd = build_ssh_process_command(connection)?;
    let password_auth = is_password_auth(connection);
    cmd.args(build_ssh_args(connection, false, None));
    cmd.arg(remote_command);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = timeout(timeout_duration, cmd.output())
        .await
        .map_err(|_| "SSH 命令执行超时".to_string())?
        .map_err(|e| map_command_spawn_error("SSH 命令执行失败", e, password_auth))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("SSH 命令失败，exit={}", output.status))
    } else {
        Err(stderr)
    }
}

const SFTP_TRANSFER_CANCELLED: &str = "__CHATOS_TRANSFER_CANCELLED__";

fn transfer_cancelled_error() -> String {
    SFTP_TRANSFER_CANCELLED.to_string()
}

fn is_transfer_cancelled_error(error: &str) -> bool {
    error == SFTP_TRANSFER_CANCELLED
}

fn check_transfer_not_cancelled(
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), String> {
    if transfer_manager.is_cancel_requested(transfer_id) {
        return Err(transfer_cancelled_error());
    }
    Ok(())
}

fn estimate_local_total_bytes(path: &FsPath) -> Result<u64, String> {
    if path.is_file() {
        return path
            .metadata()
            .map(|meta| meta.len())
            .map_err(|e| format!("读取本地文件信息失败: {e}"));
    }
    if path.is_dir() {
        let mut total: u64 = 0;
        for entry in WalkDir::new(path) {
            let entry = entry.map_err(|e| format!("扫描本地目录失败: {e}"))?;
            if entry.file_type().is_file() {
                total = total.saturating_add(
                    entry
                        .metadata()
                        .map_err(|e| format!("读取本地文件信息失败: {e}"))?
                        .len(),
                );
            }
        }
        return Ok(total);
    }
    Err("本地路径必须是文件或目录".to_string())
}

fn remote_pathbuf_to_string(path: &PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn ensure_remote_dir_recursive(sftp: &ssh2::Sftp, dir_path: &str) -> Result<(), String> {
    let normalized = normalize_remote_path(dir_path);
    if normalized == "." || normalized == "/" {
        return Ok(());
    }

    let is_absolute = normalized.starts_with('/');
    let mut current = String::new();
    if is_absolute {
        current.push('/');
    }

    for segment in normalized
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
    {
        if !current.is_empty() && !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(segment);

        let current_path = FsPath::new(current.as_str());
        match sftp.stat(current_path) {
            Ok(stat) => {
                if !stat.is_dir() {
                    return Err(format!("远端路径不是目录: {}", current));
                }
            }
            Err(_) => {
                if let Err(err) = sftp.mkdir(current_path, 0o755) {
                    match sftp.stat(current_path) {
                        Ok(stat) if stat.is_dir() => {}
                        _ => return Err(format!("创建远端目录失败 ({}): {err}", current)),
                    }
                }
            }
        }
    }

    Ok(())
}

fn copy_local_file_to_remote_with_progress(
    sftp: &ssh2::Sftp,
    local_path: &FsPath,
    remote_path: &str,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), String> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if let Some(parent) = remote_parent_path(remote_path) {
        ensure_remote_dir_recursive(sftp, parent.as_str())?;
    }

    let mut local_file =
        std::fs::File::open(local_path).map_err(|e| format!("读取本地文件失败: {e}"))?;
    let mut remote_file = sftp
        .open_mode(
            FsPath::new(remote_path),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .map_err(|e| format!("打开远端文件失败: {e}"))?;

    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let n = local_file
            .read(&mut buffer)
            .map_err(|e| format!("读取本地文件失败: {e}"))?;
        if n == 0 {
            break;
        }
        remote_file
            .write_all(&buffer[..n])
            .map_err(|e| format!("写入远端文件失败: {e}"))?;
        *transferred_bytes = transferred_bytes.saturating_add(n as u64);
        transfer_manager.set_progress(
            transfer_id,
            *transferred_bytes,
            Some(total_bytes),
            Some(local_path.to_string_lossy().to_string()),
        );
    }

    Ok(())
}

fn upload_path_recursive_with_progress(
    sftp: &ssh2::Sftp,
    local_path: &FsPath,
    remote_path: &str,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), String> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if local_path.is_file() {
        return copy_local_file_to_remote_with_progress(
            sftp,
            local_path,
            remote_path,
            total_bytes,
            transferred_bytes,
            transfer_id,
            transfer_manager,
        );
    }

    if local_path.is_dir() {
        ensure_remote_dir_recursive(sftp, remote_path)?;
        let entries =
            std::fs::read_dir(local_path).map_err(|e| format!("读取本地目录失败: {e}"))?;
        for entry in entries {
            check_transfer_not_cancelled(transfer_id, transfer_manager)?;
            let entry = entry.map_err(|e| format!("读取本地目录失败: {e}"))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let child_local = entry.path();
            let child_remote = join_remote_path(remote_path, name.as_str());
            upload_path_recursive_with_progress(
                sftp,
                child_local.as_path(),
                child_remote.as_str(),
                total_bytes,
                transferred_bytes,
                transfer_id,
                transfer_manager,
            )?;
        }
        return Ok(());
    }

    Err("本地路径必须是文件或目录".to_string())
}

fn compute_remote_total_bytes_with_stat(
    sftp: &ssh2::Sftp,
    remote_path: &str,
    stat: &ssh2::FileStat,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<u64, String> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if stat.is_file() {
        return Ok(stat.size.unwrap_or(0));
    }
    if stat.is_dir() {
        let mut total = 0u64;
        let entries = sftp
            .readdir(FsPath::new(remote_path))
            .map_err(|e| format!("读取远端目录失败: {e}"))?;
        for (entry_path, entry_stat) in entries {
            check_transfer_not_cancelled(transfer_id, transfer_manager)?;
            let child_remote = remote_pathbuf_to_string(&entry_path);
            total = total.saturating_add(compute_remote_total_bytes_with_stat(
                sftp,
                child_remote.as_str(),
                &entry_stat,
                transfer_id,
                transfer_manager,
            )?);
        }
        return Ok(total);
    }
    Ok(0)
}

fn copy_remote_file_to_local_with_progress(
    sftp: &ssh2::Sftp,
    remote_path: &str,
    local_path: &FsPath,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), String> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建本地目录失败: {e}"))?;
    }

    let mut remote_file = sftp
        .open(FsPath::new(remote_path))
        .map_err(|e| format!("读取远端文件失败: {e}"))?;
    let mut local_file =
        std::fs::File::create(local_path).map_err(|e| format!("创建本地文件失败: {e}"))?;

    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let n = remote_file
            .read(&mut buffer)
            .map_err(|e| format!("读取远端文件失败: {e}"))?;
        if n == 0 {
            break;
        }
        local_file
            .write_all(&buffer[..n])
            .map_err(|e| format!("写入本地文件失败: {e}"))?;
        *transferred_bytes = transferred_bytes.saturating_add(n as u64);
        transfer_manager.set_progress(
            transfer_id,
            *transferred_bytes,
            Some(total_bytes),
            Some(remote_path.to_string()),
        );
    }

    Ok(())
}

fn download_remote_path_with_progress(
    sftp: &ssh2::Sftp,
    remote_path: &str,
    remote_stat: &ssh2::FileStat,
    local_path: &FsPath,
    total_bytes: u64,
    transferred_bytes: &mut u64,
    transfer_id: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<(), String> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    if remote_stat.is_file() {
        return copy_remote_file_to_local_with_progress(
            sftp,
            remote_path,
            local_path,
            total_bytes,
            transferred_bytes,
            transfer_id,
            transfer_manager,
        );
    }

    if remote_stat.is_dir() {
        if local_path.exists() && !local_path.is_dir() {
            return Err("本地目标已存在且不是目录".to_string());
        }
        std::fs::create_dir_all(local_path).map_err(|e| format!("创建本地目录失败: {e}"))?;
        let entries = sftp
            .readdir(FsPath::new(remote_path))
            .map_err(|e| format!("读取远端目录失败: {e}"))?;

        for (entry_path, entry_stat) in entries {
            check_transfer_not_cancelled(transfer_id, transfer_manager)?;
            let entry_name = match entry_path.file_name().and_then(|v| v.to_str()) {
                Some(v) => v.to_string(),
                None => continue,
            };
            let child_remote = remote_pathbuf_to_string(&entry_path);
            let child_local = local_path.join(entry_name);
            download_remote_path_with_progress(
                sftp,
                child_remote.as_str(),
                &entry_stat,
                child_local.as_path(),
                total_bytes,
                transferred_bytes,
                transfer_id,
                transfer_manager,
            )?;
        }
        return Ok(());
    }

    Err("远端路径既不是文件也不是目录".to_string())
}

fn run_sftp_transfer_job(
    connection: &RemoteConnection,
    transfer_id: &str,
    direction: &str,
    local_path: &str,
    remote_path: &str,
    transfer_manager: &SftpTransferManager,
) -> Result<String, String> {
    check_transfer_not_cancelled(transfer_id, transfer_manager)?;
    let connected = connect_ssh2_session(connection, Duration::from_secs(20))?;
    let sftp = connected
        .session
        .sftp()
        .map_err(|e| format!("初始化 SFTP 失败: {e}"))?;

    if direction == "upload" {
        let source = FsPath::new(local_path);
        if !source.exists() {
            return Err("本地路径不存在".to_string());
        }
        let total_bytes = estimate_local_total_bytes(source)?;
        let mut transferred_bytes = 0u64;
        transfer_manager.set_progress(
            transfer_id,
            0,
            Some(total_bytes),
            Some(local_path.to_string()),
        );
        upload_path_recursive_with_progress(
            &sftp,
            source,
            remote_path,
            total_bytes,
            &mut transferred_bytes,
            transfer_id,
            transfer_manager,
        )?;
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let summary = if source.is_dir() {
            "目录上传完成".to_string()
        } else {
            "文件上传完成".to_string()
        };
        return Ok(summary);
    }

    if direction == "download" {
        let remote_stat = sftp
            .stat(FsPath::new(remote_path))
            .map_err(|e| format!("读取远端路径信息失败: {e}"))?;
        let total_bytes = compute_remote_total_bytes_with_stat(
            &sftp,
            remote_path,
            &remote_stat,
            transfer_id,
            transfer_manager,
        )?;
        let mut transferred_bytes = 0u64;
        transfer_manager.set_progress(
            transfer_id,
            0,
            Some(total_bytes),
            Some(remote_path.to_string()),
        );
        download_remote_path_with_progress(
            &sftp,
            remote_path,
            &remote_stat,
            FsPath::new(local_path),
            total_bytes,
            &mut transferred_bytes,
            transfer_id,
            transfer_manager,
        )?;
        check_transfer_not_cancelled(transfer_id, transfer_manager)?;
        let summary = if remote_stat.is_dir() {
            "目录下载完成".to_string()
        } else {
            "文件下载完成".to_string()
        };
        return Ok(summary);
    }

    Err("direction 仅支持 upload 或 download".to_string())
}

async fn run_scp_upload(
    connection: &RemoteConnection,
    local_path: &str,
    remote_path: &str,
) -> Result<(), String> {
    if should_use_native_ssh(connection) {
        let connection = connection.clone();
        let local = local_path.to_string();
        let remote = remote_path.to_string();
        return tokio::task::spawn_blocking(move || {
            let connected = connect_ssh2_session(&connection, Duration::from_secs(15))?;
            let sftp = connected
                .session
                .sftp()
                .map_err(|e| format!("初始化 SFTP 失败: {e}"))?;
            let mut local_file = std::fs::File::open(local.as_str())
                .map_err(|e| format!("读取本地文件失败: {e}"))?;
            let mut remote_file = sftp
                .open_mode(
                    FsPath::new(remote.as_str()),
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                    0o644,
                    OpenType::File,
                )
                .map_err(|e| format!("打开远端文件失败: {e}"))?;
            std::io::copy(&mut local_file, &mut remote_file)
                .map_err(|e| format!("上传文件失败: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("上传线程执行失败: {e}"))?;
    }

    let mut cmd = build_scp_process_command(connection)?;
    let password_auth = is_password_auth(connection);
    cmd.args(build_scp_args(connection));
    cmd.arg(local_path);
    cmd.arg(format!(
        "{}@{}:{}",
        connection.username,
        connection.host,
        shell_quote(remote_path)
    ));
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = timeout(Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| "上传超时".to_string())?
        .map_err(|e| map_command_spawn_error("上传失败", e, password_auth))?;

    if output.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

async fn run_scp_download(
    connection: &RemoteConnection,
    remote_path: &str,
    local_path: &str,
) -> Result<(), String> {
    if should_use_native_ssh(connection) {
        let connection = connection.clone();
        let local = local_path.to_string();
        let remote = remote_path.to_string();
        return tokio::task::spawn_blocking(move || {
            let connected = connect_ssh2_session(&connection, Duration::from_secs(15))?;
            let sftp = connected
                .session
                .sftp()
                .map_err(|e| format!("初始化 SFTP 失败: {e}"))?;
            let mut remote_file = sftp
                .open(FsPath::new(remote.as_str()))
                .map_err(|e| format!("读取远端文件失败: {e}"))?;
            let mut local_file = std::fs::File::create(local.as_str())
                .map_err(|e| format!("创建本地文件失败: {e}"))?;
            std::io::copy(&mut remote_file, &mut local_file)
                .map_err(|e| format!("下载文件失败: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("下载线程执行失败: {e}"))?;
    }

    let mut cmd = build_scp_process_command(connection)?;
    let password_auth = is_password_auth(connection);
    cmd.args(build_scp_args(connection));
    cmd.arg(format!(
        "{}@{}:{}",
        connection.username,
        connection.host,
        shell_quote(remote_path)
    ));
    cmd.arg(local_path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = timeout(Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| "下载超时".to_string())?
        .map_err(|e| map_command_spawn_error("下载失败", e, password_auth))?;

    if output.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

async fn run_remote_connectivity_test(connection: &RemoteConnection) -> Result<Value, String> {
    let script = "printf '__CHATOS_OK__\\n'; uname -n 2>/dev/null || hostname";
    let output = run_ssh_command(connection, script, Duration::from_secs(12)).await?;
    if !output.contains("__CHATOS_OK__") {
        return Err("远端未返回预期握手标识".to_string());
    }

    let host_line = output
        .lines()
        .filter(|line| !line.contains("__CHATOS_OK__"))
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| connection.host.clone());

    Ok(serde_json::json!({
        "success": true,
        "remote_host": host_line,
        "connected_at": crate::core::time::now_rfc3339(),
    }))
}

fn shell_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(ch);
        }
    }
    escaped.push('\'');
    escaped
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return ".".to_string();
    }

    let mut compact = trimmed.replace("\\", "/");
    while compact.contains("//") {
        compact = compact.replace("//", "/");
    }

    if compact != "/" {
        compact = compact.trim_end_matches('/').to_string();
    }

    if compact.is_empty() {
        ".".to_string()
    } else {
        compact
    }
}

fn join_remote_path(parent: &str, name: &str) -> String {
    let parent = normalize_remote_path(parent);
    if parent == "." {
        return name.to_string();
    }
    if parent == "/" {
        return format!("/{name}");
    }
    format!("{parent}/{name}")
}

fn remote_parent_path(path: &str) -> Option<String> {
    let path = normalize_remote_path(path);
    if path == "." || path == "/" {
        return None;
    }

    let mut parts = path
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    parts.pop();
    if parts.is_empty() {
        if path.starts_with('/') {
            Some("/".to_string())
        } else {
            Some(".".to_string())
        }
    } else if path.starts_with('/') {
        Some(format!("/{}", parts.join("/")))
    } else {
        Some(parts.join("/"))
    }
}

fn input_triggers_busy(data: &str) -> bool {
    if data.is_empty() {
        return false;
    }
    if data.contains('\r') || data.contains('\n') {
        return true;
    }
    data.as_bytes()
        .iter()
        .any(|b| matches!(*b, 0x03 | 0x04 | 0x1A))
}
