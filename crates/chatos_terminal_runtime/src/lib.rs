// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::{sleep, Duration, Instant};

pub const DEFAULT_TERMINAL_LOG_CAPACITY: usize = 4_000;
pub const DEFAULT_TERMINAL_READ_BUFFER_BYTES: usize = 2_048;
pub const MIN_TERMINAL_WAIT_TIMEOUT_MS: u64 = 1_000;
pub const MAX_TERMINAL_WAIT_TIMEOUT_MS: u64 = 600_000;
pub const TERMINAL_STATUS_RUNNING: &str = "running";
pub const TERMINAL_STATUS_EXITED: &str = "exited";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSessionMeta {
    pub id: String,
    pub cwd: String,
    pub project_id: Option<String>,
    pub user_id: Option<String>,
    pub command: String,
    pub started_at: String,
    pub last_active_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub exit_code: Option<i32>,
}

impl TerminalSessionMeta {
    pub fn new(
        id: String,
        cwd: String,
        project_id: Option<String>,
        user_id: Option<String>,
        command: String,
        started_at: String,
    ) -> Self {
        Self {
            id,
            cwd,
            project_id,
            user_id,
            command,
            last_active_at: started_at.clone(),
            started_at,
            finished_at: None,
            status: TERMINAL_STATUS_RUNNING.to_string(),
            exit_code: None,
        }
    }

    pub fn is_exited(&self) -> bool {
        self.status == TERMINAL_STATUS_EXITED
    }

    pub fn record_activity(&mut self, active_at: String) {
        self.last_active_at = active_at;
    }

    pub fn matches_scope(
        &self,
        canonical_root: &Path,
        project_id: Option<&str>,
        user_id: Option<&str>,
    ) -> bool {
        let same_user = user_id.is_none_or(|user_id| self.user_id.as_deref() == Some(user_id));
        let in_scope = project_id.map_or_else(
            || Path::new(self.cwd.as_str()).starts_with(canonical_root),
            |project_id| self.project_id.as_deref() == Some(project_id),
        );
        same_user && in_scope
    }

    pub fn mark_exited(&mut self, exit_code: Option<i32>, finished_at: String) -> bool {
        if self.is_exited() {
            return false;
        }
        self.status = TERMINAL_STATUS_EXITED.to_string();
        self.exit_code = exit_code;
        self.last_active_at = finished_at.clone();
        self.finished_at = Some(finished_at);
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalWaitResult {
    pub waited_ms: u64,
    pub busy: bool,
    pub timed_out: bool,
    pub finished_by: &'static str,
    pub exit_code: Option<i32>,
}

impl TerminalWaitResult {
    pub fn exited(waited_ms: u64, exit_code: Option<i32>) -> Self {
        Self {
            waited_ms,
            busy: false,
            timed_out: false,
            finished_by: "exit",
            exit_code,
        }
    }

    pub fn timed_out(waited_ms: u64, meta: &TerminalSessionMeta) -> Self {
        Self {
            waited_ms,
            busy: !meta.is_exited(),
            timed_out: true,
            finished_by: "timeout",
            exit_code: meta.exit_code,
        }
    }
}

pub fn terminal_wait_timeout_ms(timeout_ms: u64) -> u64 {
    timeout_ms.clamp(MIN_TERMINAL_WAIT_TIMEOUT_MS, MAX_TERMINAL_WAIT_TIMEOUT_MS)
}

pub async fn wait_for_terminal_session<F, Fut, E>(
    timeout_ms: u64,
    mut inspect_session: F,
) -> Result<TerminalWaitResult, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<TerminalSessionMeta, E>>,
{
    let timeout = Duration::from_millis(terminal_wait_timeout_ms(timeout_ms));
    let started = Instant::now();
    loop {
        let meta = inspect_session().await?;
        if meta.is_exited() {
            return Ok(TerminalWaitResult::exited(
                started.elapsed().as_millis() as u64,
                meta.exit_code,
            ));
        }
        if started.elapsed() >= timeout {
            return Ok(TerminalWaitResult::timed_out(
                started.elapsed().as_millis() as u64,
                &meta,
            ));
        }
        sleep(Duration::from_millis(100)).await;
    }
}

#[derive(Debug)]
pub enum TerminalPathError {
    Unavailable(std::io::Error),
    EscapesWorkspace,
}

impl fmt::Display for TerminalPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(error) => error.fmt(formatter),
            Self::EscapesWorkspace => formatter.write_str("target path escapes workspace root"),
        }
    }
}

