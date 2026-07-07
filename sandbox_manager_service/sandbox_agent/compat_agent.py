#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

import json
import os
import shutil
import subprocess
import time
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

VERSION = "0.3.0-mcp"
HOST = os.environ.get("CHATOS_AGENT_HOST", "0.0.0.0")
PORT = int(os.environ.get("CHATOS_AGENT_PORT", "49888"))
WORKSPACE = Path(os.environ.get("CHATOS_WORKSPACE", "/workspace")).resolve()
MAX_BODY_BYTES = int(os.environ.get("CHATOS_AGENT_MAX_BODY_BYTES", str(16 * 1024 * 1024)))
DEFAULT_TIMEOUT_SECONDS = int(os.environ.get("CHATOS_AGENT_COMMAND_TIMEOUT_SECONDS", "120"))
DEFAULT_MAX_OUTPUT_BYTES = int(os.environ.get("CHATOS_AGENT_MAX_OUTPUT_BYTES", str(2 * 1024 * 1024)))


def tool_schema(name, description, properties=None, required=None):
    return {
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties or {},
            "required": required or [],
            "additionalProperties": False,
        },
    }


TOOLS = [
    tool_schema(
        "read_file_raw",
        "Return UTF-8 file content from the sandbox workspace.",
        {
            "path": {"type": "string"},
            "with_line_numbers": {"type": "boolean", "default": True},
        },
        ["path"],
    ),
    tool_schema(
        "read_file_range",
        "Return UTF-8 file content from start_line to end_line.",
        {
            "path": {"type": "string"},
            "start_line": {"type": "integer", "minimum": 1},
            "end_line": {"type": "integer", "minimum": 1},
            "with_line_numbers": {"type": "boolean", "default": False},
        },
        ["path", "start_line", "end_line"],
    ),
    tool_schema(
        "list_dir",
        "List directory entries under the sandbox workspace.",
        {
            "path": {"type": "string"},
            "max_entries": {"type": "integer", "minimum": 1, "maximum": 1000},
        },
    ),
    tool_schema(
        "search_text",
        "Search text recursively under a sandbox workspace directory.",
        {
            "pattern": {"type": "string", "minLength": 1},
            "path": {"type": "string"},
            "max_results": {"type": "integer", "minimum": 1, "maximum": 500},
        },
        ["pattern"],
    ),
    tool_schema(
        "write_file",
        "Write file content in the sandbox workspace.",
        {"path": {"type": "string"}, "content": {"type": "string"}},
        ["path", "content"],
    ),
    tool_schema(
        "append_file",
        "Append file content in the sandbox workspace.",
        {"path": {"type": "string"}, "content": {"type": "string"}},
        ["path", "content"],
    ),
    tool_schema(
        "delete_path",
        "Delete a file or directory under the sandbox workspace.",
        {"path": {"type": "string"}},
        ["path"],
    ),
    tool_schema(
        "execute_command",
        "Execute a shell command in the sandbox workspace.",
        {
            "path": {"type": "string"},
            "common": {"type": "string"},
            "command": {"type": "string"},
            "background": {"type": "boolean", "default": False},
        },
    ),
]

def ensure_workspace():
    WORKSPACE.mkdir(parents=True, exist_ok=True)


def json_bytes(payload):
    return json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def resolve_workspace_path(raw_path):
    relative = raw_path or "."
    if "\x00" in relative:
        raise ValueError("path contains null byte")
    candidate = (WORKSPACE / relative).resolve()
    if candidate != WORKSPACE and WORKSPACE not in candidate.parents:
        raise ValueError("path escapes workspace")
    return candidate


def relpath(path):
    if path == WORKSPACE:
        return "."
    return str(path.relative_to(WORKSPACE))


def truncate_bytes(value, limit):
    if len(value) <= limit:
        return value, False
    return value[:limit], True


def decode_limited(value, limit):
    truncated_value, truncated = truncate_bytes(value, limit)
    return truncated_value.decode("utf-8", errors="replace"), truncated


def mcp_result(payload):
    text = payload if isinstance(payload, str) else json.dumps(payload, ensure_ascii=False, indent=2)
    out = {"content": [{"type": "text", "text": text}]}
    if not isinstance(payload, str):
        out["_structured_result"] = payload
    return out


def jsonrpc_error(request_id, code, message):
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": str(message)},
    }


