use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::broadcast;
use tokio::time::{Duration, Instant};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::models::project::ProjectService;
use crate::models::terminal::{Terminal, TerminalService};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::terminal_manager::{get_terminal_manager, TerminalEvent};

pub struct TerminalControllerOptions {
    pub root: PathBuf,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub idle_timeout_ms: u64,
    pub max_wait_ms: u64,
    pub max_output_chars: usize,
}

#[derive(Clone)]
pub struct TerminalControllerService {
    tools: HashMap<String, Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, Option<&str>) -> Result<Value, String> + Send + Sync>;

const RECENT_LOGS_MAX_PER_TERMINAL_LIMIT: i64 = 50;
const RECENT_LOGS_MAX_TERMINAL_LIMIT: u64 = 20;
const RECENT_LOGS_PER_ENTRY_MAX_CHARS: usize = 1_500;
const RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL: usize = 16_000;

#[derive(Clone)]
struct BoundContext {
    root: PathBuf,
    user_id: Option<String>,
    project_id: Option<String>,
    idle_timeout_ms: u64,
    max_wait_ms: u64,
    max_output_chars: usize,
}

impl TerminalControllerService {
    pub fn new(opts: TerminalControllerOptions) -> Result<Self, String> {
        std::fs::create_dir_all(&opts.root)
            .map_err(|err| format!("create terminal controller root failed: {}", err))?;
        let root = canonicalize_path(&opts.root)?;

        let mut service = Self {
            tools: HashMap::new(),
        };

        let bound = BoundContext {
            root: root.clone(),
            user_id: opts.user_id.clone(),
            project_id: opts
                .project_id
                .as_deref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            idle_timeout_ms: opts.idle_timeout_ms.max(1_000),
            max_wait_ms: opts.max_wait_ms.max(5_000),
            max_output_chars: opts.max_output_chars.max(1_000),
        };

        let root_for_desc = root.to_string_lossy().to_string();
        let execute_ctx = bound.clone();
        service.register_tool(
            "execute_command",
            &format!(
                "Execute command in project terminal with path switching. Relative path is resolved from project root ({root_for_desc})."
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "common": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["path", "common"]
            }),
            Arc::new(move |args, _session_id| {
                let path = required_trimmed_string(&args, "path")?;
                let command = required_trimmed_string(&args, "common")?;
                let ctx = execute_ctx.clone();
                let result = block_on_result(async move {
                    execute_command_with_context(ctx, path.as_str(), command.as_str()).await
                })?;
                Ok(text_result(result))
            }),
        );