impl std::error::Error for TerminalPathError {}

pub fn canonicalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

pub fn resolve_target_path(root: &Path, path_input: &str) -> Result<PathBuf, TerminalPathError> {
    let trimmed = path_input.trim();
    let joined = if trimmed.is_empty() || trimmed == "." {
        root.to_path_buf()
    } else {
        let path = PathBuf::from(trimmed);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    let canonical =
        canonicalize_existing(joined.as_path()).map_err(TerminalPathError::Unavailable)?;
    if !canonical.starts_with(root) {
        return Err(TerminalPathError::EscapesWorkspace);
    }
    Ok(canonical)
}

pub fn display_workspace_path(root: &Path, path: &Path) -> String {
    if path == root {
        return "/workspace".to_string();
    }
    if let Ok(relative) = path.strip_prefix(root) {
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            "/workspace".to_string()
        } else {
            format!("/workspace/{}", relative.trim_start_matches('/'))
        }
    } else {
        "/workspace".to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLogEntry {
    pub offset: i64,
    pub kind: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct OutputCapture {
    pub text: String,
    pub char_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct TerminalLogBuffer {
    entries: Vec<TerminalLogEntry>,
    max_entries: usize,
}

impl Default for TerminalLogBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_TERMINAL_LOG_CAPACITY)
    }
}

impl TerminalLogBuffer {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries: max_entries.max(1),
        }
    }

    pub fn append(
        &mut self,
        kind: impl Into<String>,
        content: impl Into<String>,
        created_at: impl Into<String>,
    ) -> bool {
        let content = content.into();
        if content.is_empty() {
            return false;
        }
        let offset = self
            .entries
            .last()
            .map(|entry| entry.offset + 1)
            .unwrap_or(0);
        self.entries.push(TerminalLogEntry {
            offset,
            kind: kind.into(),
            content,
            created_at: created_at.into(),
        });
        if self.entries.len() > self.max_entries {
            let drain = self.entries.len() - self.max_entries;
            self.entries.drain(0..drain);
        }
        true
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capture(&self, max_chars: usize) -> OutputCapture {
        collect_output_from_texts(
            self.entries.iter().map(|entry| entry.content.as_str()),
            max_chars,
        )
    }

    pub fn select_json(&self, offset: Option<i64>, limit: usize) -> Vec<Value> {
        let selected = if let Some(offset) = offset {
            self.entries
                .iter()
                .filter(|entry| entry.offset >= offset.max(0))
                .take(limit)
                .collect::<Vec<_>>()
        } else {
            self.entries.iter().rev().take(limit).collect::<Vec<_>>()
        };
        let ordered = if offset.is_some() {
            selected
        } else {
            selected.into_iter().rev().collect::<Vec<_>>()
        };
        ordered.into_iter().map(log_to_value).collect()
    }

    pub fn recent_json(&self, limit: usize) -> Vec<Value> {
        self.entries
            .iter()
            .rev()
            .take(limit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(log_to_value)
            .collect()
    }
}

pub fn collect_output_from_texts<'a, I>(items: I, max_chars: usize) -> OutputCapture
where
    I: Iterator<Item = &'a str>,
{
    let full = items.collect::<Vec<_>>().join("");
    let char_count = full.chars().count();
    if char_count <= max_chars {
        return OutputCapture {
            text: full,
            char_count,
            truncated: false,
        };
    }
    let text = full
        .chars()
        .skip(char_count.saturating_sub(max_chars))
        .collect::<String>();
    OutputCapture {
        text,
        char_count,
        truncated: true,
    }
}

pub async fn read_output_chunks<R, F, Fut>(mut reader: R, mut on_chunk: F) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    F: FnMut(String) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut buffer = vec![0_u8; DEFAULT_TERMINAL_READ_BUFFER_BYTES];
    loop {
        let count = reader.read(buffer.as_mut_slice()).await?;
        if count == 0 {
            return Ok(());
        }
        let chunk = String::from_utf8_lossy(&buffer[..count]).to_string();
        if !chunk.is_empty() {
            on_chunk(chunk).await;
        }
    }
}

pub fn log_value_content(value: &Value) -> Option<&str> {
    value.get("content").and_then(Value::as_str)
}

pub fn derive_terminal_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("terminal")
        .to_string()
}

fn log_to_value(entry: &TerminalLogEntry) -> Value {
    json!({
        "offset": entry.offset,
        "kind": entry.kind,
        "content": entry.content,
        "created_at": entry.created_at,
    })
}

#[cfg(test)]
mod tests;
