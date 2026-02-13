use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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
    root_cwd: PathBuf,
    current_cwd: Mutex<PathBuf>,
    input_line: Mutex<String>,
    busy: AtomicBool,
    root_reset_in_progress: AtomicBool,
    last_input_at: AtomicU64,
    last_output_at: AtomicU64,
    last_prompt_at: AtomicU64,
}

impl TerminalSession {
    fn new(
        terminal: &Terminal,
    ) -> Result<(Arc<Self>, Box<dyn portable_pty::Child + Send + Sync>), String> {
        let cwd = terminal.cwd.clone();
        if !Path::new(&cwd).exists() {
            return Err("cwd does not exist".to_string());
        }

        let root_cwd = canonicalize_path(Path::new(&cwd))
            .map_err(|e| format!("canonicalize cwd failed: {e}"))?;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("open pty failed: {e}"))?;

        let child = spawn_shell(root_cwd.as_path(), pair.slave)?;

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
            root_cwd: root_cwd.clone(),
            current_cwd: Mutex::new(root_cwd),
            input_line: Mutex::new(String::new()),
            busy: AtomicBool::new(false),
            root_reset_in_progress: AtomicBool::new(false),
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
                        let _ = session_clone
                            .sender
                            .send(TerminalEvent::Output(text.clone()));
                        session_clone.mark_output();
                        let cleaned = strip_ansi(&text);
                        if !cleaned.is_empty() {
                            line_buffer.push_str(&cleaned);
                            let mut parts = line_buffer.split('\n').collect::<Vec<_>>();
                            let tail = parts.pop().unwrap_or("");
                            let mut saw_prompt = false;
                            for line in parts.iter() {
                                session_clone.sync_current_cwd_from_prompt_line(line);
                                if is_prompt_line(line) {
                                    saw_prompt = true;
                                }
                            }
                            line_buffer = tail.to_string();
                            session_clone.sync_current_cwd_from_prompt_line(line_buffer.as_str());
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
        let (forward_data, blocked_messages) = self.apply_directory_guard(data);
        self.mark_input(&forward_data);

        if !forward_data.is_empty() {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| "writer lock failed".to_string())?;
            writer
                .write_all(forward_data.as_bytes())
                .map_err(|e| format!("write failed: {e}"))?;
            writer.flush().map_err(|e| format!("flush failed: {e}"))?;
        }

