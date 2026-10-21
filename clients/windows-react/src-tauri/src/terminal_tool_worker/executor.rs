use chatos_local_agent_protocol::{
    LocalAgentToolApprovalStatus, LocalAgentToolClaim, LocalAgentToolExecutionRoute,
    LocalAgentToolOutcome,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use tauri::{AppHandle, Manager};

use crate::{
    host_lifecycle::HostManager,
    platform_execution_context::{
        resolve_execution_directory, resolve_platform_execution_context, resolve_terminal_command,
        resolve_terminal_path, resolve_terminal_purpose,
    },
    terminal_runtime::{ExecuteRequest, TerminalProcessStore, TerminalRuntimeError, TerminalScope},
};

use super::code_write::{CodeWriteStore, EditScope};

pub(super) async fn execute(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
    processes: &TerminalProcessStore,
    edits: &CodeWriteStore,
) -> LocalAgentToolOutcome {
    match execute_checked(app, owner_user_id, claim, processes, edits).await {
        Ok(output) => LocalAgentToolOutcome::Succeeded { output },
        Err(error) if claim.invocation.side_effecting && !error.known_pre_effect => {
            LocalAgentToolOutcome::NeedsReview {
                reason: safe_error(&error.message),
                detail: error_detail(&claim.invocation.tool_name),
            }
        }
        Err(error) => LocalAgentToolOutcome::Failed {
            error: safe_error(&error.message),
            detail: error_detail(&claim.invocation.tool_name),
        },
    }
}

async fn execute_checked(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
    processes: &TerminalProcessStore,
    edits: &CodeWriteStore,
) -> Result<Value, ExecutionError> {
    let invocation = &claim.invocation;
    if invocation.execution_route != LocalAgentToolExecutionRoute::Platform
        || !super::PLATFORM_TOOL_NAMES.contains(&invocation.tool_name.as_str())
    {
        return Err(ExecutionError::pre_effect(
            "The requested local platform tool is unavailable.",
        ));
    }
    let context = resolve_platform_execution_context(
        app,
        &app.state::<HostManager>(),
        owner_user_id,
        &invocation.run_id,
    )
    .await
    .map_err(|_| ExecutionError::pre_effect("The local platform tool Run context is invalid."))?;
    let project_read = is_project_read_tool(&invocation.tool_name);
    let project_write = is_project_write_tool(&invocation.tool_name);
    if (project_read && !context.authorization.allows_project_read())
        || (project_write && !context.authorization.allows_project_write())
        || (!project_read && !project_write && !context.authorization.allows_terminal())
    {
        return Err(ExecutionError::pre_effect(
            "This Task did not select the requested capability.",
        ));
    }
    if matches!(invocation.tool_name.as_str(), "commit_edit_session")
        && (!invocation.requires_approval
            || invocation.approval_status != LocalAgentToolApprovalStatus::Approved)
    {
        return Err(ExecutionError::pre_effect(
            "The staged project edit requires approval before commit.",
        ));
    }
    if matches!(
        invocation.tool_name.as_str(),
        "execute_command" | "process_write" | "process_kill"
    ) && (!invocation.requires_approval
        || invocation.approval_status != LocalAgentToolApprovalStatus::Approved)
    {
        return Err(ExecutionError::pre_effect(
            "The local terminal change requires approval before execution.",
        ));
    }
    let arguments = invocation
        .arguments
        .as_object()
        .ok_or_else(|| ExecutionError::pre_effect("The local platform arguments are invalid."))?;
    if project_read {
        let scope = EditScope {
            owner_user_id: owner_user_id.to_owned(),
            workspace_scope_id: context.workspace_scope_id.clone(),
            conversation_id: context.conversation_id.clone(),
            run_id: invocation.run_id.clone(),
        };
        let root = context.execution_root;
        let tool_name = invocation.tool_name.clone();
        let arguments = arguments.clone();
        let output = tauri::async_runtime::spawn_blocking(move || {
            execute_project_read(&root, &tool_name, &arguments)
        })
        .await
        .map_err(|_| ExecutionError::pre_effect("The project read task exited unexpectedly."))??;
        edits.record_project_read(&scope);
        return Ok(output);
    }
    if project_write {
        let scope = EditScope {
            owner_user_id: owner_user_id.to_owned(),
            workspace_scope_id: context.workspace_scope_id,
            conversation_id: context.conversation_id,
            run_id: invocation.run_id.clone(),
        };
        let root = context.execution_root;
        let owned_paths = context.authorization.owned_paths().to_vec();
        let tool_name = invocation.tool_name.clone();
        let arguments = arguments.clone();
        let edits = edits.clone();
        return tauri::async_runtime::spawn_blocking(move || {
            edits
                .call(&tool_name, &arguments, scope, &root, &owned_paths)
                .map_err(|error| ExecutionError {
                    message: error.message,
                    known_pre_effect: error.known_pre_effect,
                })
        })
        .await
        .map_err(|_| ExecutionError::pre_effect("The project edit task exited unexpectedly."))?;
    }
    let scope = TerminalScope {
        run_id: invocation.run_id.clone(),
        workspace_scope_id: context.workspace_scope_id,
        project_root: context.execution_root.clone(),
    };
    match invocation.tool_name.as_str() {
        "execute_command" => {
            let command = resolve_terminal_command(arguments)
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field("command")))?;
            resolve_terminal_purpose(arguments)
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field("purpose")))?;
            let cwd = resolve_execution_directory(
                &context.execution_root,
                resolve_terminal_path(arguments),
            )
            .map_err(ExecutionError::pre_effect)?;
            let background = optional_bool(arguments, "background")?.unwrap_or(false);
            let timeout_ms = command_timeout(arguments)?;
            processes
                .execute(
                    &scope,
                    ExecuteRequest {
                        command: command.to_owned(),
                        cwd,
                        background,
                        timeout_ms,
                    },
                )
                .await
                .map_err(ExecutionError::from)
        }
        "process_poll" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .poll(
                    &scope,
                    id,
                    optional_u64(arguments, "offset")?,
                    optional_usize(arguments, "limit")?.unwrap_or(80),
                )
                .await
                .map_err(ExecutionError::from)
        }
        "process_log" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .log(
                    &scope,
                    id,
                    optional_u64(arguments, "offset")?,
                    optional_usize(arguments, "limit")?.unwrap_or(200),
                )
                .await
                .map_err(ExecutionError::from)
        }
        "process_wait" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .wait(&scope, id, wait_timeout(arguments)?.unwrap_or(30_000))
                .await
                .map_err(ExecutionError::from)
        }
        "process_write" => {
            let id = required_string(arguments, "terminal_id")?;
            let data = arguments
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field("data")))?;
            let submit = optional_bool(arguments, "submit")?.unwrap_or(false);
            processes
                .write(&scope, id, data, submit)
                .await
                .map_err(ExecutionError::from)
        }
        "process_kill" => {
            let id = required_string(arguments, "terminal_id")?;
            processes
                .kill(&scope, id)
                .await
                .map_err(ExecutionError::from)
        }
        _ => Err(ExecutionError::pre_effect(
            "The requested local platform tool is unavailable.",
        )),
    }
}

