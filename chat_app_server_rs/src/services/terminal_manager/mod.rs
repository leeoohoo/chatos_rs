mod directory_guard;
mod io_runtime;
mod path_utils;
mod prompt_parser;
mod shell_path;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use once_cell::sync::OnceCell;
use portable_pty::{native_pty_system, MasterPty, PtySize};
use tokio::sync::broadcast;

use crate::models::terminal::Terminal;
use crate::repositories::terminals;

use self::directory_guard::{
    build_return_to_root_command, clear_input_line_sequence, normalize_shell_input,
    sanitize_command_line_for_guard, validate_directory_change_command,
};
use self::io_runtime::{spawn_shell, spawn_terminal_output_persist, spawn_terminal_touch};
use self::path_utils::{canonicalize_path, path_is_within_root};
use self::prompt_parser::{
    extract_prompt_cwd, infer_prompt_cwd_from_context, is_prompt_line, strip_ansi,
};

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

        let (sender, _) = broadcast::channel(4096);

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
            let mut output_log_buffer = String::new();
            let flush_interval = std::time::Duration::from_millis(250);
            let touch_interval = std::time::Duration::from_millis(1_000);
            let mut last_flush = std::time::Instant::now();
            let mut last_touch = std::time::Instant::now();

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

                        output_log_buffer.push_str(&text);
                        let should_flush = !output_log_buffer.is_empty()
                            && (output_log_buffer.len() >= 8 * 1024
                                || last_flush.elapsed() >= flush_interval);

                        if should_flush {
                            spawn_terminal_output_persist(
                                handle.clone(),
                                session_clone.id.clone(),
                                std::mem::take(&mut output_log_buffer),
                            );
                            let now = std::time::Instant::now();
                            last_flush = now;
                            last_touch = now;
                        } else if last_touch.elapsed() >= touch_interval {
                            spawn_terminal_touch(handle.clone(), session_clone.id.clone());
                            last_touch = std::time::Instant::now();
                        }
                    }
                    Err(_) => break,
                }
            }

            if !output_log_buffer.is_empty() {
                spawn_terminal_output_persist(
                    handle.clone(),
                    session_clone.id.clone(),
                    output_log_buffer,
                );
            } else if last_touch.elapsed() >= touch_interval {
                spawn_terminal_touch(handle.clone(), session_clone.id.clone());
            }
        });

        Ok((session, child))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.sender.subscribe()
    }

    pub fn write_input(&self, data: &str) -> Result<(), String> {
        let normalized = normalize_shell_input(data);
        let (forward_data, blocked_messages) = self.apply_directory_guard(normalized.as_str());
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::directory_guard::{
        build_return_to_root_command, normalize_shell_input, sanitize_command_line_for_guard,
        validate_directory_change_command,
    };
    use super::input_triggers_busy;
    use super::path_utils::{normalize_path_for_compare, path_is_within_root};
    use super::prompt_parser::{
        extract_prompt_cwd, infer_prompt_cwd_from_context, is_prompt_line, strip_ansi,
    };

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
        let cleaned = strip_ansi(wrapped.as_str());

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
            assert_eq!(command, "cd /d \"C:\\repo\\sandbox\"\r");
        } else {
            assert_eq!(command, "cd '/tmp/repo/sandbox'\n");
        }
    }

    #[test]
    fn normalize_shell_input_uses_windows_return_key() {
        let raw = "echo one\necho two\r\necho three\r";
        let normalized = normalize_shell_input(raw);

        if cfg!(windows) {
            assert_eq!(normalized, "echo one\recho two\recho three\r");
        } else {
            assert_eq!(normalized, raw);
        }
    }
}
