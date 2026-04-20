mod diff;
mod edit;
mod fs_ops;
mod patch;
mod storage;
mod utils;

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use diff::{build_diff, extract_patch_diffs, extract_patch_targets, read_text_for_diff, DiffInput};
use edit::{apply_edit_text, EditRequest};
use fs_ops::FsOps;
use patch::apply_patch;
use storage::ChangeLogStore;
use utils::{ensure_dir, format_bytes, generate_id, normalize_name, sha256_bytes};

use crate::core::tool_io::text_result;

pub struct CodeMaintainerOptions {
    pub server_name: String,
    pub root: PathBuf,
    pub project_id: Option<String>,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
    pub enable_read_tools: bool,
    pub enable_write_tools: bool,
    pub conversation_id: Option<String>,
    pub run_id: Option<String>,
    pub db_path: Option<String>,
}

#[derive(Clone)]
pub struct CodeMaintainerService {
    tools: HashMap<String, Tool>,
    default_conversation_id: String,
    default_run_id: String,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

struct ToolContext<'a> {
    conversation_id: &'a str,
    run_id: &'a str,
}

impl CodeMaintainerService {
    pub fn new(opts: CodeMaintainerOptions) -> Result<Self, String> {
        let server_name = normalize_name(&opts.server_name);
        let root = opts.root;
        ensure_dir(&root)
            .map_err(|err| format!("create workspace dir {} failed: {}", root.display(), err))?;

        let change_log =
            ChangeLogStore::new(&server_name, opts.project_id.clone(), opts.db_path.clone())?;
        let change_log = Arc::new(Mutex::new(change_log));

        let fs_ops = FsOps::new(
            root.clone(),
            opts.allow_writes,
            opts.max_file_bytes,
            opts.max_write_bytes,
            opts.search_limit,
        );

        let default_conversation_id = opts
            .conversation_id
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| generate_id("conversation"));
        let default_run_id = opts.run_id.unwrap_or_default();

        let mut service = Self {
            tools: HashMap::new(),
            default_conversation_id,
            default_run_id,
        };

        let workspace_note = format!(
            "Workspace root: {}. Paths must stay inside this directory.",
            root.display()
        );
        let writes_note = if opts.allow_writes {
            "Writes enabled"
        } else {
            "Writes disabled"
        };
        let enable_read_tools = opts.enable_read_tools;
        let enable_write_tools = opts.enable_write_tools;

        if !enable_read_tools && !enable_write_tools {
            return Err("No tools are enabled for this code maintainer instance".to_string());
        }