const PROJECT_READ_TOOL_NAMES: [&str; 6] = [
    "read_file_raw",
    "read_file_range",
    "list_dir",
    "search_text",
    "read_file",
    "search_files",
];
const PROJECT_WRITE_TOOL_NAMES: [&str; 4] = [
    "open_edit_session",
    "stage_edit_batch",
    "commit_edit_session",
    "abort_edit_session",
];
const MAXIMUM_READ_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_SEARCH_ENTRIES: usize = 100_000;
const IGNORED_DIRECTORIES: [&str; 13] = [
    ".git",
    ".build",
    ".cache",
    ".next",
    ".idea",
    ".vscode",
    "node_modules",
    "DerivedData",
    "Pods",
    "target",
    "dist",
    "build",
    "vendor",
];

fn is_project_read_tool(name: &str) -> bool {
    PROJECT_READ_TOOL_NAMES.contains(&name)
}

fn is_project_write_tool(name: &str) -> bool {
    PROJECT_WRITE_TOOL_NAMES.contains(&name)
}

fn execute_project_read(
    root: &Path,
    tool_name: &str,
    arguments: &Map<String, Value>,
) -> Result<Value, ExecutionError> {
    match tool_name {
        "read_file_raw" => read_file(root, arguments, false),
        "read_file_range" => read_file(root, arguments, true),
        "read_file" => {
            let has_start = arguments.contains_key("start_line");
            let has_end = arguments.contains_key("end_line");
            if has_start != has_end {
                return Err(ExecutionError::pre_effect(
                    "start_line and end_line must be provided together.",
                ));
            }
            read_file(root, arguments, has_start)
        }
        "list_dir" => list_directory(root, arguments),
        "search_text" | "search_files" => search_text(root, tool_name, arguments),
        _ => Err(ExecutionError::pre_effect(
            "The requested project read tool is unavailable.",
        )),
    }
}