def read_file_raw(args):
    path = resolve_workspace_path(args.get("path"))
    max_bytes = int(args.get("max_bytes") or DEFAULT_MAX_OUTPUT_BYTES)
    with_line_numbers = bool(args.get("with_line_numbers", True))
    data, truncated = truncate_bytes(path.read_bytes(), max_bytes)
    content = data.decode("utf-8", errors="replace")
    lines = content.splitlines()
    result = {
        "path": relpath(path),
        "size_bytes": path.stat().st_size,
        "content": content,
        "line_count": len(lines),
        "truncated": truncated,
    }
    if with_line_numbers:
        result["numbered_lines"] = [
            {"line": index + 1, "text": line} for index, line in enumerate(lines)
        ]
    return result


def read_file_range(args):
    path = resolve_workspace_path(args.get("path"))
    start = int(args.get("start_line") or 1)
    end = int(args.get("end_line") or start)
    with_line_numbers = bool(args.get("with_line_numbers", False))
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    start_index = max(start, 1) - 1
    end_index = max(end, start)
    selected = lines[start_index:end_index]
    if with_line_numbers:
        content = "\n".join(
            f"{line_no}: {line}" for line_no, line in enumerate(selected, start=start_index + 1)
        )
    else:
        content = "\n".join(selected)
    if selected:
        content += "\n"
    return {
        "path": relpath(path),
        "start_line": start,
        "end_line": min(end, len(lines)),
        "total_lines": len(lines),
        "content": content,
    }


def list_dir(args):
    path = resolve_workspace_path(args.get("path"))
    max_entries = int(args.get("max_entries") or 200)
    entries = []
    for child in sorted(path.iterdir(), key=lambda item: item.name.lower())[:max_entries]:
        stat = child.lstat()
        entries.append(
            {
                "name": child.name,
                "path": relpath(child),
                "kind": "dir" if child.is_dir() else "file",
                "size": stat.st_size,
                "modified_at": int(stat.st_mtime),
            }
        )
    return {"path": relpath(path), "entries": entries}


def search_text(args):
    pattern = args.get("pattern")
    if not pattern:
        raise ValueError("pattern is required")
    root = resolve_workspace_path(args.get("path"))
    max_results = int(args.get("max_results") or 100)
    results = []
    for path in root.rglob("*"):
        if len(results) >= max_results:
            break
        if not path.is_file():
            continue
        try:
            for line_no, line in enumerate(path.read_text(encoding="utf-8", errors="ignore").splitlines(), 1):
                if pattern in line:
                    results.append({"path": relpath(path), "line": line_no, "text": line})
                    if len(results) >= max_results:
                        break
        except OSError:
            continue
    return {"count": len(results), "results": results}


def write_file(args, append=False):
    path = resolve_workspace_path(args.get("path"))
    content = args.get("content") or ""
    path.parent.mkdir(parents=True, exist_ok=True)
    mode = "a" if append else "w"
    with path.open(mode, encoding="utf-8") as handle:
        handle.write(content)
    return {"path": relpath(path), "bytes": len(content.encode("utf-8")), "append": append}


def delete_path(args):
    path = resolve_workspace_path(args.get("path"))
    if path == WORKSPACE:
        raise ValueError("cannot delete workspace root")
    if path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink()
    return {"path": relpath(path), "deleted": True}