        if enable_read_tools {
            let fs_ops = fs_ops.clone();
            service.register_tool(
                "read_file_raw",
                &format!(
                    "Return UTF-8 file content.\nwith_line_numbers defaults to true; set false to skip structured numbered lines.\n{workspace_note}"
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "with_line_numbers": { "type": "boolean", "default": true }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }),
                Arc::new(move |args, _ctx| {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("path is required".to_string())?;
                    let with_line_numbers = args
                        .get("with_line_numbers")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    let (path, size, sha256, content) = fs_ops.read_file_raw(path)?;
                    let normalized_lines: Vec<String> = content
                        .split('\n')
                        .map(|line| line.trim_end_matches('\r').to_string())
                        .collect();
                    let line_count = normalized_lines.len();
                    let ends_with_newline = content.ends_with('\n');
                    let numbered_lines = if with_line_numbers {
                        Some(
                            normalized_lines
                                .iter()
                                .enumerate()
                                .map(|(idx, text)| {
                                    json!({
                                        "line": idx + 1,
                                        "text": text
                                    })
                                })
                                .collect::<Vec<Value>>(),
                        )
                    } else {
                        None
                    };
                    let mut payload = json!({
                        "path": path,
                        "size_bytes": size,
                        "sha256": sha256,
                        "line_count": line_count,
                        "ends_with_newline": ends_with_newline,
                        "content": content
                    });
                    if let Some(lines) = numbered_lines {
                        payload["numbered_lines"] = Value::Array(lines);
                    }
                    Ok(text_result(payload))
                }),
            );
        }

        if enable_read_tools {
            let fs_ops = fs_ops.clone();
            let max_file_bytes = opts.max_file_bytes;
            service.register_tool(
                "read_file_range",
                &format!(
                    "Return UTF-8 content from start_line to end_line (1-based, inclusive).\nFile size limit: {}.\n{workspace_note}",
                    format_bytes(max_file_bytes)
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "end_line": { "type": "integer", "minimum": 1 },
                        "with_line_numbers": { "type": "boolean" }
                    },
                    "additionalProperties": false,
                    "required": ["path", "start_line", "end_line"]
                }),
                Arc::new(move |args, _ctx| {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("path is required".to_string())?;
                    let start_line = args
                        .get("start_line")
                        .and_then(|v| v.as_u64())
                        .ok_or("start_line is required".to_string())? as usize;
                    let end_line = args
                        .get("end_line")
                        .and_then(|v| v.as_u64())
                        .ok_or("end_line is required".to_string())? as usize;
                    let with_numbers = args
                        .get("with_line_numbers")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let (path, size, sha256, start, end, total, content) =
                        fs_ops.read_file_range(path, start_line, end_line, with_numbers)?;
                    Ok(text_result(json!({
                        "path": path,
                        "size_bytes": size,
                        "sha256": sha256,
                        "start_line": start,
                        "end_line": end,
                        "total_lines": total,
                        "content": content
                    })))
                }),
            );
        }

        if enable_read_tools {
            let fs_ops = fs_ops.clone();
            service.register_tool(
                "list_dir",
                &format!("List directory entries.\n{workspace_note}"),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "max_entries": { "type": "integer", "minimum": 1, "maximum": 1000 }
                    },
                    "additionalProperties": false
                }),
                Arc::new(move |args, _ctx| {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let max_entries = args
                        .get("max_entries")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                        .unwrap_or(200);
                    let entries = fs_ops.list_dir(path, max_entries)?;
                    Ok(text_result(json!({ "entries": entries })))
                }),
            );
        }

        if enable_read_tools {
            let fs_ops = fs_ops.clone();
            service.register_tool(
                "search_text",
                &format!("Search text recursively under a directory.\n{workspace_note}"),
                json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "minLength": 1 },
                        "path": { "type": "string" },
                        "max_results": { "type": "integer", "minimum": 1, "maximum": 500 }
                    },
                    "additionalProperties": false,
                    "required": ["pattern"]
                }),
                Arc::new(move |args, _ctx| {
                    let pattern = args
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .ok_or("pattern is required".to_string())?;
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let max_results = args
                        .get("max_results")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);
                    let results = fs_ops.search_text(pattern, path, max_results)?;
                    Ok(text_result(
                        json!({ "count": results.len(), "results": results }),
                    ))
                }),
            );
        }

        if enable_write_tools {
            let fs_ops = fs_ops.clone();
            let change_log = change_log.clone();
            let max_file_bytes = opts.max_file_bytes;
            service.register_tool(
                "write_file",
                &format!(
                    "Write file content (overwrite).\nMax write bytes: {}.\n{}.\n{workspace_note}",
                    format_bytes(opts.max_write_bytes),
                    writes_note
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "required": ["path", "content"]
                }),
                Arc::new(move |args, ctx| {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("path is required".to_string())?;
                    let content = args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .ok_or("content is required".to_string())?;
                    let target = fs_ops.resolve_path(path)?;
                    let existed_before = target.exists();
                    let before_snapshot = read_text_for_diff(&target, max_file_bytes)
                        .unwrap_or_else(DiffInput::omitted);
                    let result = fs_ops.write_file(path, content)?;
                    let after_snapshot = DiffInput::text(content.to_string());
                    let diff = build_diff(before_snapshot, after_snapshot);
                    let record = change_log
                        .lock()
                        .map_err(|_| "change log unavailable".to_string())?
                        .log_change(
                            &result.path,
                            "write",
                            if existed_before { "edit" } else { "create" },
                            result.bytes,
                            &result.sha256,
                            ctx.conversation_id,
                            ctx.run_id,
                            diff,
                        )?;
                    Ok(text_result(json!({ "result": result, "change": record })))
                }),
            );
        }

        if enable_write_tools {
            let fs_ops = fs_ops.clone();
            let change_log = change_log.clone();
            service.register_tool(
                "edit_file",
                &format!(
                    "Safely edit file content by replacing old_text with new_text.\nWhen old_text appears multiple times, you MUST provide more surrounding context (before_context / after_context, recommended 1-3 lines) or narrow start_line/end_line.\n{workspace_note}"
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "old_text": { "type": "string", "minLength": 1 },
                        "new_text": { "type": "string" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "end_line": { "type": "integer", "minimum": 1 },
                        "before_context": { "type": "string" },
                        "after_context": { "type": "string" },
                        "expected_matches": { "type": "integer", "minimum": 1 }
                    },
                    "additionalProperties": false,
                    "required": ["path", "old_text", "new_text"]
                }),
                Arc::new(move |args, ctx| {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("path is required".to_string())?;
                    let old_text = args
                        .get("old_text")
                        .and_then(|v| v.as_str())
                        .ok_or("old_text is required".to_string())?;
                    let new_text = args
                        .get("new_text")
                        .and_then(|v| v.as_str())
                        .ok_or("new_text is required".to_string())?;
                    let start_line = args
                        .get("start_line")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);
                    let end_line = args
                        .get("end_line")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);
                    let before_context = args
                        .get("before_context")
                        .and_then(|v| v.as_str());
                    let after_context = args
                        .get("after_context")
                        .and_then(|v| v.as_str());
                    let expected_matches = args
                        .get("expected_matches")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);

                    let (_resolved_path, _size, _sha, content) = fs_ops.read_file_raw(path)?;
                    let edit_result = apply_edit_text(
                        &content,
                        EditRequest {
                            old_text,
                            new_text,
                            start_line,
                            end_line,
                            before_context,
                            after_context,
                            expected_matches,
                        },
                    )?;

                    let updated_content = edit_result.content.clone();
                    let write_result = fs_ops.write_file(path, &updated_content)?;
                    let diff = build_diff(DiffInput::text(content), DiffInput::text(updated_content));
                    let record = change_log
                        .lock()
                        .map_err(|_| "change log unavailable".to_string())?
                        .log_change(
                            &write_result.path,
                            "edit_file",
                            "edit",
                            write_result.bytes,
                            &write_result.sha256,
                            ctx.conversation_id,
                            ctx.run_id,
                            diff,
                        )?;
                    Ok(text_result(json!({
                        "result": write_result,
                        "match": edit_result.info,
                        "change": record
                    })))
                }),
            );
        }

        if enable_write_tools {
            let fs_ops = fs_ops.clone();
            let change_log = change_log.clone();
            let max_file_bytes = opts.max_file_bytes;
            service.register_tool(
                "append_file",
                &format!(
                    "Append content to file.\nMax write bytes: {}.\n{}.\n{workspace_note}",
                    format_bytes(opts.max_write_bytes),
                    writes_note
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "required": ["path", "content"]
                }),
                Arc::new(move |args, ctx| {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("path is required".to_string())?;
                    let content = args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .ok_or("content is required".to_string())?;
                    let target = fs_ops.resolve_path(path)?;
                    let existed_before = target.exists();
                    let before_snapshot = read_text_for_diff(&target, max_file_bytes)
                        .unwrap_or_else(DiffInput::omitted);
                    let after_snapshot = if let Some(reason) = before_snapshot.reason.clone() {
                        DiffInput::omitted(reason)
                    } else {
                        let mut next = before_snapshot.text.clone().unwrap_or_default();
                        next.push_str(content);
                        DiffInput::text(next)
                    };
                    let result = fs_ops.append_file(path, content)?;
                    let diff = build_diff(before_snapshot, after_snapshot);
                    let record = change_log
                        .lock()
                        .map_err(|_| "change log unavailable".to_string())?
                        .log_change(
                            &result.path,
                            "append",
                            if existed_before { "edit" } else { "create" },
                            result.bytes,
                            &result.sha256,
                            ctx.conversation_id,
                            ctx.run_id,
                            diff,
                        )?;
                    Ok(text_result(json!({ "result": result, "change": record })))
                }),
            );
        }

        if enable_write_tools {
            let fs_ops = fs_ops.clone();
            let change_log = change_log.clone();
            let max_file_bytes = opts.max_file_bytes;
            service.register_tool(
                "delete_path",
                &format!(
                    "Delete a file or directory.\n{}.\n{workspace_note}",
                    writes_note
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }),
                Arc::new(move |args, ctx| {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("path is required".to_string())?;
                    let target = fs_ops.resolve_path(path)?;
                    let before_snapshot = read_text_for_diff(&target, max_file_bytes)
                        .unwrap_or_else(DiffInput::omitted);
                    let after_snapshot = if let Some(reason) = before_snapshot.reason.clone() {
                        DiffInput::omitted(reason)
                    } else {
                        DiffInput::text(String::new())
                    };
                    let delete_result = fs_ops.delete_path(path)?;
                    let exists_after_delete = target.exists();
                    if delete_result.deleted && exists_after_delete {
                        return Err(format!(
                            "Delete reported success but path still exists: {}",
                            delete_result.path
                        ));
                    }
                    if !delete_result.deleted {
                        return Ok(text_result(json!({
                            "result": {
                                "path": delete_result.path,
                                "deleted": false,
                                "exists_after_delete": exists_after_delete,
                                "already_absent": true
                            },
                            "message": "Path already absent. No file-system change was applied.",
                            "hint": "Verify the exact path with list_dir before retrying delete."
                        })));
                    }
                    let diff = build_diff(before_snapshot, after_snapshot);
                    let record = change_log
                        .lock()
                        .map_err(|_| "change log unavailable".to_string())?
                        .log_change(
                            &delete_result.path,
                            "delete",
                            "delete",
                            0,
                            "",
                            ctx.conversation_id,
                            ctx.run_id,
                            diff,
                        )?;
                    Ok(text_result(json!({
                        "result": {
                            "path": delete_result.path,
                            "deleted": true,
                            "exists_after_delete": exists_after_delete,
                            "already_absent": false
                        },
                        "change": record
                    })))
                }),
            );
        }

        if enable_write_tools {
            let change_log = change_log.clone();
            let fs_ops = fs_ops.clone();
            let root = root.clone();
            let allow_writes = opts.allow_writes;
            let max_file_bytes = opts.max_file_bytes;
            service.register_tool(
                "apply_patch",
                &format!(
                    "Apply a patch to one or more files.\nSupported format A (recommended): *** Begin Patch / *** Update File / *** Add File / *** Delete File / *** End Patch.\nSupported format B (stable text replace):\nUpdate File --- path/to/file\n<old content>\n+++ path/to/file\n<new content>\nEnd Patch\nFormat B requires old content to match uniquely in the file.\n{}.\n{workspace_note}",
                    writes_note
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "patch": { "type": "string", "minLength": 1 }
                    },
                    "additionalProperties": false,
                    "required": ["patch"]
                }),
                Arc::new(move |args, ctx| {
                    let patch_text = args
                        .get("patch")
                        .and_then(|v| v.as_str())
                        .ok_or("patch is required".to_string())?;
                    let patch_diffs: HashMap<String, String> = extract_patch_diffs(patch_text);
                    let patch_targets = extract_patch_targets(patch_text);
                    let mut before_snapshots: HashMap<String, DiffInput> = HashMap::new();
                    for target in patch_targets {
                        let before_path = fs_ops.resolve_path(&target.before_path)?;
                        let before_snapshot = read_text_for_diff(&before_path, max_file_bytes)
                            .unwrap_or_else(DiffInput::omitted);
                        before_snapshots.insert(target.after_path, before_snapshot);
                    }
                    let result = apply_patch(&root, patch_text, allow_writes)?;
                    let mut hashes = Vec::new();

                    {
                        let store = change_log
                            .lock()
                            .map_err(|_| "change log unavailable".to_string())?;
                        for path in &result.updated {
                            let full_path = fs_ops.resolve_path(path)?;
                            let content = std::fs::read(&full_path).map_err(|err| err.to_string())?;
                            let hash = sha256_bytes(&content);
                            let before_snapshot = before_snapshots
                                .remove(path)
                                .unwrap_or(DiffInput {
                                    text: None,
                                    reason: None,
                                });
                            let after_snapshot = read_text_for_diff(&full_path, max_file_bytes)
                                .unwrap_or_else(DiffInput::omitted);
                            let change_kind = if before_snapshot.text.is_none()
                                && before_snapshot.reason.is_none()
                            {
                                "create"
                            } else {
                                "edit"
                            };
                            let diff = build_diff(before_snapshot, after_snapshot)
                                .or_else(|| patch_diffs.get(path).cloned());
                            store.log_change(
                                path,
                                "write",
                                change_kind,
                                content.len() as i64,
                                &hash,
                                ctx.conversation_id,
                                ctx.run_id,
                                diff,
                            )?;
                            hashes.push(json!({ "path": path, "sha256": hash }));
                        }

                        for path in &result.added {
                            let full_path = fs_ops.resolve_path(path)?;
                            let content = std::fs::read(&full_path).map_err(|err| err.to_string())?;
                            let hash = sha256_bytes(&content);
                            let before_snapshot = before_snapshots
                                .remove(path)
                                .unwrap_or(DiffInput {
                                    text: None,
                                    reason: None,
                                });
                            let after_snapshot = read_text_for_diff(&full_path, max_file_bytes)
                                .unwrap_or_else(DiffInput::omitted);
                            let diff = build_diff(before_snapshot, after_snapshot)
                                .or_else(|| patch_diffs.get(path).cloned());
                            store.log_change(
                                path,
                                "write",
                                "create",
                                content.len() as i64,
                                &hash,
                                ctx.conversation_id,
                                ctx.run_id,
                                diff,
                            )?;
                            hashes.push(json!({ "path": path, "sha256": hash }));
                        }

                        for path in &result.deleted {
                            let before_snapshot = before_snapshots
                                .remove(path)
                                .unwrap_or_else(|| DiffInput::text(String::new()));
                            let after_snapshot = DiffInput::text(String::new());
                            let diff = build_diff(before_snapshot, after_snapshot)
                                .or_else(|| patch_diffs.get(path).cloned());
                            store.log_change(
                                path,
                                "delete",
                                "delete",
                                0,
                                "",
                                ctx.conversation_id,
                                ctx.run_id,
                                diff,
                            )?;
                        }
                    }

                    Ok(text_result(json!({ "result": result, "files": hashes })))
                }),
            );
        }

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        let mut tools: Vec<Value> = self
            .tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect();

        if self.tools.contains_key("read_file_raw") {
            tools.push(json!({
                "name": "read_file",
                "description": "Hermes-compatible alias. Read full file by default; when start_line + end_line are both provided, read a line range.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "end_line": { "type": "integer", "minimum": 1 },
                        "with_line_numbers": { "type": "boolean", "default": true }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }
            }));
        }

        if self.tools.contains_key("search_text") {
            tools.push(json!({
                "name": "search_files",
                "description": "Hermes-compatible alias of search_text. query maps to search_text.pattern.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "minLength": 1 },
                        "path": { "type": "string" },
                        "max_results": { "type": "integer", "minimum": 1, "maximum": 500 }
                    },
                    "additionalProperties": false,
                    "required": ["query"]
                }
            }));
        }

        if self.tools.contains_key("apply_patch") {
            tools.push(json!({
                "name": "patch",
                "description": "Hermes-compatible alias of apply_patch.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "patch": { "type": "string", "minLength": 1 }
                    },
                    "additionalProperties": false,
                    "required": ["patch"]
                }
            }));
        }

        tools
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        conversation_id: Option<&str>,
    ) -> Result<Value, String> {
        let conversation = conversation_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(self.default_conversation_id.as_str());
        let run = if self.default_run_id.trim().is_empty() {
            conversation
        } else {
            self.default_run_id.as_str()
        };
        let ctx = ToolContext {
            conversation_id: conversation,
            run_id: run,
        };

        if name == "read_file" {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("path is required".to_string())?;
            let start_line = args
                .get("start_line")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let end_line = args
                .get("end_line")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let with_line_numbers = args
                .get("with_line_numbers")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let mapped = match (start_line, end_line) {
                (Some(start_line), Some(end_line)) => {
                    let range_tool = self
                        .tools
                        .get("read_file_range")
                        .ok_or_else(|| "Tool not found: read_file_range".to_string())?;
                    let mapped_args = json!({
                        "path": path,
                        "start_line": start_line,
                        "end_line": end_line,
                        "with_line_numbers": with_line_numbers
                    });
                    return (range_tool.handler)(mapped_args, &ctx);
                }
                (None, None) => json!({
                    "path": path,
                    "with_line_numbers": with_line_numbers
                }),
                _ => {
                    return Err(
                        "start_line and end_line must be provided together for read_file"
                            .to_string(),
                    );
                }
            };
            let raw_tool = self
                .tools
                .get("read_file_raw")
                .ok_or_else(|| "Tool not found: read_file_raw".to_string())?;
            return (raw_tool.handler)(mapped, &ctx);
        }

        if name == "search_files" {
            let query = args
                .get("query")
                .or_else(|| args.get("pattern"))
                .and_then(|v| v.as_str())
                .ok_or("query is required".to_string())?;
            let search_tool = self
                .tools
                .get("search_text")
                .ok_or_else(|| "Tool not found: search_text".to_string())?;

            let mut mapped = json!({
                "pattern": query
            });
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                mapped["path"] = json!(path);
            }
            if let Some(max_results) = args.get("max_results").and_then(|v| v.as_u64()) {
                mapped["max_results"] = json!(max_results);
            }
            return (search_tool.handler)(mapped, &ctx);
        }

        let mapped_name = if name == "patch" { "apply_patch" } else { name };
        let tool = self
            .tools
            .get(mapped_name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, &ctx)
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::{CodeMaintainerOptions, CodeMaintainerService};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos();
        path.push(format!("{prefix}_{nonce}"));
        path
    }

    fn build_service(enable_write_tools: bool) -> (CodeMaintainerService, PathBuf) {
        let root = unique_temp_dir("code_maintainer_alias_workspace");
        let db_path = unique_temp_dir("code_maintainer_alias_db")
            .join("changes.jsonl")
            .to_string_lossy()
            .to_string();
        let service = CodeMaintainerService::new(CodeMaintainerOptions {
            server_name: "code_maintainer_alias_test".to_string(),
            root: root.clone(),
            project_id: Some("project_alias".to_string()),
            allow_writes: enable_write_tools,
            max_file_bytes: 256 * 1024,
            max_write_bytes: 1024 * 1024,
            search_limit: 40,
            enable_read_tools: true,
            enable_write_tools,
            conversation_id: Some("conversation_alias".to_string()),
            run_id: Some("run_alias".to_string()),
            db_path: Some(db_path),
        })
        .expect("build code maintainer service");
        (service, root)
    }

    fn response_text(value: &serde_json::Value) -> String {
        value
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn list_tools_contains_hermes_compat_aliases() {
        let (service, _root) = build_service(true);
        let tools = service.list_tools();
        let names: Vec<String> = tools
            .iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        assert!(names.iter().any(|name| name == "read_file"));
        assert!(names.iter().any(|name| name == "search_files"));
        assert!(names.iter().any(|name| name == "patch"));
    }

    #[test]
    fn read_file_alias_supports_full_and_range_modes() {
        let (service, root) = build_service(false);
        let file_path = root.join("src").join("lib.rs");
        fs::create_dir_all(file_path.parent().expect("parent")).expect("create parent");
        fs::write(&file_path, "line1\nline2\nline3\n").expect("write source file");

        let full = service
            .call_tool("read_file", json!({ "path": "src/lib.rs" }), None)
            .expect("read full");
        let full_text = response_text(&full);
        assert!(full_text.contains("\"line_count\": 4"));

        let range = service
            .call_tool(
                "read_file",
                json!({ "path": "src/lib.rs", "start_line": 2, "end_line": 3 }),
                None,
            )
            .expect("read range");
        let range_text = response_text(&range);
        assert!(range_text.contains("\"start_line\": 2"));
        assert!(range_text.contains("line2"));
    }

    #[test]
    fn search_files_alias_maps_query_to_search_text_pattern() {
        let (service, root) = build_service(false);
        let file_path = root.join("README.md");
        fs::write(&file_path, "Hermes-compatible alias smoke test").expect("write readme");

        let result = service
            .call_tool(
                "search_files",
                json!({ "query": "alias", "path": "." }),
                None,
            )
            .expect("search files");
        let text = response_text(&result);
        assert!(text.contains("\"count\": 1"));
        assert!(text.contains("README.md"));
    }

    #[test]
    fn patch_alias_maps_to_apply_patch() {
        let (service, root) = build_service(true);
        let patch_text =
            "*** Begin Patch\n*** Add File: alias_patch.txt\n+hello from alias\n*** End Patch\n";
        service
            .call_tool("patch", json!({ "patch": patch_text }), None)
            .expect("apply patch via alias");

        let created = root.join("alias_patch.txt");
        let content = fs::read_to_string(created).expect("read created file");
        assert_eq!(content.trim(), "hello from alias");
    }
}