fn read_file(
    root: &Path,
    arguments: &Map<String, Value>,
    ranged: bool,
) -> Result<Value, ExecutionError> {
    let requested = required_project_string(arguments, "path", 4_096)?;
    let path = resolve_project_path(root, requested, true)?;
    let (bytes, content) = read_utf8_file(&path)?;
    let relative = relative_path(root, &path)?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let lines = normalized_lines(&content);
    if ranged {
        let start = required_usize(arguments, "start_line")?;
        let requested_end = required_usize(arguments, "end_line")?;
        if start == 0 || requested_end == 0 || requested_end < start {
            return Err(ExecutionError::pre_effect(invalid_project_field(
                "start_line/end_line",
            )));
        }
        let end = requested_end.min(lines.len());
        let numbered = optional_bool(arguments, "with_line_numbers")?.unwrap_or(false);
        let selected = if start <= end {
            lines[start - 1..end]
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    if numbered {
                        format!("{}: {line}", start + index)
                    } else {
                        (*line).to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };
        return Ok(json!({
            "path": relative,
            "size_bytes": bytes.len(),
            "sha256": sha256,
            "start_line": start,
            "end_line": end,
            "total_lines": lines.len(),
            "content": selected,
        }));
    }
    let mut output = json!({
        "path": relative,
        "size_bytes": bytes.len(),
        "sha256": sha256,
        "line_count": lines.len(),
        "ends_with_newline": content.ends_with('\n'),
        "content": content,
    });
    if optional_bool(arguments, "with_line_numbers")?.unwrap_or(true) {
        output["numbered_lines"] = Value::Array(
            lines
                .iter()
                .enumerate()
                .map(|(index, line)| json!({"line": index + 1, "text": line}))
                .collect(),
        );
    }
    Ok(output)
}

fn list_directory(root: &Path, arguments: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let requested = optional_project_path(arguments)?.unwrap_or(".");
    let path = resolve_project_path(root, requested, false)?;
    let maximum = optional_usize(arguments, "max_entries")?
        .unwrap_or(200)
        .clamp(1, 1_000);
    let mut entries = fs::read_dir(&path)
        .map_err(|_| ExecutionError::pre_effect("The project directory is unavailable."))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    let truncated = entries.len() > maximum;
    let entries = entries
        .into_iter()
        .take(maximum)
        .map(|entry| {
            let metadata = entry
                .metadata()
                .map_err(|_| ExecutionError::pre_effect("A project entry is unavailable."))?;
            let entry_path = entry.path();
            Ok(json!({
                "name": entry.file_name().to_string_lossy(),
                "path": relative_path(root, &entry_path)?,
                "type": if metadata.is_dir() { "dir" } else { "file" },
                "size": metadata.len(),
                "mtime_ms": metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|duration| duration.as_millis().min(i64::MAX as u128) as i64),
            }))
        })
        .collect::<Result<Vec<_>, ExecutionError>>()?;
    Ok(json!({"entries": entries, "truncated": truncated}))
}