        for message in blocked_messages {
            self.emit_guard_output(&message);
        }

        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let master = self
            .master
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

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }

    fn apply_directory_guard(&self, data: &str) -> (String, Vec<String>) {
        if data.is_empty() {
            return (String::new(), Vec::new());
        }

        let mut line = match self.input_line.lock() {
            Ok(guard) => guard,
            Err(_) => return (data.to_string(), Vec::new()),
        };
        let mut current_cwd = match self.current_cwd.lock() {
            Ok(guard) => guard,
            Err(_) => return (data.to_string(), Vec::new()),
        };

        let mut forward = String::with_capacity(data.len());
        let mut blocked = Vec::new();
        let mut skip_following_lf = false;

        for ch in data.chars() {
            if skip_following_lf && ch != '\n' {
                skip_following_lf = false;
            }

            match ch {
                '\r' | '\n' => {
                    if skip_following_lf && ch == '\n' {
                        skip_following_lf = false;
                        continue;
                    }

                    let command_line = line.clone();
                    let sanitized_command = sanitize_command_line_for_guard(command_line.as_str());
                    line.clear();
                    if let Some(reason) = validate_directory_change_command(
                        sanitized_command.as_str(),
                        self.root_cwd.as_path(),
                        &mut current_cwd,
                    ) {
                        forward.push_str(
                            clear_input_line_sequence(sanitized_command.as_str()).as_str(),
                        );
                        skip_following_lf = ch == '\r';
                        blocked.push(reason);
                        continue;
                    }
                    forward.push(ch);
                }
                c if c as u32 == 27 => {
                    // Keep ESC in the line buffer so ANSI control sequences can be
                    // removed before parsing directory-change commands.
                    line.push(ch);
                    forward.push(ch);
                }
                '\u{8}' | '\u{7f}' => {
                    line.pop();
                    forward.push(ch);
                }
                '\u{15}' => {
                    line.clear();
                    forward.push(ch);
                }
                '\u{3}' | '\u{4}' | '\u{1a}' => {
                    line.clear();
                    forward.push(ch);
                }
                _ if ch.is_control() => {
                    forward.push(ch);
                }
                _ => {
                    line.push(ch);
                    forward.push(ch);
                }
            }
        }

        (forward, blocked)
    }

    fn emit_guard_output(&self, message: &str) {
        let output = format!("\r\n{message}\r\n");
        let _ = self.sender.send(TerminalEvent::Output(output));
    }

    fn sync_current_cwd_from_prompt_line(&self, line: &str) {
        let parsed_cwd = extract_prompt_cwd(line).or_else(|| {
            let current = self.current_cwd.lock().ok()?.clone();
            infer_prompt_cwd_from_context(line, current.as_path(), self.root_cwd.as_path())
        });

        let Some(parsed_cwd) = parsed_cwd else {
            return;
        };

        if !path_is_within_root(parsed_cwd.as_path(), self.root_cwd.as_path()) {
            self.reset_shell_to_root(parsed_cwd.as_path());
            return;
        }

        self.root_reset_in_progress.store(false, Ordering::Relaxed);

        if let Ok(mut cwd_guard) = self.current_cwd.lock() {
            *cwd_guard = parsed_cwd;
        }
    }

    fn reset_shell_to_root(&self, escaped_cwd: &Path) {
        if self.root_reset_in_progress.swap(true, Ordering::Relaxed) {
            return;
        }

        self.emit_guard_output(
            format!(
                "Blocked: path escaped terminal root ({}). Resetting to {}",
                escaped_cwd.display(),
                self.root_cwd.display()
            )
            .as_str(),
        );

        if let Ok(mut line) = self.input_line.lock() {
            line.clear();
        }
        if let Ok(mut cwd_guard) = self.current_cwd.lock() {
            *cwd_guard = self.root_cwd.clone();
        }

        let restore = build_return_to_root_command(self.root_cwd.as_path());
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(restore.as_bytes());
            let _ = writer.flush();
        }
    }

    fn mark_input(&self, data: &str) {
        self.last_input_at.store(now_millis(), Ordering::Relaxed);
        if input_triggers_busy(data) {
            self.set_busy(true);
        }
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
                let _ =
                    terminals::update_terminal_status(&id_clone, Some("exited".to_string()), None)
                        .await;
            });
        });
        self.sessions.insert(terminal.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn create(
        &self,
        name: String,
        cwd: String,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Result<Terminal, String> {
        let terminal = Terminal::new(name, cwd, user_id, project_id);
        terminals::create_terminal(&terminal).await?;
        let _ = self.spawn_session(&terminal)?;
        Ok(terminal)
    }

    pub async fn ensure_running(
        &self,
        terminal: &Terminal,
    ) -> Result<Arc<TerminalSession>, String> {
        if let Some(session) = self.get(&terminal.id) {
            return Ok(session);
        }
        let session = self.spawn_session(terminal)?;
        let _ = terminals::update_terminal_status(&terminal.id, Some("running".to_string()), None)
            .await;
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
    TERMINAL_MANAGER
        .get_or_init(|| Arc::new(TerminalsManager::new()))
        .clone()
}

fn spawn_shell(
    cwd: &Path,
    slave: Box<dyn SlavePty + Send>,
) -> Result<Box<dyn portable_pty::Child + Send + Sync>, String> {
    let shell = select_shell();
    let mut cmd = CommandBuilder::new(shell.clone());
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    slave
        .spawn_command(cmd)
        .map_err(|e| format!("{shell}: {e}"))
}

fn select_shell() -> String {
    if cfg!(windows) {
        if let Ok(comspec) = std::env::var("COMSPEC") {
            let trimmed = comspec.trim();
            if !trimmed.is_empty() && Path::new(trimmed).exists() {
                return trimmed.to_string();
            }
        }
        if let Some(path) = find_in_path(&["cmd.exe", "cmd"]) {
            return path;
        }
        if let Some(path) = find_in_path(&["pwsh.exe", "pwsh"]) {
            return path;
        }
        if let Some(path) = find_in_path(&["powershell.exe", "powershell"]) {
            return path;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirChangeKind {
    Cd,
    SetLocation,
    Pushd,
    Popd,
}

#[derive(Debug, Clone)]
struct DirChangeCommand {
    kind: DirChangeKind,
    target: Option<String>,
    has_extra_args: bool,
}

fn validate_directory_change_command(
    line: &str,
    root_cwd: &Path,
    current_cwd: &mut PathBuf,
) -> Option<String> {
    let command = parse_directory_change_command(line)?;

    let target_is_absolute = command
        .target
        .as_deref()
        .map(|t| Path::new(t.trim()).is_absolute())
        .unwrap_or(false);

    if command.has_extra_args {
        return Some(
            "Blocked: run directory-change commands alone (no chained arguments).".to_string(),
        );
    }

    if matches!(command.kind, DirChangeKind::Pushd | DirChangeKind::Popd) {
        return Some("Blocked: pushd/popd are disabled for this restricted terminal.".to_string());
    }

    if let Some(target) = command.target.as_deref() {
        let target = target.trim();
        if target == "-" {
            return Some("Blocked: cd - is disabled in this restricted terminal.".to_string());
        }
        if has_dynamic_cd_syntax(target) {
            return Some(
                "Blocked: cd path cannot contain shell expansions or control operators."
                    .to_string(),
            );
        }
    }

    let resolved =
        match resolve_cd_target(root_cwd, current_cwd.as_path(), command.target.as_deref()) {
            Some(path) => path,
            None => {
                if target_is_absolute {
                    return Some(
                        "Blocked: cannot verify absolute cd target (path does not resolve)."
                            .to_string(),
                    );
                }
                return None;
            }
        };

    if !path_is_within_root(resolved.as_path(), root_cwd) {
        let root = root_cwd.display();
        return Some(format!(
            "Blocked: cannot leave terminal root directory: {root}"
        ));
    }

    *current_cwd = resolved;
    None
}

fn parse_directory_change_command(line: &str) -> Option<DirChangeCommand> {
    let words = split_shell_words(line.trim())?;
    if words.is_empty() {
        return None;
    }

    let command = words[0].to_ascii_lowercase();
    match command.as_str() {
        "cd" | "chdir" => parse_cd_command(words),
        "set-location" | "sl" => parse_set_location_command(words),
        "pushd" => Some(DirChangeCommand {
            kind: DirChangeKind::Pushd,
            target: words.get(1).cloned(),
            has_extra_args: words.len() > 2,
        }),
        "popd" => Some(DirChangeCommand {
            kind: DirChangeKind::Popd,
            target: None,
            has_extra_args: words.len() > 1,
        }),
        _ => None,
    }
}

fn parse_cd_command(words: Vec<String>) -> Option<DirChangeCommand> {
    let mut idx = 1;
    if idx < words.len() && words[idx].eq_ignore_ascii_case("/d") {
        idx += 1;
    }

    let target = if idx < words.len() {
        Some(words[idx].clone())
    } else {
        None
    };
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };

    Some(DirChangeCommand {
        kind: DirChangeKind::Cd,
        target,
        has_extra_args,
    })
}

fn parse_set_location_command(words: Vec<String>) -> Option<DirChangeCommand> {
    let mut idx = 1;
    if idx < words.len()
        && (words[idx].eq_ignore_ascii_case("-path")
            || words[idx].eq_ignore_ascii_case("-literalpath"))
    {
        idx += 1;
    }

    let target = if idx < words.len() {
        Some(words[idx].clone())
    } else {
        None
    };
    let has_extra_args = if target.is_some() {
        idx + 1 < words.len()
    } else {
        idx < words.len()
    };

    Some(DirChangeCommand {
        kind: DirChangeKind::SetLocation,
        target,
        has_extra_args,
    })
}

fn split_shell_words(input: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in input.chars() {
        match quote {
            Some(marker) => {
                if ch == marker {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                } else if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
        }
    }

    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        words.push(current);
    }
    Some(words)
}

fn extract_prompt_cwd(line: &str) -> Option<PathBuf> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    static POWERSHELL_PROMPT_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| {
            regex::Regex::new(r"(?:^|[\s\]])PS (?:[^:>\r\n]+::)?([A-Za-z]:\\[^>\r\n]*)> ?$")
                .unwrap()
        });
    static CMD_PROMPT_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"([A-Za-z]:\\[^>\r\n]*)> ?$").unwrap());
    static UNIX_PROMPT_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"(/[^#$%>\r\n]*)[#$%>] ?$").unwrap());

    if let Some(caps) = POWERSHELL_PROMPT_RE.captures(trimmed) {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }
    if let Some(caps) = CMD_PROMPT_RE.captures(trimmed) {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }
    if let Some(caps) = UNIX_PROMPT_RE.captures(trimmed) {
        return canonicalize_prompt_path(caps.get(1).map(|m| m.as_str())?);
    }

    None
}