        let recent_logs_ctx = bound.clone();
        service.register_tool(
            "get_recent_logs",
            "Get recent logs grouped by terminal for current agent project.",
            json!({
                "type": "object",
                "properties": {
                    "per_terminal_limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "terminal_limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, _session_id| {
                let per_terminal_limit = args
                    .get("per_terminal_limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10)
                    .clamp(1, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT);
                let terminal_limit =
                    args.get("terminal_limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(20)
                        .clamp(1, RECENT_LOGS_MAX_TERMINAL_LIMIT) as usize;
                let ctx = recent_logs_ctx.clone();
                let result = block_on_result(async move {
                    get_recent_logs_with_context(ctx, per_terminal_limit, terminal_limit).await
                })?;
                Ok(text_result(result))
            }),
        );

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, session_id)
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.tools.insert(
            name.to_string(),
            Tool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
    }
}

async fn execute_command_with_context(
    ctx: BoundContext,
    path_input: &str,
    command: &str,
) -> Result<Value, String> {
    let (project_id, project_root) = resolve_project_root(&ctx).await?;
    let target_path = resolve_target_path(project_root.as_path(), path_input)?;

    let manager = get_terminal_manager();
    let (terminal, reused) = if let Some(idle) =
        find_idle_terminal(&project_id, project_root.as_path(), ctx.user_id.as_deref()).await?
    {
        (idle, true)
    } else {
        let name = derive_terminal_name(project_root.as_path());
        let created = manager
            .create(
                name,
                project_root.to_string_lossy().to_string(),
                ctx.user_id.clone(),
                project_id.clone(),
            )
            .await?;
        (created, false)
    };

    let session = manager.ensure_running(&terminal).await?;
    let mut receiver = session.subscribe();

    let input_data = build_input_payload(project_root.as_path(), target_path.as_path(), command);
    session.write_input(input_data.as_str())?;

    let trimmed_command = command.trim();
    if !trimmed_command.is_empty() {
        let cmd_log = TerminalLog::new(
            terminal.id.clone(),
            "command".to_string(),
            trimmed_command.to_string(),
        );
        let _ = TerminalLogService::create(cmd_log).await;
    }
    if !input_data.is_empty() {
        let input_log = TerminalLog::new(terminal.id.clone(), "input".to_string(), input_data);
        let _ = TerminalLogService::create(input_log).await;
    }
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    let capture = capture_command_output(
        &mut receiver,
        Duration::from_millis(ctx.idle_timeout_ms),
        Duration::from_millis(ctx.max_wait_ms),
        ctx.max_output_chars,
    )
    .await;

    Ok(json!({
        "project_id": project_id,
        "project_root": project_root.to_string_lossy(),
        "terminal_id": terminal.id,
        "terminal_reused": reused,
        "path": target_path.to_string_lossy(),
        "common": command,
        "output": capture.output,
        "output_chars": capture.output.chars().count(),
        "truncated": capture.truncated,
        "finished_by": capture.finished_by,
        "idle_timeout_ms": ctx.idle_timeout_ms,
        "max_wait_ms": ctx.max_wait_ms,
        "max_output_chars": ctx.max_output_chars
    }))
}

async fn get_recent_logs_with_context(
    ctx: BoundContext,
    per_terminal_limit: i64,
    terminal_limit: usize,
) -> Result<Value, String> {
    let terminals = list_terminals_for_context(&ctx).await?;
    let total_terminals = terminals.len();

    if total_terminals == 0 {
        return Ok(json!({
            "result_scope": "no_terminal",
            "is_multiple_terminals": false,
            "terminal_count": 0,
            "total_terminals": 0,
            "per_terminal_limit": per_terminal_limit,
            "terminal_limit": terminal_limit,
            "terminals": []
        }));
    }

    let mut selected = terminals;
    if selected.len() > terminal_limit {
        selected.truncate(terminal_limit);
    }

    let mut terminal_results = Vec::new();
    for terminal in selected {
        let logs =
            TerminalLogService::list_recent(terminal.id.as_str(), per_terminal_limit).await?;
        let (compact_logs, truncation) = compact_recent_logs(
            logs.as_slice(),
            RECENT_LOGS_PER_ENTRY_MAX_CHARS,
            RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL,
        );
        terminal_results.push(json!({
            "terminal_id": terminal.id,
            "terminal_name": terminal.name,
            "status": terminal.status,
            "cwd": terminal.cwd,
            "project_id": terminal.project_id,
            "last_active_at": terminal.last_active_at,
            "log_count": logs.len(),
            "returned_log_count": compact_logs.len(),
            "truncated": truncation.truncated,
            "truncation": {
                "per_log_capped": truncation.per_log_capped,
                "total_capped": truncation.total_capped,
                "dropped_logs": truncation.dropped_logs,
                "original_chars": truncation.original_chars,
                "returned_chars": truncation.returned_chars
            },
            "logs": compact_logs
        }));
    }

    let terminal_count = terminal_results.len();
    let result_scope = if terminal_count > 1 {
        "multiple_terminals"
    } else {
        "single_terminal"
    };

    Ok(json!({
        "result_scope": result_scope,
        "is_multiple_terminals": terminal_count > 1,
        "terminal_count": terminal_count,
        "total_terminals": total_terminals,
        "per_terminal_limit": per_terminal_limit,
        "terminal_limit": terminal_limit,
        "terminals": terminal_results
    }))
}

struct OutputCapture {
    output: String,
    truncated: bool,
    finished_by: &'static str,
}

#[derive(Debug, Default)]
struct LogTruncationStats {
    truncated: bool,
    per_log_capped: usize,
    total_capped: bool,
    dropped_logs: usize,
    original_chars: usize,
    returned_chars: usize,
}

fn compact_recent_logs(
    logs: &[TerminalLog],
    per_entry_max_chars: usize,
    total_max_chars: usize,
) -> (Vec<Value>, LogTruncationStats) {
    let mut stats = LogTruncationStats::default();
    if logs.is_empty() || total_max_chars == 0 {
        stats.truncated = !logs.is_empty();
        stats.total_capped = !logs.is_empty();
        stats.dropped_logs = logs.len();
        return (Vec::new(), stats);
    }

    let mut kept_rev: Vec<Value> = Vec::new();
    let mut total_chars = 0usize;
    let mut hit_total_limit = false;

    for (index_from_newest, log) in logs.iter().rev().enumerate() {
        let original_chars = log.content.chars().count();
        stats.original_chars += original_chars;

        let mut content = log.content.clone();
        let mut entry_truncated = false;
        if original_chars > per_entry_max_chars {
            content = truncate_keep_tail(log.content.as_str(), per_entry_max_chars);
            entry_truncated = true;
            stats.per_log_capped += 1;
        }

        let content_chars = content.chars().count();
        let remaining = total_max_chars.saturating_sub(total_chars);
        if remaining == 0 {
            hit_total_limit = true;
            stats.dropped_logs = logs.len().saturating_sub(index_from_newest);
            break;
        }

        if content_chars > remaining {
            content = truncate_keep_tail(content.as_str(), remaining);
            hit_total_limit = true;
            stats.dropped_logs = logs
                .len()
                .saturating_sub(index_from_newest)
                .saturating_sub(1);
            kept_rev.push(json!({
                "id": log.id,
                "terminal_id": log.terminal_id,
                "log_type": log.log_type,
                "content": content,
                "created_at": log.created_at,
            }));
            break;
        }

        total_chars += content_chars;
        kept_rev.push(json!({
            "id": log.id,
            "terminal_id": log.terminal_id,
            "log_type": log.log_type,
            "content": content,
            "created_at": log.created_at,
        }));

        if entry_truncated {
            stats.truncated = true;
        }
    }

    let mut kept = kept_rev;
    kept.reverse();
    stats.returned_chars = kept
        .iter()
        .map(|item| {
            item.get("content")
                .and_then(|value| value.as_str())
                .map(|value| value.chars().count())
                .unwrap_or(0)
        })
        .sum();
    stats.total_capped = hit_total_limit;
    stats.truncated = stats.truncated || hit_total_limit || stats.dropped_logs > 0;
    (kept, stats)
}

fn truncate_keep_tail(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = input.chars().count();
    if total <= max_chars {
        return input.to_string();
    }

    let marker = format!("[...truncated {} chars...]\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return input
            .chars()
            .rev()
            .take(max_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }

    let keep_tail = max_chars - marker_chars;
    let tail: String = input
        .chars()
        .rev()
        .take(keep_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}", marker, tail)
}

async fn capture_command_output(
    receiver: &mut broadcast::Receiver<TerminalEvent>,
    idle_timeout: Duration,
    max_wait: Duration,
    max_output_chars: usize,
) -> OutputCapture {
    let start = Instant::now();
    let mut last_output_at = Instant::now();
    let mut output = String::new();
    let mut truncated = false;

    let finished_by = loop {
        let elapsed = start.elapsed();
        if elapsed >= max_wait {
            break "max_wait_timeout";
        }

        let idle_elapsed = last_output_at.elapsed();
        if idle_elapsed >= idle_timeout {
            break "idle_timeout";
        }

        let until_idle = idle_timeout - idle_elapsed;
        let until_deadline = max_wait - elapsed;
        let wait_duration = std::cmp::min(until_idle, until_deadline);

        match tokio::time::timeout(wait_duration, receiver.recv()).await {
            Ok(Ok(TerminalEvent::Output(chunk))) => {
                append_tail(
                    &mut output,
                    chunk.as_str(),
                    max_output_chars,
                    &mut truncated,
                );
                last_output_at = Instant::now();
            }
            Ok(Ok(TerminalEvent::Exit(code))) => {
                append_tail(
                    &mut output,
                    format!("\n[terminal exited with code {code}]\n").as_str(),
                    max_output_chars,
                    &mut truncated,
                );
                break "terminal_exit";
            }
            Ok(Ok(TerminalEvent::State(_))) => {}
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                // Lagged means output is still flowing; treat it as activity so we
                // do not declare idle while the command is still running.
                last_output_at = Instant::now();
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                break "receiver_closed";
            }
            Err(_) => {
                if start.elapsed() >= max_wait {
                    break "max_wait_timeout";
                }
                break "idle_timeout";
            }
        }
    };

    OutputCapture {
        output,
        truncated,
        finished_by,
    }
}

fn append_tail(output: &mut String, chunk: &str, max_chars: usize, truncated: &mut bool) {
    if chunk.is_empty() {
        return;
    }
    output.push_str(chunk);
    let char_count = output.chars().count();
    if char_count <= max_chars {
        return;
    }
    *truncated = true;
    let tail: String = output
        .chars()
        .rev()
        .take(max_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    *output = tail;
}

async fn resolve_project_root(ctx: &BoundContext) -> Result<(Option<String>, PathBuf), String> {
    if let Some(project_id) = ctx.project_id.as_deref() {
        let project = ProjectService::get_by_id(project_id)
            .await?
            .ok_or_else(|| format!("project not found: {}", project_id))?;
        let root = canonicalize_path(Path::new(project.root_path.as_str()))?;
        return Ok((Some(project.id), root));
    }

    let root = canonicalize_path(ctx.root.as_path())?;
    if let Some(found) = infer_project_id_from_root(root.as_path(), ctx.user_id.as_deref()).await {
        return Ok((Some(found), root));
    }
    Ok((None, root))
}

async fn infer_project_id_from_root(root: &Path, user_id: Option<&str>) -> Option<String> {
    let list = ProjectService::list(user_id.map(|v| v.to_string()))
        .await
        .ok()?;
    for project in list {
        let p = PathBuf::from(project.root_path.as_str());
        if let Ok(project_root) = canonicalize_path(p.as_path()) {
            if same_path(project_root.as_path(), root) {
                return Some(project.id);
            }
        }
    }
    None
}

fn resolve_target_path(project_root: &Path, path_input: &str) -> Result<PathBuf, String> {
    let trimmed = path_input.trim();
    let candidate = if trimmed.is_empty() || trimmed == "." {
        project_root.to_path_buf()
    } else if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        project_root.join(trimmed)
    };

    if !candidate.exists() {
        return Err(format!("path does not exist: {}", candidate.display()));
    }
    if !candidate.is_dir() {
        return Err(format!("path is not a directory: {}", candidate.display()));
    }

    let canonical = canonicalize_path(candidate.as_path())?;
    if !is_path_within_root(canonical.as_path(), project_root) {
        return Err(format!(
            "path escaped project root: {} not in {}",
            canonical.display(),
            project_root.display()
        ));
    }
    Ok(canonical)
}

fn build_input_payload(project_root: &Path, target_path: &Path, command: &str) -> String {
    let mut payload = String::new();
    payload.push_str(cd_command_for_path(project_root).as_str());

    if !same_path(target_path, project_root) {
        payload.push_str(cd_command_for_path(target_path).as_str());
    }

    let normalized_command = normalize_shell_input(command);
    payload.push_str(normalized_command.as_str());
    if !normalized_command.ends_with('\n') && !normalized_command.ends_with('\r') {
        payload.push_str(shell_input_newline());
    }

    payload
}

async fn list_terminals_for_context(ctx: &BoundContext) -> Result<Vec<Terminal>, String> {
    let mut terminals = TerminalService::list(ctx.user_id.clone()).await?;
    terminals.retain(|terminal| {
        if let Some(pid) = ctx.project_id.as_deref() {
            terminal.project_id.as_deref() == Some(pid)
        } else {
            terminal_cwd_in_root(terminal.cwd.as_str(), ctx.root.as_path())
        }
    });
    terminals.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    Ok(terminals)
}

async fn find_idle_terminal(
    project_id: &Option<String>,
    project_root: &Path,
    user_id: Option<&str>,
) -> Result<Option<Terminal>, String> {
    let terminals = TerminalService::list(user_id.map(|v| v.to_string())).await?;
    let manager = get_terminal_manager();

    for terminal in terminals {
        if terminal.status == "exited" {
            continue;
        }

        if let Some(pid) = project_id.as_deref() {
            if terminal.project_id.as_deref() != Some(pid) {
                continue;
            }
        } else if !terminal_cwd_in_root(terminal.cwd.as_str(), project_root) {
            continue;
        }

        let busy = manager.get_busy(terminal.id.as_str()).unwrap_or(false);
        if !busy {
            return Ok(Some(terminal));
        }
    }

    Ok(None)
}

fn terminal_cwd_in_root(cwd: &str, root: &Path) -> bool {
    let path = PathBuf::from(cwd);
    let canonical = match canonicalize_path(path.as_path()) {
        Ok(v) => v,
        Err(_) => return false,
    };
    is_path_within_root(canonical.as_path(), root)
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path)
        .map(normalize_canonical_path)
        .map_err(|err| format!("canonicalize {} failed: {}", path.display(), err))
}

fn cd_command_for_path(path: &Path) -> String {
    if cfg!(windows) {
        return format!("cd /d {}{}", shell_quote_path(path), shell_input_newline());
    }
    format!("cd {}{}", shell_quote_path(path), shell_input_newline())
}

fn shell_input_newline() -> &'static str {
    if cfg!(windows) {
        "\r"
    } else {
        "\n"
    }
}

fn normalize_shell_input(input: &str) -> String {
    if !cfg!(windows) {
        return input.to_string();
    }

    input.replace("\r\n", "\r").replace('\n', "\r")
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

fn same_path(left: &Path, right: &Path) -> bool {
    canonicalize_path(left)
        .ok()
        .zip(canonicalize_path(right).ok())
        .map(|(a, b)| a == b)
        .unwrap_or(false)
}

fn is_path_within_root(path: &Path, root: &Path) -> bool {
    let root = match canonicalize_path(root) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let path = match canonicalize_path(path) {
        Ok(v) => v,
        Err(_) => return false,
    };
    path == root || path.starts_with(root)
}

fn derive_terminal_name(root: &Path) -> String {
    root.file_name()
        .map(|s| format!("{}-terminal", s.to_string_lossy()))
        .unwrap_or_else(|| "project-terminal".to_string())
}

fn shell_quote_path(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();
    if cfg!(windows) {
        return format!("\"{}\"", raw.replace('"', "\"\""));
    }
    format!("'{}'", raw.replace('"', "\\\"").replace('\'', "'\"'\"'"))
}

fn required_string<'a>(args: &'a Value, field: &str) -> Result<&'a str, String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{field} is required"))
}

fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = required_string(args, field)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{build_input_payload, normalize_shell_input};
    use std::path::Path;

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

    #[test]
    fn build_input_payload_uses_shell_line_endings() {
        let root = if cfg!(windows) {
            Path::new(r"C:\\repo\\sandbox")
        } else {
            Path::new("/tmp/repo/sandbox")
        };

        let payload = build_input_payload(root, root, "echo hi");

        if cfg!(windows) {
            assert!(payload.contains("\r"));
            assert!(!payload.contains("\n"));
        } else {
            assert!(payload.contains("\n"));
        }
    }
}
