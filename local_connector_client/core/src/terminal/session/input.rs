// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::Ordering;

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::relay::terminal_event;
use crate::terminal::guard::{
    clear_terminal_input_line, normalize_terminal_input, sanitize_terminal_command_line,
    validate_local_terminal_command,
};

use super::{InteractiveCommandSubmission, LocalPtySession, PreparedTerminalInput};

impl LocalPtySession {
    pub(crate) fn set_submitted_command(&self, command: &str) {
        let sanitized = sanitize_terminal_command_line(command).trim().to_string();
        if sanitized.is_empty() {
            return;
        }
        if let Ok(mut submitted) = self.submitted_command.lock() {
            *submitted = Some(sanitized);
        }
    }

    pub(crate) fn prepare_input(&self, data: &str) -> Result<PreparedTerminalInput> {
        let previous_cwd = self
            .current_cwd
            .lock()
            .map_err(|_| anyhow!("terminal cwd lock failed"))?
            .clone();
        let (forward_data, blocked_messages, submissions) = self.apply_directory_guard(data);
        Ok(PreparedTerminalInput {
            forward_data,
            blocked_messages,
            submissions,
            previous_cwd,
        })
    }

    pub(crate) fn commit_prepared_input(
        &self,
        prepared: PreparedTerminalInput,
    ) -> Result<Vec<InteractiveCommandSubmission>> {
        let PreparedTerminalInput {
            forward_data,
            blocked_messages,
            submissions,
            ..
        } = prepared;
        if forward_data.contains('\r') || forward_data.contains('\n') {
            self.busy.store(true, Ordering::SeqCst);
        }
        self.write_forward_data(forward_data.as_str())?;
        self.emit_blocked_messages(blocked_messages);
        Ok(submissions)
    }

    pub(crate) fn reject_prepared_input(
        &self,
        prepared: PreparedTerminalInput,
        messages: Vec<String>,
    ) -> Result<Vec<InteractiveCommandSubmission>> {
        if let Ok(mut current_cwd) = self.current_cwd.lock() {
            *current_cwd = prepared.previous_cwd;
        }
        if let Ok(mut line) = self.input_line.lock() {
            line.clear();
        }
        let clear_line = prepared
            .submissions
            .iter()
            .rev()
            .find(|submission| !submission.command.trim().is_empty())
            .map(|submission| clear_terminal_input_line(submission.command.as_str()))
            .unwrap_or_default();
        self.write_forward_data(clear_line.as_str())?;
        self.emit_blocked_messages(messages);
        Ok(prepared.submissions)
    }

    fn take_submitted_command(&self) -> Option<String> {
        self.submitted_command
            .lock()
            .ok()
            .and_then(|mut submitted| submitted.take())
    }

    fn clear_submitted_command(&self) {
        if let Ok(mut submitted) = self.submitted_command.lock() {
            *submitted = None;
        }
    }