fn canonicalize_prompt_path(raw_path: &str) -> Option<PathBuf> {
    let candidate = raw_path.trim();
    if candidate.is_empty() {
        return None;
    }

    let path = Path::new(candidate);
    if !path.is_absolute() {
        return None;
    }

    canonicalize_path(path).ok()
}

fn infer_prompt_cwd_from_context(
    line: &str,
    current_cwd: &Path,
    _root_cwd: &Path,
) -> Option<PathBuf> {
    let hint = extract_prompt_dir_hint(line)?;
    let normalized_hint = hint.trim_end_matches(|c| c == '/' || c as u32 == 92);
    if normalized_hint.is_empty() {
        return None;
    }

    if normalized_hint == "~" || normalized_hint.starts_with("~/") {
        let home = std::env::var("HOME").ok()?;
        let base = PathBuf::from(home);
        let candidate = if normalized_hint == "~" {
            base
        } else {
            let rel = normalized_hint.trim_start_matches("~/");
            base.join(rel)
        };
        return canonicalize_path(candidate.as_path()).ok();
    }

    if Path::new(normalized_hint).is_absolute() {
        return canonicalize_path(Path::new(normalized_hint)).ok();
    }

    if normalized_hint == "." {
        return Some(current_cwd.to_path_buf());
    }

    if normalized_hint == ".." {
        return canonicalize_path(current_cwd.parent()?).ok();
    }

    if current_cwd
        .file_name()
        .and_then(|n| n.to_str())
        .map(|name| name == normalized_hint)
        .unwrap_or(false)
    {
        return Some(current_cwd.to_path_buf());
    }

    if let Some(parent_raw) = current_cwd.parent() {
        if parent_raw
            .file_name()
            .and_then(|n| n.to_str())
            .map(|name| name == normalized_hint)
            .unwrap_or(false)
        {
            return canonicalize_path(parent_raw).ok();
        }
    }

    canonicalize_path(current_cwd.join(normalized_hint).as_path()).ok()
}

