mod admin_server;
mod diff;
mod fs_ops;
mod mcp;
mod patch;
mod storage;
mod utils;

use crate::admin_server::{run_admin_server, AdminServerOptions};
use crate::diff::{build_diff, extract_patch_diffs, read_text_for_diff, DiffInput};
use crate::fs_ops::FsOps;
use crate::mcp::McpServer;
use crate::patch::apply_patch;
use crate::storage::ChangeLogStore;
use crate::utils::{clamp_number, ensure_dir, format_bytes, generate_id, normalize_id, normalize_name, parse_args, resolve_state_dir, sha256_bytes};
use serde_json::json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;

fn main() {
    let argv: Vec<String> = env::args().skip(1).collect();
    let args = parse_args(&argv);
    if args.flags.contains("help") || args.flags.contains("h") {
        print_help();
        return;
    }

    let server_name = normalize_name(
        args.values
            .get("name")
            .map(String::as_str)
            .unwrap_or("code_maintainer"),
    );
    let root = args
        .values
        .get("root")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let allow_writes = args.flags.contains("write")
        || args
            .values
            .get("mode")
            .map(|v| v.to_lowercase().contains("write"))
            .unwrap_or(false);
    let max_file_bytes = clamp_number(args.values.get("max-bytes"), 1024, 50 * 1024 * 1024, 256 * 1024);
    let max_write_bytes =
        clamp_number(args.values.get("max-write-bytes"), 1024, 100 * 1024 * 1024, 5 * 1024 * 1024);
    let search_limit = clamp_number(args.values.get("max-search-results"), 1, 500, 40) as usize;

    let session_id_arg = normalize_id(args.values.get("session-id").or_else(|| args.values.get("session")));
    let run_id_arg = normalize_id(args.values.get("run-id").or_else(|| args.values.get("run")));

    let session_id = if !session_id_arg.is_empty() {
        session_id_arg
    } else if let Ok(val) = env::var("MODEL_CLI_SESSION_ID") {
        val
    } else {
        generate_id("session")
    };
    let run_id = if !run_id_arg.is_empty() {
        run_id_arg
    } else {
        env::var("MODEL_CLI_RUN_ID").unwrap_or_default()
    };

    env::set_var("MODEL_CLI_SESSION_ID", &session_id);
    if !run_id.is_empty() {
        env::set_var("MODEL_CLI_RUN_ID", &run_id);
    }

    ensure_dir(&root).expect("failed to create root directory");
    let state_dir = resolve_state_dir(&server_name);
    ensure_dir(&state_dir).expect("failed to create state directory");
    let db_path = args
        .values
        .get("db")
        .cloned()
        .or_else(|| env::var("MODEL_CLI_FILE_CHANGES_DB").ok())
        .unwrap_or_else(|| state_dir.join(format!("{server_name}.db.sqlite")).to_string_lossy().to_string());

    let change_log = ChangeLogStore::new(&db_path).expect("failed to open change log db");
    let change_log = Rc::new(RefCell::new(change_log));
    let fs_ops = FsOps::new(
        root.clone(),
        allow_writes,
        max_file_bytes,
        max_write_bytes,
        search_limit,
    );

    let admin_port = args
        .values
        .get("admin-port")
        .and_then(|v| v.parse::<u16>().ok());
    if let Some(port) = admin_port {
        let admin_host = args
            .values
            .get("admin-host")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let admin_ui_root = resolve_admin_ui_root(args.values.get("admin-ui-root"));
        let options = AdminServerOptions {
            server_name: server_name.clone(),
            root: root.clone(),
            db_path: db_path.clone(),
            allow_writes,
            max_file_bytes,
            max_write_bytes,
            search_limit,
            session_id: session_id.clone(),
            run_id: run_id.clone(),
            host: admin_host,
            port,
            admin_ui_root,
        };
        std::thread::spawn(move || {
            if let Err(err) = run_admin_server(options) {
                eprintln!("[code-admin] {err}");
            }
        });
    }

    let mut server = McpServer::new(server_name.clone(), "0.1.0");
    let workspace_note = format!(
        "Workspace root: {}. Paths must stay inside this directory.",
        root.display()
    );

    {
        let fs_ops = fs_ops.clone();
        server.register_tool(
            "read_file_raw",
            &format!("Return UTF-8 file content without line numbers.\n{workspace_note}"),
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            Box::new(move |args| {
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

    {
        let fs_ops = fs_ops.clone();
        server.register_tool(
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
                "required": ["path", "start_line", "end_line"]
            }),
            Box::new(move |args| {
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

    {
        let fs_ops = fs_ops.clone();
        server.register_tool(
            "list_dir",
            &format!("List directory entries.\n{workspace_note}"),
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_entries": { "type": "integer", "minimum": 1, "maximum": 1000 }
                }
            }),
            Box::new(move |args| {
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

    {
        let fs_ops = fs_ops.clone();
        server.register_tool(
            "search_text",
            &format!("Search text recursively under a directory.\n{workspace_note}"),
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "minLength": 1 },
                    "path": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "required": ["pattern"]
            }),
            Box::new(move |args| {
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
                Ok(text_result(json!({ "count": results.len(), "results": results })))
            }),
        );
    }

    {
        let fs_ops = fs_ops.clone();
        let change_log = change_log.clone();
        let session_id = session_id.clone();
        let run_id = run_id.clone();
        let max_file_bytes = max_file_bytes;
        server.register_tool(
            "write_file",
            &format!(
                "Write file content (overwrite).\nMax write bytes: {}.\n{}.\n{workspace_note}",
                format_bytes(max_write_bytes),
                if allow_writes { "Writes enabled" } else { "Writes disabled" }
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            Box::new(move |args| {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("path is required".to_string())?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("content is required".to_string())?;
                let target = fs_ops.resolve_path(path)?;
                let before_snapshot =
                    read_text_for_diff(&target, max_file_bytes).unwrap_or_else(DiffInput::omitted);
                let result = fs_ops.write_file(path, content)?;
                let after_snapshot = DiffInput::text(content.to_string());
                let diff = build_diff(before_snapshot, after_snapshot);
                let record = change_log
                    .borrow()
                    .log_change(
                        &result.path,
                        "write",
                        result.bytes,
                        &result.sha256,
                        &session_id,
                        &run_id,
                        diff,
                    )?;
                Ok(text_result(json!({ "result": result, "change": record })))
            }),
        );
    }

    {
        let fs_ops = fs_ops.clone();
        let change_log = change_log.clone();
        let session_id = session_id.clone();
        let run_id = run_id.clone();
        let max_file_bytes = max_file_bytes;
        server.register_tool(
            "append_file",
            &format!(
                "Append content to file.\nMax write bytes: {}.\n{}.\n{workspace_note}",
                format_bytes(max_write_bytes),
                if allow_writes { "Writes enabled" } else { "Writes disabled" }
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            Box::new(move |args| {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("path is required".to_string())?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("content is required".to_string())?;
                let target = fs_ops.resolve_path(path)?;
                let before_snapshot =
                    read_text_for_diff(&target, max_file_bytes).unwrap_or_else(DiffInput::omitted);
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
                    .borrow()
                    .log_change(
                        &result.path,
                        "append",
                        result.bytes,
                        &result.sha256,
                        &session_id,
                        &run_id,
                        diff,
                    )?;
                Ok(text_result(json!({ "result": result, "change": record })))
            }),
        );
    }

    {
        let fs_ops = fs_ops.clone();
        let change_log = change_log.clone();
        let session_id = session_id.clone();
        let run_id = run_id.clone();
        let max_file_bytes = max_file_bytes;
        server.register_tool(
            "delete_path",
            &format!(
                "Delete a file or directory.\n{}.\n{workspace_note}",
                if allow_writes { "Writes enabled" } else { "Writes disabled" }
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            Box::new(move |args| {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("path is required".to_string())?;
                let target = fs_ops.resolve_path(path)?;
                let before_snapshot =
                    read_text_for_diff(&target, max_file_bytes).unwrap_or_else(DiffInput::omitted);
                let after_snapshot = if let Some(reason) = before_snapshot.reason.clone() {
                    DiffInput::omitted(reason)
                } else {
                    DiffInput::text(String::new())
                };
                let deleted_path = fs_ops.delete_path(path)?;
                let diff = build_diff(before_snapshot, after_snapshot);
                let record = change_log
                    .borrow()
                    .log_change(&deleted_path, "delete", 0, "", &session_id, &run_id, diff)?;
                Ok(text_result(json!({ "result": { "path": deleted_path }, "change": record })))
            }),
        );
    }

    {
        let change_log = change_log.clone();
        let fs_ops = fs_ops.clone();
        let session_id = session_id.clone();
        let run_id = run_id.clone();
        server.register_tool(
            "apply_patch",
            &format!(
                "Apply a patch to one or more files.\nPatch format uses *** Begin Patch / *** Update File / *** Add File / *** Delete File / *** End Patch.\n{}.\n{workspace_note}",
                if allow_writes { "Writes enabled" } else { "Writes disabled" }
            ),
            json!({
                "type": "object",
                "properties": {
                    "patch": { "type": "string", "minLength": 1 }
                },
                "required": ["patch"]
            }),
            Box::new(move |args| {
                let patch_text = args
                    .get("patch")
                    .and_then(|v| v.as_str())
                    .ok_or("patch is required".to_string())?;
                let patch_diffs: HashMap<String, String> = extract_patch_diffs(patch_text);
                let result = apply_patch(&root, patch_text, allow_writes)?;
                let mut hashes = Vec::new();

                for path in result.updated.iter().chain(result.added.iter()) {
                    let full_path = fs_ops.resolve_path(path)?;
                    let content = std::fs::read(&full_path).map_err(|err| err.to_string())?;
                    let hash = sha256_bytes(&content);
                    let diff = patch_diffs.get(path).cloned();
                    change_log.borrow().log_change(
                        path,
                        "write",
                        content.len() as i64,
                        &hash,
                        &session_id,
                        &run_id,
                        diff,
                    )?;
                    hashes.push(json!({ "path": path, "sha256": hash }));
                }

                for path in &result.deleted {
                    let diff = patch_diffs.get(path).cloned();
                    change_log
                        .borrow()
                        .log_change(path, "delete", 0, "", &session_id, &run_id, diff)?;
                }

                Ok(text_result(json!({ "result": result, "files": hashes })))
            }),
        );
    }

    if let Err(err) = server.run_stdio() {
        eprintln!("[{server_name}] Server crashed: {err}");
        std::process::exit(1);
    }
}

fn text_result(data: serde_json::Value) -> serde_json::Value {
    let text = if data.is_string() {
        data.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&data).unwrap_or_else(|_| "{}".to_string())
    };
    json!({
        "content": [
            { "type": "text", "text": text }
        ]
    })
}

fn print_help() {
    println!(
        "Usage: code-maintainer-mcp-server-rs [--root <path>] [--name <id>] [--write] [--mode <text>] [--session-id <id>] [--run-id <id>]\n\nOptions:\n  --root <path>            Workspace root (default cwd)\n  --name <id>              MCP server name (default code_maintainer)\n  --write                  Allow write operations\n  --mode <text>            If contains 'write' then enables writes\n  --max-bytes <n>          Max file bytes to read (default 256 KB)\n  --max-write-bytes <n>    Max write bytes (default 5 MB)\n  --max-search-results <n> Max search results (default 40)\n  --db <path>              SQLite path for change log\n  --session-id <id>        Session ID override\n  --run-id <id>            Run ID override\n  --admin-port <p>         Start admin HTTP server on port p\n  --admin-host <h>         Admin HTTP bind host (default 127.0.0.1)\n  --admin-ui-root <path>   Admin UI dist directory\n  --help                   Show help"
    );
}

fn resolve_admin_ui_root(value: Option<&String>) -> Option<PathBuf> {
    if let Some(path) = value {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(PathBuf::from(trimmed));
    }
    let candidate = PathBuf::from("admin-ui").join("dist");
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}