fn search_text(
    root: &Path,
    tool_name: &str,
    arguments: &Map<String, Value>,
) -> Result<Value, ExecutionError> {
    let pattern_field = if tool_name == "search_files" {
        "query"
    } else {
        "pattern"
    };
    let pattern = required_project_string(arguments, pattern_field, 8_192)?;
    let requested = optional_project_path(arguments)?.unwrap_or(".");
    let start = resolve_project_path(root, requested, false)?;
    let maximum = optional_usize(arguments, "max_results")?
        .unwrap_or(200)
        .clamp(1, 500);
    let mut pending = vec![start];
    let mut visited = 0usize;
    let mut results = Vec::new();
    let mut truncated = false;
    while let Some(path) = pending.pop() {
        if visited >= MAXIMUM_SEARCH_ENTRIES || results.len() >= maximum {
            truncated = true;
            break;
        }
        visited += 1;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| ExecutionError::pre_effect("A project search entry is unavailable."))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if path != root
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| IGNORED_DIRECTORIES.contains(&name))
            {
                continue;
            }
            let mut children = fs::read_dir(&path)
                .map_err(|_| ExecutionError::pre_effect("A project directory is unavailable."))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            children.sort();
            children.reverse();
            pending.extend(children);
            continue;
        }
        if !metadata.is_file() || metadata.len() > MAXIMUM_READ_BYTES {
            continue;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        if fs::File::open(&path)
            .and_then(|file| file.take(MAXIMUM_READ_BYTES + 1).read_to_end(&mut bytes))
            .is_err()
            || bytes.len() as u64 > MAXIMUM_READ_BYTES
        {
            continue;
        }
        let Ok(content) = std::str::from_utf8(&bytes) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                results.push(json!({
                    "path": relative_path(root, &path)?,
                    "line": index + 1,
                    "text": line.chars().take(2_000).collect::<String>(),
                }));
                if results.len() >= maximum {
                    truncated = true;
                    break;
                }
            }
        }
    }
    Ok(json!({"results": results, "truncated": truncated}))
}

fn resolve_project_path(
    root: &Path,
    requested: &str,
    require_file: bool,
) -> Result<PathBuf, ExecutionError> {
    if requested.len() > 4_096 || requested.contains('\0') || requested.trim() != requested {
        return Err(ExecutionError::pre_effect(invalid_project_field("path")));
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|_| ExecutionError::pre_effect("The project root is unavailable."))?;
    let candidate = if requested.is_empty() || requested == "." {
        canonical_root.clone()
    } else if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        canonical_root.join(requested)
    };
    let resolved = fs::canonicalize(candidate)
        .map_err(|_| ExecutionError::pre_effect("The requested project path is unavailable."))?;
    let metadata = resolved
        .metadata()
        .map_err(|_| ExecutionError::pre_effect("The requested project path is unavailable."))?;
    if !resolved.starts_with(&canonical_root)
        || (require_file && !metadata.is_file())
        || (!require_file && !metadata.is_dir())
    {
        return Err(ExecutionError::pre_effect(
            "The requested path is outside the authorized project scope.",
        ));
    }
    Ok(resolved)
}

fn read_utf8_file(path: &Path) -> Result<(Vec<u8>, String), ExecutionError> {
    let metadata = path
        .metadata()
        .map_err(|_| ExecutionError::pre_effect("The project file is unavailable."))?;
    if !metadata.is_file() || metadata.len() > MAXIMUM_READ_BYTES {
        return Err(ExecutionError::pre_effect(
            "The project file exceeds the supported text size.",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .and_then(|file| file.take(MAXIMUM_READ_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| ExecutionError::pre_effect("The project file is unavailable."))?;
    if bytes.len() as u64 > MAXIMUM_READ_BYTES {
        return Err(ExecutionError::pre_effect(
            "The project file exceeds the supported text size.",
        ));
    }
    let content = String::from_utf8(bytes.clone())
        .map_err(|_| ExecutionError::pre_effect("The project file is not UTF-8 text."))?;
    Ok((bytes, content))
}

fn normalized_lines(content: &str) -> Vec<&str> {
    if content.is_empty() {
        Vec::new()
    } else {
        content
            .strip_suffix('\n')
            .unwrap_or(content)
            .split('\n')
            .collect()
    }
}

fn relative_path(root: &Path, path: &Path) -> Result<String, ExecutionError> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|_| ExecutionError::pre_effect("The project root is unavailable."))?;
    path.strip_prefix(canonical_root)
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            }
        })
        .map_err(|_| ExecutionError::pre_effect("The project path is outside its root."))
}

fn optional_project_path(arguments: &Map<String, Value>) -> Result<Option<&str>, ExecutionError> {
    arguments
        .get("path")
        .map(|value| {
            value
                .as_str()
                .filter(|value| value.len() <= 4_096)
                .ok_or_else(|| ExecutionError::pre_effect(invalid_project_field("path")))
        })
        .transpose()
}