fn extract_prompt_dir_hint(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    let marker = trimmed.chars().last()?;
    if !matches!(marker, '$' | '#' | '%' | '>') {
        return None;
    }

    let without_marker = trimmed
        .get(..trimmed.len().saturating_sub(marker.len_utf8()))?
        .trim_end();
    let token = without_marker.split_whitespace().last()?.trim();
    if token.is_empty() {
        return None;
    }

    // Avoid guessing from tokens that are probably user/host prefixes.
    if token.contains('@') || token.contains(':') {
        return None;
    }

    Some(token.to_string())
}

fn clear_input_line_sequence(command_line: &str) -> String {
    let mut seq = String::new();
    for _ in 0..command_line.chars().count() {
        // Backspace + overwrite + backspace clears one character in most terminals.
        seq.push('\u{8}');
        seq.push(' ');
        seq.push('\u{8}');
    }
    seq
}

fn sanitize_command_line_for_guard(command_line: &str) -> String {
    if command_line.is_empty() {
        return String::new();
    }

    let stripped = strip_ansi(command_line);
    stripped.chars().filter(|ch| !ch.is_control()).collect()
}

fn has_dynamic_cd_syntax(target: &str) -> bool {
    let trimmed = target.trim();
    trimmed.starts_with('~')
        || trimmed
            .chars()
            .any(|ch| matches!(ch, '$' | '%' | '`' | ';' | '|' | '&' | '>' | '<'))
}

