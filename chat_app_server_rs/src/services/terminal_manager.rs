use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use dashmap::DashMap;
use once_cell::sync::OnceCell;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize, SlavePty};
use tokio::sync::broadcast;

use crate::models::terminal::Terminal;
use crate::models::terminal_log::TerminalLog;
use crate::repositories::{terminal_logs, terminals};

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Output(String),
    Exit(i32),
    State(bool),
}

pub struct TerminalSession {
    pub id: String,
    sender: broadcast::Sender<TerminalEvent>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    busy: AtomicBool,
    last_input_at: AtomicU64,
    last_output_at: AtomicU64,
    last_prompt_at: AtomicU64,
}

impl TerminalSession {
    fn new(terminal: &Terminal) -> Result<(Arc<Self>, Box<dyn portable_pty::Child + Send + Sync>), String> {
        let cwd = terminal.cwd.clone();
        if !Path::new(&cwd).exists() {
            return Err("cwd does not exist".to_string());
        }

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("open pty failed: {e}"))?;

        let child = spawn_shell(&cwd, pair.slave)?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("clone reader failed: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("take writer failed: {e}"))?;

        let (sender, _) = broadcast::channel(1024);

        let session = Arc::new(TerminalSession {
            id: terminal.id.clone(),
            sender,
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            busy: AtomicBool::new(false),
            last_input_at: AtomicU64::new(0),
            last_output_at: AtomicU64::new(0),
            last_prompt_at: AtomicU64::new(0),
        });

        let session_clone = session.clone();
        let handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut line_buffer = String::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = session_clone.sender.send(TerminalEvent::Output(text.clone()));
                        session_clone.mark_output();
                        let cleaned = strip_ansi(&text);
                        if !cleaned.is_empty() {
                            line_buffer.push_str(&cleaned);
                            let mut parts = line_buffer.split('\n').collect::<Vec<_>>();
                            let tail = parts.pop().unwrap_or("");
                            let mut saw_prompt = false;
                            for line in parts.iter() {
                                if is_prompt_line(line) {
                                    saw_prompt = true;
                                    break;
                                }
                            }
                            line_buffer = tail.to_string();
                            if !saw_prompt && is_prompt_line(line_buffer.as_str()) {
                                saw_prompt = true;
                            }
                            if saw_prompt {
                                session_clone.mark_prompt();
                            }
                        }
                        let terminal_id = session_clone.id.clone();
                        let handle = handle.clone();
                        handle.spawn(async move {
                            let _ = terminals::touch_terminal(&terminal_id).await;
                            let log = TerminalLog::new(terminal_id, "output".to_string(), text);
                            let _ = terminal_logs::create_terminal_log(&log).await;
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        Ok((session, child))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.sender.subscribe()
    }

    pub fn write_input(&self, data: &str) -> Result<(), String> {
        self.mark_input();
        let mut writer = self.writer.lock().map_err(|_| "writer lock failed".to_string())?;
        writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("write failed: {e}"))?;
        writer.flush().map_err(|e| format!("flush failed: {e}"))?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let master = self.master.lock().map_err(|_| "master lock failed".to_string())?;
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

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }

    fn mark_input(&self) {
        self.last_input_at.store(now_millis(), Ordering::Relaxed);
        self.set_busy(true);
    }

    fn mark_output(&self) {
        self.last_output_at.store(now_millis(), Ordering::Relaxed);
    }

    fn mark_prompt(&self) {
        self.last_prompt_at.store(now_millis(), Ordering::Relaxed);
        self.set_busy(false);
    }

    fn set_busy(&self, busy: bool) {
        let prev = self.busy.swap(busy, Ordering::Relaxed);
        if prev != busy {
            let _ = self.sender.send(TerminalEvent::State(busy));
        }
    }
}

pub struct TerminalsManager {
    sessions: DashMap<String, Arc<TerminalSession>>,
}

impl TerminalsManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<TerminalSession>> {
        self.sessions.get(id).map(|s| s.clone())
    }

    pub fn get_busy(&self, id: &str) -> Option<bool> {
        self.sessions.get(id).map(|s| s.is_busy())
    }