fn required_project_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, ExecutionError> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.len() <= maximum
                && !value.contains('\0')
        })
        .ok_or_else(|| ExecutionError::pre_effect(invalid_project_field(field)))
}

fn required_usize(arguments: &Map<String, Value>, field: &str) -> Result<usize, ExecutionError> {
    optional_usize(arguments, field)?
        .filter(|value| *value > 0)
        .ok_or_else(|| ExecutionError::pre_effect(invalid_project_field(field)))
}

fn invalid_project_field(field: &str) -> String {
    format!("The project read field is invalid: {field}.")
}

fn command_timeout(arguments: &Map<String, Value>) -> Result<Option<u64>, ExecutionError> {
    if let Some(value) = optional_u64(arguments, "timeout_ms")? {
        return Ok(Some(value));
    }
    Ok(optional_u64(arguments, "timeout")?.map(|seconds| seconds.saturating_mul(1_000)))
}

fn wait_timeout(arguments: &Map<String, Value>) -> Result<Option<u64>, ExecutionError> {
    command_timeout(arguments)
}

fn required_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, ExecutionError> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 160)
        .ok_or_else(|| ExecutionError::pre_effect(invalid_field(field)))
}

fn optional_bool(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<bool>, ExecutionError> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field(field)))
        })
        .transpose()
}

fn optional_u64(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<u64>, ExecutionError> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| ExecutionError::pre_effect(invalid_field(field)))
        })
        .transpose()
}

fn optional_usize(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<usize>, ExecutionError> {
    optional_u64(arguments, field)
        .map(|value| value.map(|number| usize::try_from(number).unwrap_or(usize::MAX)))
}

fn invalid_field(field: &str) -> String {
    format!("The local terminal field is invalid: {field}.")
}

fn error_detail(tool_name: &str) -> Value {
    json!({
        "tool_name": tool_name,
        "phase": "windows_terminal_execution"
    })
}

fn safe_error(error: &str) -> String {
    let lowered = error.to_ascii_lowercase();
    if [
        "authorization",
        "api_key",
        "apikey",
        "access_token",
        "password",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        "Local terminal execution failed; sensitive details were hidden.".into()
    } else {
        error.chars().take(1_000).collect()
    }
}

#[derive(Debug)]
struct ExecutionError {
    message: String,
    known_pre_effect: bool,
}

impl ExecutionError {
    fn pre_effect(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            known_pre_effect: true,
        }
    }
}

impl From<TerminalRuntimeError> for ExecutionError {
    fn from(error: TerminalRuntimeError) -> Self {
        Self {
            message: error.message,
            known_pre_effect: error.known_pre_effect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_read_tools_are_root_scoped_and_macos_compatible() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {\n    // needle\n}\n").unwrap();
        fs::write(directory.path().join("outside.txt"), "secret").unwrap();

        let listed =
            execute_project_read(&root, "list_dir", json!({"path": "."}).as_object().unwrap())
                .unwrap();
        assert_eq!(listed["entries"][0]["path"], "src");

        let read = execute_project_read(
            &root,
            "read_file_range",
            json!({
                "path": "src/main.rs",
                "start_line": 2,
                "end_line": 2,
                "with_line_numbers": true
            })
            .as_object()
            .unwrap(),
        )
        .unwrap();
        assert_eq!(read["content"], "2:     // needle");

        let searched = execute_project_read(
            &root,
            "search_files",
            json!({"query": "needle", "path": "."}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(searched["results"][0]["path"], "src/main.rs");
        assert_eq!(searched["results"][0]["line"], 2);

        assert!(execute_project_read(
            &root,
            "read_file_raw",
            json!({"path": "../outside.txt"}).as_object().unwrap(),
        )
        .is_err());
    }

    #[test]
    fn project_search_skips_build_and_dependency_directories() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("node_modules/package")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("node_modules/package/index.js"), "needle").unwrap();
        fs::write(root.join("src/index.ts"), "needle").unwrap();
        let searched = execute_project_read(
            root,
            "search_text",
            json!({"pattern": "needle"}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(searched["results"].as_array().unwrap().len(), 1);
        assert_eq!(searched["results"][0]["path"], "src/index.ts");
    }
}
