use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, MasterPty, PtySize};
use tokio::sync::broadcast;

use crate::models::terminal::Terminal;

use super::directory_guard::{
    build_return_to_root_command, clear_input_line_sequence, normalize_shell_input,
    sanitize_command_line_for_guard, validate_directory_change_command,
};
use super::io_runtime::{spawn_shell, spawn_terminal_output_persist, spawn_terminal_touch};
use super::output_history::{OutputHistory, SNAPSHOT_MAX_LINES};
use super::path_utils::{canonicalize_path, path_is_within_root};
use super::prompt_parser::{extract_prompt_cwd, infer_prompt_cwd_from_context, strip_ansi};
use super::{input_triggers_busy, now_millis, TerminalEvent};

pub struct TerminalSession {
    id: String,
    pub(super) sender: broadcast::Sender<TerminalEvent>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    output_history: Mutex<OutputHistory>,
    root_cwd: PathBuf,
    current_cwd: Mutex<PathBuf>,
    input_line: Mutex<String>,
    busy: AtomicBool,
    awaiting_command_output: AtomicBool,
    root_reset_in_progress: AtomicBool,
    last_input_at: AtomicU64,
    last_output_at: AtomicU64,
    last_prompt_at: AtomicU64,
}

impl TerminalSession {
    pub(super) fn new(
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
            output_history: Mutex::new(OutputHistory::default()),
            root_cwd: root_cwd.clone(),
            current_cwd: Mutex::new(root_cwd),
            input_line: Mutex::new(String::new()),
            busy: AtomicBool::new(false),
            awaiting_command_output: AtomicBool::new(false),
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
                        session_clone.append_and_emit_output(text.clone());

                        let cleaned = strip_ansi(&text);
                        let mut saw_prompt = false;
                        if !cleaned.is_empty() {
                            line_buffer.push_str(&cleaned);
                            let mut parts = line_buffer.split('\n').collect::<Vec<_>>();
                            let tail = parts.pop().unwrap_or("");
                            for line in parts.iter() {
                                let is_prompt =
                                    session_clone.sync_current_cwd_from_prompt_line(line);
                                session_clone.observe_output_line(line, is_prompt);
                                if is_prompt {
                                    saw_prompt = true;
                                }
                            }
                            line_buffer = tail.to_string();
                            let tail_is_prompt = session_clone
                                .sync_current_cwd_from_prompt_line(line_buffer.as_str());
                            session_clone.observe_output_line(line_buffer.as_str(), tail_is_prompt);
                            if !saw_prompt && tail_is_prompt {
                                saw_prompt = true;
                            }
                            if saw_prompt {
                                session_clone.mark_prompt();
                            }
                        }

                        output_log_buffer.push_str(&text);
                        let should_flush = !output_log_buffer.is_empty()
                            && (output_log_buffer.len() >= 8 * 1024
                                || last_flush.elapsed() >= flush_interval
                                || saw_prompt);

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

    pub fn output_snapshot_tail_lines(&self, max_lines: usize) -> String {
        match self.output_history.lock() {
            Ok(history) => history.snapshot_tail_lines(max_lines.min(SNAPSHOT_MAX_LINES)),
            Err(_) => String::new(),
        }
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
        self.append_and_emit_output(output);
    }

    fn append_and_emit_output(&self, output: String) {
        if let Ok(mut history) = self.output_history.lock() {
            history.push(output.clone());
        }
        let _ = self.sender.send(TerminalEvent::Output(output));
        self.mark_output();
    }

    fn sync_current_cwd_from_prompt_line(&self, line: &str) -> bool {
        let parsed_cwd = extract_prompt_cwd(line).or_else(|| {
            let current = self.current_cwd.lock().ok()?.clone();
            infer_prompt_cwd_from_context(line, current.as_path(), self.root_cwd.as_path())
        });

        let Some(parsed_cwd) = parsed_cwd else {
            return false;
        };

        if !path_is_within_root(parsed_cwd.as_path(), self.root_cwd.as_path()) {
            self.reset_shell_to_root(parsed_cwd.as_path());
            return true;
        }

        self.root_reset_in_progress.store(false, Ordering::Relaxed);

        if let Ok(mut cwd_guard) = self.current_cwd.lock() {
            *cwd_guard = parsed_cwd;
        }
        true
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
            self.awaiting_command_output
                .store(has_visible_command_text(data), Ordering::Relaxed);
            self.set_busy(true);
        }
    }

    fn mark_output(&self) {
        self.last_output_at.store(now_millis(), Ordering::Relaxed);
    }

    fn mark_prompt(&self) {
        self.last_prompt_at.store(now_millis(), Ordering::Relaxed);
        if self.awaiting_command_output.load(Ordering::Relaxed) {
            return;
        }
        self.set_busy(false);
    }

    fn observe_output_line(&self, line: &str, is_prompt: bool) {
        if !self.is_busy() || is_prompt {
            return;
        }
        if line.trim().is_empty() {
            return;
        }
        self.awaiting_command_output.store(false, Ordering::Relaxed);
    }

    fn set_busy(&self, busy: bool) {
        let prev = self.busy.swap(busy, Ordering::Relaxed);
        if prev != busy {
            let _ = self.sender.send(TerminalEvent::State(busy));
        }
    }
}

fn has_visible_command_text(data: &str) -> bool {
    data.chars()
        .any(|ch| !ch.is_control() && !ch.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::has_visible_command_text;

    #[test]
    fn visible_command_text_detection_ignores_control_only_input() {
        assert!(!has_visible_command_text(""));
        assert!(!has_visible_command_text("\r"));
        assert!(!has_visible_command_text("\n"));
        assert!(!has_visible_command_text("\u{3}"));
    }

    #[test]
    fn visible_command_text_detection_recognizes_actual_command_input() {
        assert!(has_visible_command_text("npm run dev\r"));
        assert!(has_visible_command_text(" ls\n"));
    }
}
