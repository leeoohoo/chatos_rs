use dashmap::DashMap;
use once_cell::sync::OnceCell;
use portable_pty::{native_pty_system, MasterPty, PtySize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::broadcast;
use tokio::time::Duration;

use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

use super::terminal_io::{
    is_io_would_block, request_pty_resize_nonblocking, write_channel_nonblocking,
};
use super::{
    connect_ssh2_session_with_verification, input_triggers_busy, is_password_auth,
    should_use_native_ssh, spawn_remote_shell,
};

const REMOTE_TERMINAL_SNAPSHOT_LIMIT_BYTES: usize = 512 * 1024;
const REMOTE_TERMINAL_IDLE_TIMEOUT: StdDuration = StdDuration::from_secs(20 * 60);
const REMOTE_TERMINAL_IDLE_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
pub(super) enum DisconnectReason {
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

#[derive(Debug, Clone)]
pub(super) enum RemoteTerminalEvent {
    Output(String),
    Exit(i32),
    State(bool),
}

enum NativeTerminalControl {
    Input(String),
    Resize { cols: u16, rows: u16 },
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

pub(super) struct RemoteTerminalSession {
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
        verification_code: Option<&str>,
    ) -> Result<
        (
            Arc<Self>,
            Option<Box<dyn portable_pty::Child + Send + Sync>>,
        ),
        String,
    > {
        if is_password_auth(connection) {
            // Keep password/OTP auth in native ssh2 path so second-factor challenges are
            // surfaced to frontend and can trigger the verification modal.
            return Self::new_native(connection, verification_code);
        }
        if should_use_native_ssh(connection) {
            Self::new_native(connection, verification_code)
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
        verification_code: Option<&str>,
    ) -> Result<
        (
            Arc<Self>,
            Option<Box<dyn portable_pty::Child + Send + Sync>>,
        ),
        String,
    > {
        let connected = connect_ssh2_session_with_verification(
            connection,
            Duration::from_secs(12),
            verification_code,
        )?;
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

    pub(super) fn subscribe(&self) -> broadcast::Receiver<RemoteTerminalEvent> {
        self.sender.subscribe()
    }

    pub(super) fn write_input(&self, data: &str) -> Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }
        self.touch_activity();
        self.set_busy(input_triggers_busy(data));
        self.send_control_input(data)
    }

    pub(super) fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
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

    pub(super) fn touch_activity(&self) {
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

    pub(super) fn is_busy(&self) -> bool {
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

    pub(super) fn output_snapshot(&self) -> String {
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

pub(super) struct RemoteTerminalManager {
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

    pub(super) async fn ensure_running(
        &self,
        connection: &RemoteConnection,
        verification_code: Option<&str>,
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

        let (session, child) = RemoteTerminalSession::new(connection, verification_code)?;
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

    pub(super) fn close_with_reason(&self, connection_id: &str, reason: DisconnectReason) -> bool {
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

static REMOTE_TERMINAL_MANAGER: OnceCell<Arc<RemoteTerminalManager>> = OnceCell::new();

pub(super) fn get_remote_terminal_manager() -> Arc<RemoteTerminalManager> {
    REMOTE_TERMINAL_MANAGER
        .get_or_init(|| {
            let manager = Arc::new(RemoteTerminalManager::new());
            RemoteTerminalManager::spawn_idle_sweeper(manager.clone());
            manager
        })
        .clone()
}