fn resolve_cd_target(root_cwd: &Path, current_cwd: &Path, target: Option<&str>) -> Option<PathBuf> {
    let raw_target = target.unwrap_or("").trim();

    if raw_target.is_empty() {
        return Some(root_cwd.to_path_buf());
    }

    let candidate = if Path::new(raw_target).is_absolute() {
        PathBuf::from(raw_target)
    } else {
        current_cwd.join(raw_target)
    };

    canonicalize_path(candidate.as_path()).ok()
}

fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    let candidate_norm = normalize_path_for_compare(candidate);
    let root_norm = normalize_path_for_compare(root);

    if candidate_norm == root_norm {
        return true;
    }

    let prefix = format!("{root_norm}/");
    candidate_norm.starts_with(&prefix)
}

fn normalize_path_for_compare(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");

    if let Some(stripped) = normalized.strip_prefix("//?/UNC/") {
        normalized = format!("//{}", stripped);
    } else if let Some(stripped) = normalized.strip_prefix("//?/") {
        normalized = stripped.to_string();
    }

    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }

    if cfg!(windows) {
        normalized = normalized.to_ascii_lowercase();
    }

    normalized
}

fn build_return_to_root_command(root: &Path) -> String {
    if cfg!(windows) {
        return format!("cd /d {}\n", shell_quote_path_for_shell(root));
    }
    format!("cd {}\n", shell_quote_path_for_shell(root))
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    std::fs::canonicalize(path).map(normalize_canonical_path)
}

fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return path;
    }

    let raw = path.to_string_lossy().to_string();
    if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{}", stripped));
    }
    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }
    path
}

fn shell_quote_path_for_shell(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();
    if cfg!(windows) {
        return format!("\"{}\"", raw.replace('"', "\"\""));
    }
    format!("'{}'", raw.replace('"', "\\\"").replace('\'', "'\"'\"'"))
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn input_triggers_busy(data: &str) -> bool {
    if data.is_empty() {
        return false;
    }
    if data.contains('\r') || data.contains('\n') {
        return true;
    }

    // Ctrl-C / Ctrl-D / Ctrl-Z may start or interrupt foreground commands.
    data.as_bytes()
        .iter()
        .any(|b| matches!(*b, 0x03 | 0x04 | 0x1A))
}

fn strip_ansi(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    static ANSI_CSI_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").unwrap());
    static ANSI_OSC_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)").unwrap()
    });
    static ANSI_ESC_RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"\x1B[@-_]").unwrap());

    let without_osc = ANSI_OSC_RE.replace_all(input, "");
    let without_csi = ANSI_CSI_RE.replace_all(&without_osc, "");
    ANSI_ESC_RE.replace_all(&without_csi, "").to_string()
}

fn is_prompt_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    static PROMPT_PATTERNS: once_cell::sync::Lazy<Vec<regex::Regex>> =
        once_cell::sync::Lazy::new(|| {
            vec![
                regex::Regex::new(r"^\([^)]+\)\s?.*[#$%>] ?$").unwrap(),
                regex::Regex::new(r"^[^\n\r]*@[^\n\r]*[#$%>] ?$").unwrap(),
                regex::Regex::new(r"^PS [A-Za-z]:\\.*> ?$").unwrap(),
                regex::Regex::new(r"^[A-Za-z]:\\.*> ?$").unwrap(),
                regex::Regex::new(r"^.*\$\s?$").unwrap(),
                regex::Regex::new(r"^.*%\s?$").unwrap(),
                regex::Regex::new(r"^.*>\s?$").unwrap(),
            ]
        });
    PROMPT_PATTERNS.iter().any(|re| re.is_match(trimmed))
}

