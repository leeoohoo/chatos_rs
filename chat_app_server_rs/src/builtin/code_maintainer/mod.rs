mod diff;
mod fs_ops;
mod patch;
mod storage;
mod utils;

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use diff::{build_diff, extract_patch_diffs, extract_patch_targets, read_text_for_diff, DiffInput};
use fs_ops::FsOps;
use patch::apply_patch;
use storage::ChangeLogStore;
use utils::{ensure_dir, format_bytes, generate_id, normalize_name, sha256_bytes};

use crate::core::tool_io::text_result;

pub struct CodeMaintainerOptions {
    pub server_name: String,
    pub root: PathBuf,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
    pub enable_read_tools: bool,
    pub enable_write_tools: bool,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub db_path: Option<String>,
}

#[derive(Clone)]
pub struct CodeMaintainerService {
    tools: HashMap<String, Tool>,
    default_session_id: String,
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
    session_id: &'a str,
    run_id: &'a str,
}

impl CodeMaintainerService {
    pub fn new(opts: CodeMaintainerOptions) -> Result<Self, String> {
        let server_name = normalize_name(&opts.server_name);
        let root = opts.root;
        ensure_dir(&root)
            .map_err(|err| format!("create workspace dir {} failed: {}", root.display(), err))?;

        let change_log = ChangeLogStore::new(&server_name, opts.db_path.clone())?;
        let change_log = Arc::new(Mutex::new(change_log));

        let fs_ops = FsOps::new(
            root.clone(),
            opts.allow_writes,
            opts.max_file_bytes,
            opts.max_write_bytes,
            opts.search_limit,
        );

        let default_session_id = opts
            .session_id
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| generate_id("session"));
        let default_run_id = opts.run_id.unwrap_or_default();

        let mut service = Self {
            tools: HashMap::new(),
            default_session_id,
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
                &format!("Return UTF-8 file content without line numbers.\n{workspace_note}"),
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "required": ["path"]
                }),
                Arc::new(move |args, _ctx| {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("path is required".to_string())?;
                    let (path, size, sha256, content) = fs_ops.read_file_raw(path)?;
                    Ok(text_result(json!({
                        "path": path,
                        "size_bytes": size,
                        "sha256": sha256,
                        "content": content
                    })))
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
                            result.bytes,
                            &result.sha256,
                            ctx.session_id,
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
                            result.bytes,
                            &result.sha256,
                            ctx.session_id,
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
                    let deleted_path = fs_ops.delete_path(path)?;
                    let diff = build_diff(before_snapshot, after_snapshot);
                    let record = change_log
                        .lock()
                        .map_err(|_| "change log unavailable".to_string())?
                        .log_change(
                            &deleted_path,
                            "delete",
                            0,
                            "",
                            ctx.session_id,
                            ctx.run_id,
                            diff,
                        )?;
                    Ok(text_result(
                        json!({ "result": { "path": deleted_path }, "change": record }),
                    ))
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
                    "Apply a patch to one or more files.\nPatch format uses *** Begin Patch / *** Update File / *** Add File / *** Delete File / *** End Patch.\n{}.\n{workspace_note}",
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
                        for path in result.updated.iter().chain(result.added.iter()) {
                            let full_path = fs_ops.resolve_path(path)?;
                            let content = std::fs::read(&full_path).map_err(|err| err.to_string())?;
                            let hash = sha256_bytes(&content);
                            let before_snapshot = before_snapshots
                                .remove(path)
                                .unwrap_or_else(|| DiffInput::text(String::new()));
                            let after_snapshot = read_text_for_diff(&full_path, max_file_bytes)
                                .unwrap_or_else(DiffInput::omitted);
                            let diff = build_diff(before_snapshot, after_snapshot)
                                .or_else(|| patch_diffs.get(path).cloned());
                            store.log_change(
                                path,
                                "write",
                                content.len() as i64,
                                &hash,
                                ctx.session_id,
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
                            store.log_change(path, "delete", 0, "", ctx.session_id, ctx.run_id, diff)?;
                        }
                    }

                    Ok(text_result(json!({ "result": result, "files": hashes })))
                }),
            );
        }

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
        let session = session_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(self.default_session_id.as_str());
        let run = if self.default_run_id.trim().is_empty() {
            session
        } else {
            self.default_run_id.as_str()
        };
        let ctx = ToolContext {
            session_id: session,
            run_id: run,
        };
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