    fn spawn_session(&self, terminal: &Terminal) -> Result<Arc<TerminalSession>, String> {
        let (session, mut child) = TerminalSession::new(terminal)?;
        let id = terminal.id.clone();
        let sender = session.sender.clone();
        let handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code()).unwrap_or(0) as i32;
            let _ = sender.send(TerminalEvent::Exit(code));
            let id_clone = id.clone();
            let handle = handle.clone();
            handle.spawn(async move {
                let _ = terminals::update_terminal_status(&id_clone, Some("exited".to_string()), None).await;
            });
        });
        self.sessions.insert(terminal.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn create(&self, name: String, cwd: String, user_id: Option<String>) -> Result<Terminal, String> {
        let terminal = Terminal::new(name, cwd, user_id);
        terminals::create_terminal(&terminal).await?;
        let _ = self.spawn_session(&terminal)?;
        Ok(terminal)
    }

    pub async fn ensure_running(&self, terminal: &Terminal) -> Result<Arc<TerminalSession>, String> {
        if let Some(session) = self.get(&terminal.id) {
            return Ok(session);
        }
        let session = self.spawn_session(terminal)?;
        let _ = terminals::update_terminal_status(&terminal.id, Some("running".to_string()), None).await;
        Ok(session)
    }

    pub async fn close(&self, id: &str) -> Result<(), String> {
        if let Some(session) = self.sessions.remove(id).map(|(_, s)| s) {
            let _ = session.write_input("exit\n");
        }
        terminals::update_terminal_status(id, Some("exited".to_string()), None).await?;
        Ok(())
    }
}

static TERMINAL_MANAGER: OnceCell<Arc<TerminalsManager>> = OnceCell::new();

pub fn get_terminal_manager() -> Arc<TerminalsManager> {
    TERMINAL_MANAGER.get_or_init(|| Arc::new(TerminalsManager::new())).clone()
}

fn spawn_shell(cwd: &str, slave: Box<dyn SlavePty + Send>) -> Result<Box<dyn portable_pty::Child + Send + Sync>, String> {
    let shell = select_shell();
    let mut cmd = CommandBuilder::new(shell.clone());
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    slave.spawn_command(cmd).map_err(|e| format!("{shell}: {e}"))
}

fn select_shell() -> String {
    if cfg!(windows) {
        if let Some(path) = find_in_path(&["pwsh.exe", "pwsh"]) {
            return path;
        }
        if let Some(path) = find_in_path(&["powershell.exe", "powershell"]) {
            return path;
        }
        if let Ok(comspec) = std::env::var("COMSPEC") {
            if !comspec.trim().is_empty() {
                return comspec;
            }
        }
        return "cmd.exe".to_string();
    }

    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.trim().is_empty() {
            return shell;
        }
    }
    if Path::new("/bin/bash").exists() {
        return "/bin/bash".to_string();
    }
    if Path::new("/bin/zsh").exists() {
        return "/bin/zsh".to_string();
    }
    "/bin/sh".to_string()
}

fn find_in_path(candidates: &[&str]) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in std::env::split_paths(&path_var) {
        for name in candidates {
            let full = dir.join(name);
            if full.exists() {
                return Some(full.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn strip_ansi(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    static ANSI_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").unwrap()
    });
    ANSI_RE.replace_all(input, "").to_string()
}

fn is_prompt_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    static PROMPT_PATTERNS: once_cell::sync::Lazy<Vec<regex::Regex>> = once_cell::sync::Lazy::new(|| {
        vec![
            regex::Regex::new(r"^\\([^)]+\\)\\s?.*[#$%>] ?$").unwrap(),
            regex::Regex::new(r"^[^\\n\\r]*@[^\\n\\r]*[#$%>] ?$").unwrap(),
            regex::Regex::new(r"^PS [A-Za-z]:\\\\.*> ?$").unwrap(),
            regex::Regex::new(r"^[A-Za-z]:\\\\.*> ?$").unwrap(),
            regex::Regex::new(r"^.*\\$\\s?$").unwrap(),
            regex::Regex::new(r"^.*%\\s?$").unwrap(),
            regex::Regex::new(r"^.*>\\s?$").unwrap(),
        ]
    });
    PROMPT_PATTERNS.iter().any(|re| re.is_match(trimmed))
}