def execute_command(args):
    command = args.get("common") or args.get("command")
    if not command:
        raise ValueError("common or command is required")
    cwd = resolve_workspace_path(args.get("path") or args.get("cwd") or ".")
    cwd.mkdir(parents=True, exist_ok=True)
    timeout = int(args.get("timeout_seconds") or DEFAULT_TIMEOUT_SECONDS)
    max_output = int(args.get("max_output_bytes") or DEFAULT_MAX_OUTPUT_BYTES)
    started_at = time.time()
    try:
        completed = subprocess.run(
            str(command),
            cwd=str(cwd),
            shell=True,
            executable="/bin/bash" if Path("/bin/bash").exists() else "/bin/sh",
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        timed_out = False
        exit_code = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
    except subprocess.TimeoutExpired as error:
        timed_out = True
        exit_code = 124
        stdout = error.stdout or b""
        stderr = (error.stderr or b"") + f"\ncommand timed out after {timeout}s".encode("utf-8")
    stdout_text, stdout_truncated = decode_limited(stdout, max_output)
    stderr_text, stderr_truncated = decode_limited(stderr, max_output)
    output_text, output_truncated = decode_limited(stdout + stderr, max_output)
    return {
        "project_root": str(WORKSPACE),
        "path": str(cwd),
        "common": command,
        "background": False,
        "busy": False,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "duration_ms": int((time.time() - started_at) * 1000),
        "stdout": stdout_text,
        "stderr": stderr_text,
        "output": output_text,
        "output_chars": len(output_text),
        "truncated": output_truncated or stdout_truncated or stderr_truncated,
        "finished_by": "timeout" if timed_out else "exit",
    }


def call_tool(name, args):
    args = args or {}
    if name == "read_file_raw":
        return read_file_raw(args)
    if name == "read_file_range":
        return read_file_range(args)
    if name == "list_dir":
        return list_dir(args)
    if name == "search_text":
        return search_text(args)
    if name == "write_file":
        return write_file(args, append=False)
    if name == "append_file":
        return write_file(args, append=True)
    if name == "delete_path":
        return delete_path(args)
    if name == "execute_command":
        return execute_command(args)
    raise ValueError(f"unknown tool: {name}")


class Handler(BaseHTTPRequestHandler):
    server_version = f"chatos-sandbox-agent/{VERSION}"

    def log_message(self, fmt, *args):
        print(f"{self.address_string()} - {fmt % args}", flush=True)

    def send_json(self, status, payload):
        body = json_bytes(payload)
        self.send_response(status)
        self.send_header("content-type", "application/json; charset=utf-8")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def read_json(self):
        length = int(self.headers.get("content-length") or "0")
        if length > MAX_BODY_BYTES:
            raise ValueError(f"request body too large: {length} bytes")
        raw = self.rfile.read(length) if length else b"{}"
        return json.loads(raw.decode("utf-8"))

    def do_GET(self):
        try:
            if self.path == "/health":
                ensure_workspace()
                probe = WORKSPACE / ".chatos_agent_health"
                probe.write_text("ok", encoding="utf-8")
                probe.unlink(missing_ok=True)
                self.send_json(
                    HTTPStatus.OK,
                    {
                        "ok": True,
                        "service": "chatos-sandbox-agent",
                        "agent_version": VERSION,
                        "workspace": str(WORKSPACE),
                        "workspace_writable": True,
                        "mcp_endpoint": "/mcp",
                        "tools_count": len(TOOLS),
                    },
                )
                return
            self.send_json(HTTPStatus.NOT_FOUND, {"ok": False, "error": "route not found"})
        except Exception as error:
            self.send_json(HTTPStatus.INTERNAL_SERVER_ERROR, {"ok": False, "error": str(error)})

    def do_POST(self):
        payload = None
        try:
            payload = self.read_json()
            if self.path == "/mcp":
                request_id = payload.get("id")
                method = payload.get("method")
                params = payload.get("params") or {}
                if method == "initialize":
                    self.send_json(
                        HTTPStatus.OK,
                        {
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "result": {
                                "protocolVersion": "2024-11-05",
                                "capabilities": {"tools": {}},
                                "serverInfo": {
                                    "name": "sandbox_agent",
                                    "version": VERSION,
                                },
                            },
                        },
                    )
                    return
                if method in ("notifications/initialized", "ping"):
                    self.send_json(
                        HTTPStatus.OK,
                        {"jsonrpc": "2.0", "id": request_id, "result": {}},
                    )
                    return
                if method == "tools/list":
                    self.send_json(HTTPStatus.OK, {"jsonrpc": "2.0", "id": request_id, "result": {"tools": TOOLS}})
                    return
                if method == "tools/call":
                    name = params.get("name")
                    args = params.get("arguments") or {}
                    try:
                        result = call_tool(name, args)
                    except Exception as error:
                        self.send_json(HTTPStatus.OK, jsonrpc_error(request_id, -32000, error))
                        return
                    self.send_json(
                        HTTPStatus.OK,
                        {
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "result": mcp_result(result),
                        },
                    )
                    return
                self.send_json(HTTPStatus.OK, jsonrpc_error(request_id, -32601, f"method not found: {method}"))
                return
            self.send_json(HTTPStatus.NOT_FOUND, {"ok": False, "error": "route not found"})
        except ValueError as error:
            if self.path == "/mcp":
                request_id = payload.get("id") if isinstance(payload, dict) else None
                self.send_json(HTTPStatus.OK, jsonrpc_error(request_id, -32000, error))
            else:
                self.send_json(HTTPStatus.BAD_REQUEST, {"ok": False, "error": str(error)})
        except Exception as error:
            if self.path == "/mcp":
                request_id = payload.get("id") if isinstance(payload, dict) else None
                self.send_json(HTTPStatus.OK, jsonrpc_error(request_id, -32000, error))
            else:
                self.send_json(HTTPStatus.INTERNAL_SERVER_ERROR, {"ok": False, "error": str(error)})


def main():
    ensure_workspace()
    print(
        json.dumps(
            {
                "service": "chatos-sandbox-agent",
                "version": VERSION,
                "host": HOST,
                "port": PORT,
                "workspace": str(WORKSPACE),
            }
        ),
        flush=True,
    )
    ThreadingHTTPServer((HOST, PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