    fn write_forward_data(&self, forward_data: &str) -> Result<()> {
        if forward_data.is_empty() {
            return Ok(());
        }
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("terminal writer lock failed"))?;
        writer
            .write_all(forward_data.as_bytes())
            .map_err(|err| anyhow!("write terminal input failed: {err}"))?;
        writer
            .flush()
            .map_err(|err| anyhow!("flush terminal input failed: {err}"))
    }

    fn emit_blocked_messages(&self, blocked_messages: Vec<String>) {
        for message in blocked_messages {
            let data = format!("\r\n{message}\r\n");
            self.append_output(data.as_str());
            let _ = self.outbound.send(terminal_event(
                "terminal_output",
                self.id.as_str(),
                json!({ "data": data }),
            ));
        }
    }

    fn apply_directory_guard(
        &self,
        data: &str,
    ) -> (String, Vec<String>, Vec<InteractiveCommandSubmission>) {
        if data.is_empty() {
            return (String::new(), Vec::new(), Vec::new());
        }
        let mut line = match self.input_line.lock() {
            Ok(line) => line,
            Err(_) => return (data.to_string(), Vec::new(), Vec::new()),
        };
        let mut current_cwd = match self.current_cwd.lock() {
            Ok(current_cwd) => current_cwd,
            Err(_) => return (data.to_string(), Vec::new(), Vec::new()),
        };
        let mut forward = String::with_capacity(data.len());
        let mut blocked = Vec::new();
        let mut submissions = Vec::new();
        let mut skip_following_lf = false;

        for ch in normalize_terminal_input(data).chars() {
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
                    let mut sanitized = sanitize_terminal_command_line(command_line.as_str());
                    if let Some(submitted_command) = self.take_submitted_command() {
                        sanitized = submitted_command;
                    }
                    let cwd_before = current_cwd.clone();
                    line.clear();
                    if let Some(reason) = validate_local_terminal_command(
                        sanitized.as_str(),
                        self.root_cwd.as_path(),
                        &mut current_cwd,
                    ) {
                        if !sanitized.trim().is_empty() {
                            submissions.push(InteractiveCommandSubmission {
                                command: sanitized.clone(),
                                cwd: cwd_before,
                                blocked_reason: Some(reason.clone()),
                            });
                        }
                        forward.push_str(clear_terminal_input_line(sanitized.as_str()).as_str());
                        skip_following_lf = ch == '\r';
                        blocked.push(reason);
                        continue;
                    }
                    if !sanitized.trim().is_empty() {
                        submissions.push(InteractiveCommandSubmission {
                            command: sanitized,
                            cwd: cwd_before,
                            blocked_reason: None,
                        });
                    }
                    forward.push(ch);
                }
                '\u{8}' | '\u{7f}' => {
                    line.pop();
                    forward.push(ch);
                }
                '\u{3}' => {
                    line.clear();
                    self.clear_submitted_command();
                    forward.push(ch);
                }
                _ => {
                    line.push(ch);
                    forward.push(ch);
                }
            }
        }
        (forward, blocked, submissions)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Result as IoResult, Write};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex as StdMutex;

    use portable_pty::{ChildKiller, MasterPty, PtySize};
    use tokio::sync::mpsc;

    use super::*;

    #[derive(Debug)]
    struct FakeChildKiller;

    impl ChildKiller for FakeChildKiller {
        fn kill(&mut self) -> IoResult<()> {
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(FakeChildKiller)
        }
    }

    struct FakeMasterPty;

    impl MasterPty for FakeMasterPty {
        fn resize(&self, _size: PtySize) -> Result<(), anyhow::Error> {
            Ok(())
        }

        fn get_size(&self) -> Result<PtySize, anyhow::Error> {
            Ok(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
        }

        fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>, anyhow::Error> {
            Ok(Box::new(Cursor::new(Vec::<u8>::new())))
        }

        fn take_writer(&self) -> Result<Box<dyn Write + Send>, anyhow::Error> {
            Ok(Box::new(Cursor::new(Vec::<u8>::new())))
        }

        #[cfg(unix)]
        fn process_group_leader(&self) -> Option<std::os::raw::c_int> {
            None
        }

        #[cfg(unix)]
        fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
            None
        }

        #[cfg(unix)]
        fn tty_name(&self) -> Option<PathBuf> {
            None
        }
    }

    fn fake_session(root_cwd: PathBuf) -> LocalPtySession {
        let (tx, _rx) = mpsc::unbounded_channel();
        LocalPtySession {
            id: "test-session".to_string(),
            root_cwd: root_cwd.clone(),
            current_cwd: StdMutex::new(root_cwd),
            input_line: StdMutex::new(String::new()),
            submitted_command: StdMutex::new(None),
            writer: StdMutex::new(Box::new(Cursor::new(Vec::<u8>::new()))),
            master: StdMutex::new(Box::new(FakeMasterPty)),
            child_killer: StdMutex::new(Box::new(FakeChildKiller)),
            outbound: tx,
            output_history: StdMutex::new(String::new()),
            busy: AtomicBool::new(false),
            exited: AtomicBool::new(false),
        }
    }

    #[test]
    fn prepare_input_prefers_submitted_command_over_partial_line_buffer() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-connector-terminal-input-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.as_path()).expect("create temp root");
        let root = std::fs::canonicalize(root.as_path()).expect("canonical temp root");
        let session = fake_session(root.clone());

        let seed = session.prepare_input("cat _up").expect("seed input");
        assert!(seed.submissions.is_empty());

        session.set_submitted_command("cat __up");
        let prepared = session.prepare_input("\r").expect("prepare enter");

        assert_eq!(prepared.submissions.len(), 1);
        assert_eq!(prepared.submissions[0].command, "cat __up");
        assert!(prepared.submissions[0].blocked_reason.is_none());

        std::fs::remove_dir_all(root.as_path()).expect("cleanup temp root");
    }
}