#[cfg(test)]
mod tests {
    use super::{
        build_return_to_root_command, extract_prompt_cwd, infer_prompt_cwd_from_context,
        input_triggers_busy, is_prompt_line, normalize_path_for_compare, path_is_within_root,
        sanitize_command_line_for_guard, validate_directory_change_command,
    };
    use std::path::Path;

    #[test]
    fn input_only_marks_busy_when_it_can_change_foreground_command() {
        assert!(!input_triggers_busy(""));
        assert!(!input_triggers_busy("n"));
        assert!(!input_triggers_busy("\u{7f}"));
        assert!(input_triggers_busy("\r"));
        assert!(input_triggers_busy("npm run dev\r"));
        assert!(input_triggers_busy("\n"));
        assert!(input_triggers_busy("\u{3}"));
        assert!(input_triggers_busy("\u{4}"));
        assert!(input_triggers_busy("\u{1a}"));
    }

    #[test]
    fn prompt_detection_matches_common_shell_prompts() {
        assert!(is_prompt_line("PS C:\\repo>"));
        assert!(is_prompt_line("C:\\repo>"));
        assert!(is_prompt_line("user@host:~/repo$"));
        assert!(!is_prompt_line("vite v5 ready in 123 ms"));
    }

    #[test]
    fn prompt_parser_handles_ansi_wrapped_prompt() {
        let cwd = std::env::current_dir().unwrap();
        let expected = std::fs::canonicalize(&cwd).unwrap();

        let plain_prompt = if cfg!(windows) {
            format!("PS {}>", cwd.display())
        } else {
            format!("{}$", cwd.display())
        };
        let wrapped = format!("\u{1b}]633;A\u{7}\u{1b}[32m{plain_prompt}\u{1b}[0m");
        let cleaned = super::strip_ansi(wrapped.as_str());

        let parsed = extract_prompt_cwd(cleaned.as_str()).unwrap();
        assert_eq!(
            normalize_path_for_compare(parsed.as_path()),
            normalize_path_for_compare(expected.as_path())
        );
    }

    #[test]
    fn prompt_parser_recognizes_shell_working_directory() {
        let cwd = std::env::current_dir().unwrap();
        let expected = std::fs::canonicalize(&cwd).unwrap();

        if cfg!(windows) {
            let line = format!("PS {}>", cwd.display());
            let parsed = extract_prompt_cwd(line.as_str()).unwrap();
            assert_eq!(
                normalize_path_for_compare(parsed.as_path()),
                normalize_path_for_compare(expected.as_path())
            );
        } else {
            let line = format!("{}$", cwd.display());
            let parsed = extract_prompt_cwd(line.as_str()).unwrap();
            assert_eq!(
                normalize_path_for_compare(parsed.as_path()),
                normalize_path_for_compare(expected.as_path())
            );
        }
    }

