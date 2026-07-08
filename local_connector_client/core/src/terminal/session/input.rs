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

use super::{InteractiveCommandSubmission, LocalPtySession};

impl LocalPtySession {
    pub(crate) fn write_input(&self, data: &str) -> Result<Vec<InteractiveCommandSubmission>> {
        let (forward_data, blocked_messages, submissions) = self.apply_directory_guard(data);
        if forward_data.contains('\r') || forward_data.contains('\n') {
            self.busy.store(true, Ordering::SeqCst);
        }
        if !forward_data.is_empty() {
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| anyhow!("terminal writer lock failed"))?;
            writer
                .write_all(forward_data.as_bytes())
                .map_err(|err| anyhow!("write terminal input failed: {err}"))?;
            writer
                .flush()
                .map_err(|err| anyhow!("flush terminal input failed: {err}"))?;
        }
        for message in blocked_messages {
            let data = format!("\r\n{message}\r\n");
            self.append_output(data.as_str());
            let _ = self.outbound.send(terminal_event(
                "terminal_output",
                self.id.as_str(),
                json!({ "data": data }),
            ));
        }
        Ok(submissions)
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
                    let sanitized = sanitize_terminal_command_line(command_line.as_str());
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