    #[test]
    fn directory_guard_allows_descendants_and_blocks_escape() {
        let unique = format!(
            "chatos-terminal-guard-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        let root = base.join("root");
        let child = root.join("child");

        std::fs::create_dir_all(&child).unwrap();

        let root = std::fs::canonicalize(&root).unwrap();
        let mut current = root.clone();

        assert!(
            validate_directory_change_command("cd child", root.as_path(), &mut current).is_none()
        );
        assert!(path_is_within_root(current.as_path(), root.as_path()));

        assert!(validate_directory_change_command("cd ..", root.as_path(), &mut current).is_none());
        assert_eq!(current, root);

        let blocked_root_parent =
            validate_directory_change_command("cd ..", root.as_path(), &mut current);
        assert!(blocked_root_parent.is_some());

        let escape = format!("cd ..{}..", std::path::MAIN_SEPARATOR);
        let blocked =
            validate_directory_change_command(escape.as_str(), root.as_path(), &mut current);
        assert!(blocked.is_some());

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn directory_guard_blocks_dynamic_cd_syntax() {
        let root = std::fs::canonicalize(".").unwrap();
        let mut current = root.clone();
        let blocked = validate_directory_change_command("cd $HOME", root.as_path(), &mut current);
        assert!(blocked.is_some());
    }

    #[test]
    fn directory_guard_blocks_escape_when_cd_is_pasted_with_ansi_wrapper() {
        let unique = format!(
            "chatos-terminal-guard-ansi-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        let root = base.join("root");
        let outside = base.join("outside");

        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let root = std::fs::canonicalize(&root).unwrap();
        let outside = std::fs::canonicalize(&outside).unwrap();
        let mut current = root.clone();

        let pasted = format!("\x1b[200~cd {}\x1b[201~", outside.display());
        let sanitized = sanitize_command_line_for_guard(pasted.as_str());
        assert_eq!(sanitized, format!("cd {}", outside.display()));

        let blocked =
            validate_directory_change_command(sanitized.as_str(), root.as_path(), &mut current);
        assert!(blocked.is_some());

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn directory_guard_blocks_unresolvable_absolute_target() {
        let root = std::fs::canonicalize(".").unwrap();
        let mut current = root.clone();
        let candidate = format!(
            "/chatos-restricted-terminal-unresolvable-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        let command = format!("cd {candidate}");
        let blocked =
            validate_directory_change_command(command.as_str(), root.as_path(), &mut current);
        assert!(blocked.is_some());
    }

    #[test]
    fn prompt_inference_handles_basename_prompts_within_root() {
        let unique = format!(
            "chatos-terminal-prompt-infer-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        let root = base.join("root");
        let child = root.join("child");

        std::fs::create_dir_all(&child).unwrap();

        let root = std::fs::canonicalize(&root).unwrap();
        let child = std::fs::canonicalize(&child).unwrap();

        let parsed_child = infer_prompt_cwd_from_context(
            "(base) user@host child %",
            root.as_path(),
            root.as_path(),
        )
        .unwrap();
        assert_eq!(
            normalize_path_for_compare(parsed_child.as_path()),
            normalize_path_for_compare(child.as_path())
        );

        let root_name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .to_string();
        let parent_prompt = format!("(base) user@host {root_name} %");
        let parsed_parent =
            infer_prompt_cwd_from_context(parent_prompt.as_str(), child.as_path(), root.as_path())
                .unwrap();
        assert_eq!(
            normalize_path_for_compare(parsed_parent.as_path()),
            normalize_path_for_compare(root.as_path())
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn prompt_inference_parses_home_tilde_prompt() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let expected = std::fs::canonicalize(home).unwrap();
        let guessed = infer_prompt_cwd_from_context(
            "(base) user@host ~ %",
            expected.as_path(),
            expected.as_path(),
        )
        .unwrap();

        assert_eq!(
            normalize_path_for_compare(guessed.as_path()),
            normalize_path_for_compare(expected.as_path())
        );
    }

    #[test]
    fn sanitize_command_line_removes_tab_control_sequences() {
        let raw = "\x1b[200~cd /Users/lilei/\tproject\x1b[201~";
        let sanitized = sanitize_command_line_for_guard(raw);
        assert_eq!(sanitized, "cd /Users/lilei/project");
    }

    #[test]
    fn build_return_to_root_command_navigates_back_to_root() {
        let root = if cfg!(windows) {
            Path::new("C:\\repo\\sandbox")
        } else {
            Path::new("/tmp/repo/sandbox")
        };
        let command = build_return_to_root_command(root);

        if cfg!(windows) {
            assert_eq!(command, "cd C:\\repo\\sandbox\n");
        } else {
            assert_eq!(command, "cd /tmp/repo/sandbox\n");
        }
    }
}
