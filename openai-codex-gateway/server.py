#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import binascii
import json
import os
import shutil
import sqlite3
import sys
import threading
import time
import uuid
import warnings
from dataclasses import dataclass
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Callable
from urllib.parse import urlparse

REPO_ROOT = Path(__file__).resolve().parents[1]
SDK_IMPORT_SOURCE = "unknown"

warnings.filterwarnings(
    "ignore",
    message=r'Field "model_.*" has conflict with protected namespace "model_".*',
    category=UserWarning,
)


def resolve_bundled_sdk_candidates() -> list[Path]:
    here = Path(__file__).resolve().parent
    return [
        here / "vendor",
    ]


def resolve_local_sdk_candidates() -> list[Path]:
    return [
        REPO_ROOT / "sdk" / "python" / "src",
        REPO_ROOT / "chat_app_server_rs" / "docs" / "codex" / "sdk" / "python" / "src",
    ]


def load_sdk_imports() -> tuple[Any, ...]:
    global SDK_IMPORT_SOURCE
    mode = os.environ.get("CODEX_GATEWAY_SDK_MODE", "auto").strip().lower()
    if mode not in {"auto", "installed", "local"}:
        mode = "auto"

    errors: list[str] = []

    def try_import() -> tuple[Any, ...]:
        from codex_app_server.client import AppServerClient, AppServerConfig
        from codex_app_server.generated.v2_all import (
            AgentMessageDeltaNotification,
            AgentMessageThreadItem,
            ItemCompletedNotification,
            ModelListResponse,
            ReasoningSummaryTextDeltaNotification,
            ReasoningTextDeltaNotification,
            ReasoningThreadItem,
            ThreadTokenUsageUpdatedNotification,
            TurnCompletedNotification,
        )

        return (
            AppServerClient,
            AppServerConfig,
            AgentMessageDeltaNotification,
            AgentMessageThreadItem,
            ItemCompletedNotification,
            ModelListResponse,
            ReasoningSummaryTextDeltaNotification,
            ReasoningTextDeltaNotification,
            ReasoningThreadItem,
            ThreadTokenUsageUpdatedNotification,
            TurnCompletedNotification,
        )

    if mode in {"auto", "local"}:
        for candidate in resolve_bundled_sdk_candidates():
            if not (candidate / "codex_app_server" / "client.py").exists():
                continue
            if str(candidate) not in sys.path:
                sys.path.insert(0, str(candidate))
            try:
                imports = try_import()
                SDK_IMPORT_SOURCE = f"bundled:{candidate}"
                return imports
            except ModuleNotFoundError as exc:
                errors.append(f"bundled sdk import failed from {candidate}: {exc}")

    if mode in {"auto", "installed"}:
        try:
            imports = try_import()
            SDK_IMPORT_SOURCE = "installed"
            return imports
        except ModuleNotFoundError as exc:
            errors.append(f"installed package import failed: {exc}")

    if mode in {"auto", "local"}:
        for candidate in resolve_local_sdk_candidates():
            if not (candidate / "codex_app_server" / "client.py").exists():
                continue
            if str(candidate) not in sys.path:
                sys.path.insert(0, str(candidate))
            try:
                imports = try_import()
                SDK_IMPORT_SOURCE = f"local:{candidate}"
                return imports
            except ModuleNotFoundError as exc:
                errors.append(f"local sdk import failed from {candidate}: {exc}")

    local_install_hints = []
    for candidate in resolve_local_sdk_candidates():
        if (candidate / "codex_app_server" / "client.py").exists():
            sdk_project_dir = candidate.parent if candidate.name == "src" else candidate
            local_install_hints.append(
                f"  cd {sdk_project_dir}\n  python -m pip install -e ."
            )

    hint_lines = [
        "Missing Codex Python SDK.",
        "",
        "Preferred: use the SDK bundled inside openai-codex-gateway/vendor.",
        "Alternative: install the official SDK into your current Python environment.",
        "Fallback: install one of the bundled local SDK copies:",
    ]
    if local_install_hints:
        hint_lines.extend(local_install_hints)
    else:
        hint_lines.append("  (no local sdk/python candidate found)")
    if errors:
        hint_lines.extend(["", "Import attempts:"])
        hint_lines.extend(f"  - {err}" for err in errors)
    raise SystemExit("\n".join(hint_lines))


(
    AppServerClient,
    AppServerConfig,
    AgentMessageDeltaNotification,
    AgentMessageThreadItem,
    ItemCompletedNotification,
    ModelListResponse,
    ReasoningSummaryTextDeltaNotification,
    ReasoningTextDeltaNotification,
    ReasoningThreadItem,
    ThreadTokenUsageUpdatedNotification,
    TurnCompletedNotification,
) = load_sdk_imports()


@dataclass
class GatewayConfig:
    host: str
    port: int
    codex_bin: str | None
    cwd: str | None
    sandbox: str
    approval_policy: str
    state_db_path: str


@dataclass
class TurnResult:
    thread_id: str
    turn_id: str
    output_text: str
    reasoning_text: str
    status: str
    usage: dict[str, Any] | None
    error: dict[str, Any] | None
    tool_calls: list["ToolCallRecord"]


@dataclass
class ToolCallRecord:
    call_id: str
    name: str
    arguments: Any


class ResponseThreadStore:
    def __init__(self, db_path: str) -> None:
        self._lock = threading.Lock()
        self._db_path = Path(db_path).expanduser()
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn = sqlite3.connect(self._db_path, check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.execute("PRAGMA synchronous=NORMAL")
        self._conn.execute(
            """
            CREATE TABLE IF NOT EXISTS response_threads (
                response_id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )
            """
        )
        self._conn.commit()
        count_row = self._conn.execute(
            "SELECT COUNT(*) FROM response_threads"
        ).fetchone()
        state_log(
            "db.ready",
            f"path={self._db_path}",
            f"entries={count_row[0] if count_row else 0}",
        )

    def put(self, response_id: str, thread_id: str) -> None:
        with self._lock:
            self._conn.execute(
                """
                INSERT INTO response_threads (response_id, thread_id, updated_at)
                VALUES (?, ?, ?)
                ON CONFLICT(response_id) DO UPDATE SET
                    thread_id = excluded.thread_id,
                    updated_at = excluded.updated_at
                """,
                (response_id, thread_id, int(time.time())),
            )
            self._conn.commit()
        state_log(
            "map.put",
            f"response_id={response_id}",
            f"thread_id={thread_id}",
        )

    def get_thread(self, response_id: str) -> str | None:
        with self._lock:
            row = self._conn.execute(
                "SELECT thread_id FROM response_threads WHERE response_id = ?",
                (response_id,),
            ).fetchone()
        thread_id = row[0] if row else None
        state_log(
            "map.lookup",
            f"response_id={response_id}",
            f"hit={'yes' if thread_id else 'no'}",
        )
        return thread_id

    def close(self) -> None:
        with self._lock:
            self._conn.close()


def _deny_approval(_method: str, _params: dict[str, Any] | None) -> dict[str, Any]:
    # Do not auto-approve command/file-change requests for public HTTP callers.
    if _method in {"item/commandExecution/requestApproval", "item/fileChange/requestApproval"}:
        return {"decision": "decline"}
    return {}


def gateway_developer_instructions() -> str:
    return (
        "Gateway policy: only use caller-provided tools for this request "
        "(dynamic function tools and configured MCP servers). "
        "Do not use Codex built-in environment tools such as shell/command execution, "
        "file editing/apply_patch, request_permissions, or web_search. "
        "If the required caller-provided tool is unavailable, explain that limitation "
        "and ask the user to provide/enable the tool."
    )


def debug_enabled() -> bool:
    value = os.environ.get("GATEWAY_DEBUG", "")
    return value.strip().lower() in {"1", "true", "yes", "on"}


def debug_log(*parts: Any) -> None:
    if not debug_enabled():
        return
    message = " ".join(str(part) for part in parts)
    print(f"[gateway] {message}", file=sys.stderr, flush=True)


def reasoning_log(*parts: Any) -> None:
    message = " ".join(str(part) for part in parts)
    print(f"[gateway.reasoning] {message}", file=sys.stderr, flush=True)


def state_log(*parts: Any) -> None:
    message = " ".join(str(part) for part in parts)
    print(f"[gateway.state] {message}", file=sys.stderr, flush=True)


class CodexBridge:
    def __init__(self, cfg: GatewayConfig, store: ResponseThreadStore) -> None:
        self._cfg = cfg
        self._store = store

    def _app_server_config(self, api_key: str | None) -> AppServerConfig:
        env: dict[str, str] = {}
        if api_key:
            env["CODEX_API_KEY"] = api_key

        return AppServerConfig(
            codex_bin=self._cfg.codex_bin,
            cwd=self._cfg.cwd,
            env=env or None,
        )

    def _run_turn(
        self,
        *,
        input_items: list[dict[str, Any]],
        model: str | None,
        reasoning_effort: str | None,
        reasoning_summary: str | None,
        previous_response_id: str | None,
        api_key: str | None,
        request_cwd: str | None,
        request_config_overrides: dict[str, Any] | None,
        function_tools: list[dict[str, Any]],
        provided_tool_outputs: dict[str, list[dict[str, Any]]],
        on_delta: Callable[[str], None] | None = None,
        on_reasoning_delta: Callable[[str], None] | None = None,
    ) -> TurnResult:
        thread_id: str
        turn_id: str
        output_text = ""
        reasoning_text = ""
        reasoning_tokens = 0
        reasoning_event_count = 0
        usage: dict[str, Any] | None = None
        status = "failed"
        error: dict[str, Any] | None = None
        tool_calls: list[ToolCallRecord] = []
        seen_call_ids: set[str] = set()
        missing_tool_output_detected = False
        interrupt_sent = False

        debug_log(
            "run_turn.start",
            f"model={model or 'default'}",
            f"reasoning_effort={reasoning_effort or 'default'}",
            f"reasoning_summary={reasoning_summary or 'default'}",
            f"prev={'yes' if previous_response_id else 'no'}",
            f"cwd={request_cwd or self._cfg.cwd or 'default'}",
            f"input_items={len(input_items)}",
            f"function_tools={len(function_tools)}",
            f"provided_outputs={len(provided_tool_outputs)}",
        )
        if function_tools:
            names = [str(tool.get("name", "unknown")) for tool in function_tools[:16]]
            debug_log("run_turn.tools", ", ".join(names))

        def handle_server_request(method: str, params: dict[str, Any] | None) -> dict[str, Any]:
            nonlocal missing_tool_output_detected
            if method in {"item/commandExecution/requestApproval", "item/fileChange/requestApproval"}:
                state_log("run_turn.builtin_request_declined", f"method={method}")
                return {"decision": "decline"}

            if method != "item/tool/call":
                return {}

            payload = params or {}
            call_id_raw = payload.get("callId")
            tool_name_raw = payload.get("tool")
            arguments = payload.get("arguments")

            call_id = call_id_raw if isinstance(call_id_raw, str) and call_id_raw else make_id("call")
            tool_name = tool_name_raw if isinstance(tool_name_raw, str) and tool_name_raw else "unknown_tool"

            if call_id not in seen_call_ids:
                seen_call_ids.add(call_id)
                tool_calls.append(
                    ToolCallRecord(
                        call_id=call_id,
                        name=tool_name,
                        arguments=arguments,
                    )
                )

            content_items = provided_tool_outputs.get(call_id)
            debug_log(
                "run_turn.tool_call",
                f"name={tool_name}",
                f"call_id={call_id}",
                f"has_output={'yes' if content_items is not None else 'no'}",
            )
            if content_items is not None:
                return {
                    "contentItems": content_items,
                    "success": True,
                }

            missing_tool_output_detected = True
            return {
                "contentItems": [
                    {
                        "type": "inputText",
                        "text": f"TOOL_OUTPUT_DEFERRED call_id={call_id}",
                    }
                ],
                "success": True,
            }

        config = self._app_server_config(api_key)
        client = AppServerClient(config=config, approval_handler=handle_server_request)

        try:
            client.start()
            client.initialize()

            thread_id = ""
            if previous_response_id:
                resumed_thread = self._store.get_thread(previous_response_id)
                if not resumed_thread:
                    raise ValueError(f"unknown previous_response_id: {previous_response_id}")
                resume_params: dict[str, Any] = {
                    "approvalPolicy": self._cfg.approval_policy,
                    "sandbox": self._cfg.sandbox,
                    "developerInstructions": gateway_developer_instructions(),
                    **({"model": model} if model else {}),
                    **({"cwd": request_cwd} if request_cwd else {}),
                    **({"config": request_config_overrides} if request_config_overrides else {}),
                }
                if function_tools:
                    resume_params["dynamicTools"] = function_tools
                resumed = client.thread_resume(resumed_thread, resume_params)
                thread_id = resumed.thread.id
            else:
                start_params: dict[str, Any] = {
                    "approvalPolicy": self._cfg.approval_policy,
                    "sandbox": self._cfg.sandbox,
                    "developerInstructions": gateway_developer_instructions(),
                    **({"model": model} if model else {}),
                    **({"cwd": request_cwd} if request_cwd else {}),
                    **({"config": request_config_overrides} if request_config_overrides else {}),
                }
                if function_tools:
                    start_params["dynamicTools"] = function_tools
                started = client.thread_start(start_params)
                thread_id = started.thread.id

            turn_started = client.turn_start(
                thread_id,
                input_items,
                params={
                    **({"cwd": request_cwd} if request_cwd else {}),
                    **({"model": model} if model else {}),
                    **({"effort": reasoning_effort} if reasoning_effort else {}),
                    **({"summary": reasoning_summary} if reasoning_summary else {}),
                },
            )
            turn_id = turn_started.turn.id
            reasoning_log(
                "turn.started",
                f"thread_id={thread_id}",
                f"turn_id={turn_id}",
                f"effort={reasoning_effort or 'none'}",
                f"summary={reasoning_summary or 'none'}",
                f"prev_response={'yes' if previous_response_id else 'no'}",
            )

            while True:
                event = client.next_notification()
                event_method = getattr(event, "method", "unknown")
                payload = event.payload

                if missing_tool_output_detected and not interrupt_sent:
                    try:
                        client.turn_interrupt(thread_id, turn_id)
                    except Exception:
                        pass
                    interrupt_sent = True

                if isinstance(payload, AgentMessageDeltaNotification) and payload.turn_id == turn_id:
                    output_text += payload.delta
                    if on_delta:
                        on_delta(payload.delta)
                    continue

                if isinstance(payload, ReasoningTextDeltaNotification) and payload.turn_id == turn_id:
                    reasoning_event_count += 1
                    reasoning_log(
                        "sdk.event",
                        f"method={event_method}",
                        "type=reasoning_text_delta",
                        f"turn_id={payload.turn_id}",
                        f"chars={len(payload.delta)}",
                    )
                    reasoning_text += payload.delta
                    if on_reasoning_delta:
                        on_reasoning_delta(payload.delta)
                    continue

                if (
                    isinstance(payload, ReasoningSummaryTextDeltaNotification)
                    and payload.turn_id == turn_id
                ):
                    reasoning_event_count += 1
                    reasoning_log(
                        "sdk.event",
                        f"method={event_method}",
                        "type=reasoning_summary_delta",
                        f"turn_id={payload.turn_id}",
                        f"chars={len(payload.delta)}",
                    )
                    reasoning_text += payload.delta
                    if on_reasoning_delta:
                        on_reasoning_delta(payload.delta)
                    continue

                if isinstance(payload, ItemCompletedNotification) and payload.turn_id == turn_id:
                    item = payload.item.root
                    if isinstance(item, AgentMessageThreadItem) and item.text:
                        output_text = item.text
                    if isinstance(item, ReasoningThreadItem):
                        summary_text = "".join(item.summary or [])
                        content_text = "".join(item.content or [])
                        fallback_text = (summary_text + content_text).strip()
                        reasoning_log(
                            "sdk.event",
                            f"method={event_method}",
                            "type=reasoning_item_completed",
                            f"summary_chars={len(summary_text)}",
                            f"content_chars={len(content_text)}",
                            f"used_fallback={'yes' if not reasoning_text and bool(fallback_text) else 'no'}",
                        )
                        if not reasoning_text:
                            reasoning_text = fallback_text
                            if reasoning_text and on_reasoning_delta:
                                on_reasoning_delta(reasoning_text)
                    continue

                if (
                    isinstance(payload, ThreadTokenUsageUpdatedNotification)
                    and payload.turn_id == turn_id
                ):
                    reasoning_tokens = payload.token_usage.last.reasoning_output_tokens
                    usage = {
                        "input_tokens": payload.token_usage.last.input_tokens,
                        "output_tokens": payload.token_usage.last.output_tokens,
                        "total_tokens": payload.token_usage.last.total_tokens,
                        "input_tokens_details": {
                            "cached_tokens": payload.token_usage.last.cached_input_tokens,
                        },
                        "output_tokens_details": {
                            "reasoning_tokens": payload.token_usage.last.reasoning_output_tokens,
                        },
                    }
                    reasoning_log(
                        "sdk.event",
                        f"method={event_method}",
                        "type=token_usage",
                        f"input_tokens={payload.token_usage.last.input_tokens}",
                        f"output_tokens={payload.token_usage.last.output_tokens}",
                        f"reasoning_tokens={reasoning_tokens}",
                    )
                    continue

                if isinstance(payload, TurnCompletedNotification) and payload.turn.id == turn_id:
                    status = payload.turn.status.value
                    reasoning_log(
                        "turn.completed",
                        f"turn_id={turn_id}",
                        f"status={status}",
                        f"reasoning_chars={len(reasoning_text)}",
                        f"reasoning_tokens={reasoning_tokens}",
                        f"reasoning_events={reasoning_event_count}",
                    )
                    if (reasoning_effort or reasoning_summary) and not reasoning_text:
                        reasoning_log(
                            "turn.reasoning_missing",
                            f"turn_id={turn_id}",
                            f"reasoning_tokens={reasoning_tokens}",
                            f"reasoning_requested_effort={reasoning_effort or 'none'}",
                            f"reasoning_requested_summary={reasoning_summary or 'none'}",
                        )
                    if payload.turn.error is not None:
                        error = {
                            "message": payload.turn.error.message,
                            "codex_error_info": to_json_compatible(
                                payload.turn.error.codex_error_info
                            ),
                        }
                    break
        finally:
            client.close()

        debug_log(
            "run_turn.done",
            f"status={status}",
            f"tool_calls={len(tool_calls)}",
            f"output_chars={len(output_text)}",
            f"reasoning_chars={len(reasoning_text)}",
        )

        return TurnResult(
            thread_id=thread_id,
            turn_id=turn_id,
            output_text=output_text,
            reasoning_text=reasoning_text,
            status=status,
            usage=usage,
            error=error,
            tool_calls=tool_calls,
        )

    def list_models(self, api_key: str | None) -> dict[str, Any]:
        config = self._app_server_config(api_key)
        client = AppServerClient(config=config, approval_handler=_deny_approval)
        try:
            client.start()
            client.initialize()
            models: ModelListResponse = client.model_list(include_hidden=False)
            data = [
                {
                    "id": model.id,
                    "object": "model",
                    "created": 0,
                    "owned_by": "codex",
                    "display_name": model.display_name,
                }
                for model in models.data
            ]
            return {"object": "list", "data": data}
        finally:
            client.close()

    def create_response(
        self,
        *,
        payload: dict[str, Any],
        api_key: str | None,
        request_cwd: str | None,
        request_config_overrides: dict[str, Any] | None,
        function_tools: list[dict[str, Any]],
        provided_tool_outputs: dict[str, list[dict[str, Any]]],
        on_delta: Callable[[str], None] | None = None,
    ) -> tuple[str, dict[str, Any]]:
        input_items = extract_turn_input_items(payload)
        input_items = merge_input_items_with_tool_outputs(input_items, provided_tool_outputs)
        input_items = ensure_non_empty_turn_input(input_items)
        if not input_items:
            raise ValueError("request input is empty; provide `input` text/image/file")

        model = payload.get("model")
        model_name = model if isinstance(model, str) and model else "codex-default"

        previous_response_id_raw = payload.get("previous_response_id")
        previous_response_id = (
            previous_response_id_raw
            if isinstance(previous_response_id_raw, str) and previous_response_id_raw
            else None
        )
        reasoning_effort, reasoning_summary = extract_reasoning_options(payload)
        response_tools_raw = payload.get("tools")
        response_tools = response_tools_raw if isinstance(response_tools_raw, list) else []

        response_id = make_id("resp")
        message_id = make_id("msg")

        result = self._run_turn(
            input_items=input_items,
            model=model,
            reasoning_effort=reasoning_effort,
            reasoning_summary=reasoning_summary,
            previous_response_id=previous_response_id,
            api_key=api_key,
            request_cwd=request_cwd,
            request_config_overrides=request_config_overrides,
            function_tools=function_tools,
            provided_tool_outputs=provided_tool_outputs,
            on_delta=on_delta,
        )

        self._store.put(response_id, result.thread_id)

        unresolved_calls = [
            call for call in result.tool_calls if call.call_id not in provided_tool_outputs
        ]
        if unresolved_calls:
            function_outputs = [
                {
                    "id": make_id("fc"),
                    "type": "function_call",
                    "call_id": call.call_id,
                    "name": call.name,
                    "arguments": encode_tool_arguments(call.arguments),
                }
                for call in unresolved_calls
            ]
            body = {
                "id": response_id,
                "object": "response",
                "created_at": int(time.time()),
                "status": "completed",
                "model": model_name,
                "output": function_outputs,
                "output_text": "",
                "usage": result.usage,
                "error": result.error,
                "previous_response_id": previous_response_id,
                "tools": response_tools,
                "metadata": {
                    "thread_id": result.thread_id,
                    "turn_id": result.turn_id,
                    "pending_tool_calls": [
                        {
                            "call_id": call.call_id,
                            "name": call.name,
                        }
                        for call in unresolved_calls
                    ],
                },
            }
            if result.reasoning_text:
                body["reasoning"] = result.reasoning_text
        else:
            body = {
                "id": response_id,
                "object": "response",
                "created_at": int(time.time()),
                "status": result.status,
                "model": model_name,
                "output": [
                    {
                        "id": message_id,
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": [
                            {
                                "type": "output_text",
                                "text": result.output_text,
                            }
                        ],
                    }
                ],
                "output_text": result.output_text,
                "usage": result.usage,
                "error": result.error,
                "previous_response_id": previous_response_id,
                "tools": response_tools,
                "metadata": {
                    "thread_id": result.thread_id,
                    "turn_id": result.turn_id,
                },
            }
            if result.reasoning_text:
                body["reasoning"] = result.reasoning_text
        return response_id, body


class GatewayServer(ThreadingHTTPServer):
    def __init__(self, server_address: tuple[str, int], cfg: GatewayConfig):
        self.cfg = cfg
        self.store = ResponseThreadStore(cfg.state_db_path)
        self.bridge = CodexBridge(cfg, self.store)
        super().__init__(server_address, GatewayHandler)

    def server_close(self) -> None:
        try:
            self.store.close()
        finally:
            super().server_close()


class GatewayHandler(BaseHTTPRequestHandler):
    server_version = "CodexOpenAIGateway/0.1"

    @property
    def gateway(self) -> GatewayServer:
        return self.server  # type: ignore[return-value]

    def do_OPTIONS(self) -> None:
        self.send_response(HTTPStatus.NO_CONTENT)
        self._write_common_headers()
        self.end_headers()

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        try:
            if path == "/healthz":
                self._write_json(HTTPStatus.OK, {"ok": True})
                return

            if path == "/v1/models":
                api_key = extract_bearer_token(self.headers.get("Authorization"))
                body = self.gateway.bridge.list_models(api_key)
                self._write_json(HTTPStatus.OK, body)
                return

            self._write_json(HTTPStatus.NOT_FOUND, error_payload("not_found", "not found"))
        except Exception as exc:  # noqa: BLE001
            self._write_json(
                HTTPStatus.INTERNAL_SERVER_ERROR,
                error_payload("server_error", str(exc)),
            )

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        if path != "/v1/responses":
            self._write_json(HTTPStatus.NOT_FOUND, error_payload("not_found", "not found"))
            return

        try:
            payload = self._read_json_body()
            request_cwd = extract_request_cwd(payload)
            request_config_overrides = extract_request_config_overrides(payload)
            function_tools = extract_function_tools(payload)
            provided_tool_outputs = extract_function_call_outputs(payload)
            stream = bool(payload.get("stream", False))
            api_key = extract_bearer_token(self.headers.get("Authorization"))
            raw_tools = payload.get("tools")
            requested_tools_count = len(raw_tools) if isinstance(raw_tools, list) else 0
            reasoning_effort, reasoning_summary = extract_reasoning_options(payload)
            reasoning_raw = payload.get("reasoning")
            debug_log(
                "http.request",
                "POST /v1/responses",
                f"stream={stream}",
                f"cwd={request_cwd or self.gateway.cfg.cwd or 'default'}",
                f"tools={requested_tools_count}",
                f"function_tools={len(function_tools)}",
                f"tool_outputs={len(provided_tool_outputs)}",
            )
            reasoning_log(
                "request.received",
                f"stream={stream}",
                f"reasoning_field={'yes' if 'reasoning' in payload else 'no'}",
                f"reasoning_type={type(reasoning_raw).__name__ if 'reasoning' in payload else 'missing'}",
                f"effort={reasoning_effort or 'none'}",
                f"summary={reasoning_summary or 'none'}",
                f"model={payload.get('model') if isinstance(payload.get('model'), str) else 'default'}",
                f"prev_response={'yes' if isinstance(payload.get('previous_response_id'), str) and payload.get('previous_response_id') else 'no'}",
            )

            if stream:
                self._handle_stream(
                    payload,
                    api_key,
                    request_cwd,
                    request_config_overrides,
                    function_tools,
                    provided_tool_outputs,
                )
                return

            _, body = self.gateway.bridge.create_response(
                payload=payload,
                api_key=api_key,
                request_cwd=request_cwd,
                request_config_overrides=request_config_overrides,
                function_tools=function_tools,
                provided_tool_outputs=provided_tool_outputs,
            )
            status = HTTPStatus.OK if body.get("status") != "failed" else HTTPStatus.BAD_GATEWAY
            self._write_json(status, body)
        except ValueError as exc:
            self._write_json(HTTPStatus.BAD_REQUEST, error_payload("invalid_request", str(exc)))
        except Exception as exc:  # noqa: BLE001
            self._write_json(
                HTTPStatus.INTERNAL_SERVER_ERROR,
                error_payload("server_error", str(exc)),
            )

    def _handle_stream(
        self,
        payload: dict[str, Any],
        api_key: str | None,
        request_cwd: str | None,
        request_config_overrides: dict[str, Any] | None,
        function_tools: list[dict[str, Any]],
        provided_tool_outputs: dict[str, list[dict[str, Any]]],
    ) -> None:
        self.send_response(HTTPStatus.OK)
        self._write_common_headers()
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()

        response_id = make_id("resp")
        model_raw = payload.get("model")
        model_name = model_raw if isinstance(model_raw, str) and model_raw else "codex-default"
        previous_response_id = (
            payload.get("previous_response_id")
            if isinstance(payload.get("previous_response_id"), str)
            else None
        )
        response_tools_raw = payload.get("tools")
        response_tools = response_tools_raw if isinstance(response_tools_raw, list) else []
        created_at = int(time.time())
        sequence_number = 0
        reasoning_effort, reasoning_summary = extract_reasoning_options(payload)
        reasoning_log(
            "stream.start",
            f"response_id={response_id}",
            f"effort={reasoning_effort or 'none'}",
            f"summary={reasoning_summary or 'none'}",
            f"cwd={request_cwd or self.gateway.cfg.cwd or 'default'}",
            f"function_tools={len(function_tools)}",
            f"tool_outputs={len(provided_tool_outputs)}",
        )

        def send_stream_event(event: dict[str, Any]) -> None:
            nonlocal sequence_number
            event["sequence_number"] = sequence_number
            event_type = event.get("type")
            if event_type == "response.reasoning.delta":
                delta = event.get("delta")
                reasoning_log(
                    "stream.emit",
                    f"type={event_type}",
                    f"sequence={sequence_number}",
                    f"chars={len(delta) if isinstance(delta, str) else 0}",
                )
            elif event_type == "response.reasoning.done":
                text = event.get("text")
                reasoning_log(
                    "stream.emit",
                    f"type={event_type}",
                    f"sequence={sequence_number}",
                    f"chars={len(text) if isinstance(text, str) else 0}",
                )
            sequence_number += 1
            self._send_sse(event)

        def send_done_marker() -> None:
            reasoning_log("stream.done", f"response_id={response_id}", f"sequence={sequence_number}")
            self.wfile.write(b"event: done\n")
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            self.close_connection = True

        def message_item(message_id: str, text: str, *, status: str) -> dict[str, Any]:
            return {
                "id": message_id,
                "type": "message",
                "status": status,
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": text,
                        "annotations": [],
                    }
                ],
            }

        def response_obj(
            *,
            status: str,
            output: list[dict[str, Any]],
            usage: dict[str, Any] | None = None,
            error: dict[str, Any] | None = None,
            reasoning: str | None = None,
            previous_response_id: str | None = None,
            metadata: dict[str, Any] | None = None,
        ) -> dict[str, Any]:
            body: dict[str, Any] = {
                "id": response_id,
                "object": "response",
                "created_at": created_at,
                "status": status,
                "model": model_name,
                "output": output,
                "parallel_tool_calls": False,
                "tool_choice": "auto",
                "tools": response_tools,
            }
            if usage is not None:
                body["usage"] = usage
            if error is not None:
                body["error"] = error
            if reasoning is not None:
                body["reasoning"] = reasoning
            if previous_response_id is not None:
                body["previous_response_id"] = previous_response_id
            if metadata is not None:
                body["metadata"] = metadata
            return body

        send_stream_event(
            {
                "type": "response.created",
                "response": response_obj(
                    status="in_progress",
                    output=[],
                    previous_response_id=previous_response_id,
                ),
            }
        )

        try:
            input_items = extract_turn_input_items(payload)
            input_items = merge_input_items_with_tool_outputs(input_items, provided_tool_outputs)
            input_items = ensure_non_empty_turn_input(input_items)
            if not input_items:
                raise ValueError("request input is empty; provide `input` text/image/file")

            if function_tools:
                tool_message_id = make_id("msg")
                tool_chunks: list[str] = []
                reasoning_chunks: list[str] = []
                tool_message_started = False

                def ensure_tool_message_started() -> None:
                    nonlocal tool_message_started
                    if tool_message_started:
                        return
                    send_stream_event(
                        {
                            "type": "response.output_item.added",
                            "output_index": 0,
                            "item": {
                                "id": tool_message_id,
                                "type": "message",
                                "status": "in_progress",
                                "role": "assistant",
                                "content": [],
                            },
                        }
                    )
                    send_stream_event(
                        {
                            "type": "response.content_part.added",
                            "output_index": 0,
                            "item_id": tool_message_id,
                            "content_index": 0,
                            "part": {
                                "type": "output_text",
                                "text": "",
                                "annotations": [],
                            },
                        }
                    )
                    tool_message_started = True

                def tool_on_delta(delta: str) -> None:
                    tool_chunks.append(delta)
                    ensure_tool_message_started()
                    send_stream_event(
                        {
                            "type": "response.output_text.delta",
                            "output_index": 0,
                            "item_id": tool_message_id,
                            "content_index": 0,
                            "delta": delta,
                            "logprobs": [],
                        }
                    )

                def tool_on_reasoning_delta(delta: str) -> None:
                    if not delta:
                        return
                    reasoning_chunks.append(delta)
                    send_stream_event(
                        {
                            "type": "response.reasoning.delta",
                            "delta": delta,
                        }
                    )

                result = self.gateway.bridge._run_turn(
                    input_items=input_items,
                    model=model_raw if isinstance(model_raw, str) else None,
                    reasoning_effort=reasoning_effort,
                    reasoning_summary=reasoning_summary,
                    previous_response_id=previous_response_id,
                    api_key=api_key,
                    request_cwd=request_cwd,
                    request_config_overrides=request_config_overrides,
                    function_tools=function_tools,
                    provided_tool_outputs=provided_tool_outputs,
                    on_delta=tool_on_delta,
                    on_reasoning_delta=tool_on_reasoning_delta,
                )

                self.gateway.store.put(response_id, result.thread_id)

                unresolved_calls = [
                    call for call in result.tool_calls if call.call_id not in provided_tool_outputs
                ]
                tool_full_text = result.output_text or "".join(tool_chunks)
                reasoning_full_text = result.reasoning_text or "".join(reasoning_chunks)
                if reasoning_full_text:
                    send_stream_event(
                        {
                            "type": "response.reasoning.done",
                            "text": reasoning_full_text,
                        }
                    )

                if unresolved_calls:
                    output_items: list[dict[str, Any]] = []
                    function_items: list[dict[str, Any]] = []
                    pending_calls: list[dict[str, str]] = []
                    output_index_offset = 0

                    if tool_message_started:
                        send_stream_event(
                            {
                                "type": "response.output_text.done",
                                "output_index": 0,
                                "item_id": tool_message_id,
                                "content_index": 0,
                                "text": tool_full_text,
                                "logprobs": [],
                            }
                        )
                        send_stream_event(
                            {
                                "type": "response.content_part.done",
                                "output_index": 0,
                                "item_id": tool_message_id,
                                "content_index": 0,
                                "part": {
                                    "type": "output_text",
                                    "text": tool_full_text,
                                    "annotations": [],
                                },
                            }
                        )
                        done_message = message_item(tool_message_id, tool_full_text, status="completed")
                        send_stream_event(
                            {
                                "type": "response.output_item.done",
                                "output_index": 0,
                                "item": done_message,
                            }
                        )
                        output_items.append(done_message)
                        output_index_offset = 1

                    for call_index, call in enumerate(unresolved_calls):
                        output_index = output_index_offset + call_index
                        function_item_id = make_id("fc")
                        encoded_arguments = encode_tool_arguments(call.arguments)

                        in_progress_item = {
                            "id": function_item_id,
                            "type": "function_call",
                            "status": "in_progress",
                            "call_id": call.call_id,
                            "name": call.name,
                            "arguments": "",
                        }
                        send_stream_event(
                            {
                                "type": "response.output_item.added",
                                "output_index": output_index,
                                "item": in_progress_item,
                            }
                        )

                        if encoded_arguments:
                            send_stream_event(
                                {
                                    "type": "response.function_call_arguments.delta",
                                    "output_index": output_index,
                                    "item_id": function_item_id,
                                    "delta": encoded_arguments,
                                }
                            )

                        send_stream_event(
                            {
                                "type": "response.function_call_arguments.done",
                                "output_index": output_index,
                                "item_id": function_item_id,
                                "name": call.name,
                                "arguments": encoded_arguments,
                            }
                        )

                        done_item = {
                            "id": function_item_id,
                            "type": "function_call",
                            "status": "completed",
                            "call_id": call.call_id,
                            "name": call.name,
                            "arguments": encoded_arguments,
                        }
                        send_stream_event(
                            {
                                "type": "response.output_item.done",
                                "output_index": output_index,
                                "item": done_item,
                            }
                        )

                        function_items.append(done_item)
                        pending_calls.append(
                            {
                                "call_id": call.call_id,
                                "name": call.name,
                            }
                        )

                    completed = response_obj(
                        status="completed",
                        output=[*output_items, *function_items],
                        usage=result.usage,
                        error=result.error,
                        reasoning=reasoning_full_text if reasoning_full_text else None,
                        previous_response_id=previous_response_id,
                        metadata={
                            "thread_id": result.thread_id,
                            "turn_id": result.turn_id,
                            "pending_tool_calls": pending_calls,
                        },
                    )
                    send_stream_event({"type": "response.completed", "response": completed})
                    send_done_marker()
                    return

                ensure_tool_message_started()
                if not tool_chunks and tool_full_text:
                    send_stream_event(
                        {
                            "type": "response.output_text.delta",
                            "output_index": 0,
                            "item_id": tool_message_id,
                            "content_index": 0,
                            "delta": tool_full_text,
                            "logprobs": [],
                        }
                    )
                send_stream_event(
                    {
                        "type": "response.output_text.done",
                        "output_index": 0,
                        "item_id": tool_message_id,
                        "content_index": 0,
                        "text": tool_full_text,
                        "logprobs": [],
                    }
                )
                send_stream_event(
                    {
                        "type": "response.content_part.done",
                        "output_index": 0,
                        "item_id": tool_message_id,
                        "content_index": 0,
                        "part": {
                            "type": "output_text",
                            "text": tool_full_text,
                            "annotations": [],
                        },
                    }
                )
                done_message = message_item(tool_message_id, tool_full_text, status="completed")
                send_stream_event(
                    {
                        "type": "response.output_item.done",
                        "output_index": 0,
                        "item": done_message,
                    }
                )
                completed = response_obj(
                    status=result.status,
                    output=[done_message],
                    usage=result.usage,
                    error=result.error,
                    reasoning=reasoning_full_text if reasoning_full_text else None,
                    previous_response_id=previous_response_id,
                    metadata={
                        "thread_id": result.thread_id,
                        "turn_id": result.turn_id,
                    },
                )

                event_type = "response.completed" if result.status != "failed" else "response.failed"
                send_stream_event({"type": event_type, "response": completed})
                send_done_marker()
                return

            message_id = make_id("msg")
            send_stream_event(
                {
                    "type": "response.output_item.added",
                    "output_index": 0,
                    "item": {
                        "id": message_id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": [],
                    },
                }
            )
            send_stream_event(
                {
                    "type": "response.content_part.added",
                    "output_index": 0,
                    "item_id": message_id,
                    "content_index": 0,
                    "part": {
                        "type": "output_text",
                        "text": "",
                        "annotations": [],
                    },
                }
            )

            chunks: list[str] = []
            reasoning_chunks: list[str] = []

            def on_delta(delta: str) -> None:
                chunks.append(delta)
                send_stream_event(
                    {
                        "type": "response.output_text.delta",
                        "output_index": 0,
                        "item_id": message_id,
                        "content_index": 0,
                        "delta": delta,
                        "logprobs": [],
                    }
                )

            def on_reasoning_delta(delta: str) -> None:
                if not delta:
                    return
                reasoning_chunks.append(delta)
                send_stream_event(
                    {
                        "type": "response.reasoning.delta",
                        "delta": delta,
                    }
                )

            result = self.gateway.bridge._run_turn(
                input_items=input_items,
                model=model_raw if isinstance(model_raw, str) else None,
                reasoning_effort=reasoning_effort,
                reasoning_summary=reasoning_summary,
                previous_response_id=previous_response_id,
                api_key=api_key,
                request_cwd=request_cwd,
                request_config_overrides=request_config_overrides,
                function_tools=function_tools,
                provided_tool_outputs=provided_tool_outputs,
                on_delta=on_delta,
                on_reasoning_delta=on_reasoning_delta,
            )

            self.gateway.store.put(response_id, result.thread_id)

            full_text = result.output_text
            if not full_text:
                full_text = "".join(chunks)
            reasoning_full_text = result.reasoning_text or "".join(reasoning_chunks)
            if reasoning_full_text:
                send_stream_event(
                    {
                        "type": "response.reasoning.done",
                        "text": reasoning_full_text,
                    }
                )

            send_stream_event(
                {
                    "type": "response.output_text.done",
                    "output_index": 0,
                    "item_id": message_id,
                    "content_index": 0,
                    "text": full_text,
                    "logprobs": [],
                }
            )
            send_stream_event(
                {
                    "type": "response.content_part.done",
                    "output_index": 0,
                    "item_id": message_id,
                    "content_index": 0,
                    "part": {
                        "type": "output_text",
                        "text": full_text,
                        "annotations": [],
                    },
                }
            )
            done_message = message_item(message_id, full_text, status="completed")
            send_stream_event(
                {
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": done_message,
                }
            )

            completed = response_obj(
                status=result.status,
                output=[done_message],
                usage=result.usage,
                error=result.error,
                reasoning=reasoning_full_text if reasoning_full_text else None,
                previous_response_id=previous_response_id,
                metadata={
                    "thread_id": result.thread_id,
                    "turn_id": result.turn_id,
                },
            )

            event_type = "response.completed" if result.status != "failed" else "response.failed"
            send_stream_event({"type": event_type, "response": completed})
            send_done_marker()
        except BrokenPipeError:
            return
        except Exception as exc:  # noqa: BLE001
            send_stream_event(
                {
                    "type": "error",
                    "code": "server_error",
                    "message": str(exc),
                    "param": None,
                }
            )
            send_done_marker()

    def _write_common_headers(self) -> None:
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET,POST,OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Authorization,Content-Type")

    def _read_json_body(self) -> dict[str, Any]:
        raw_len = self.headers.get("Content-Length")
        if raw_len is None:
            raise ValueError("missing Content-Length")
        try:
            content_length = int(raw_len)
        except ValueError as exc:  # noqa: PERF203
            raise ValueError("invalid Content-Length") from exc

        raw = self.rfile.read(content_length)
        if not raw:
            return {}

        try:
            payload = json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError as exc:
            raise ValueError("request body is not valid JSON") from exc
        if not isinstance(payload, dict):
            raise ValueError("request body must be a JSON object")
        return payload

    def _write_json(self, status: HTTPStatus, body: dict[str, Any]) -> None:
        encoded = json.dumps(body, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self._write_common_headers()
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _send_sse(self, data: dict[str, Any]) -> None:
        payload = json.dumps(data, ensure_ascii=False)
        event_name = data.get("type")
        if isinstance(event_name, str) and event_name:
            self.wfile.write(f"event: {event_name}\n".encode("utf-8"))
        self.wfile.write(f"data: {payload}\n\n".encode("utf-8"))
        self.wfile.flush()


def extract_bearer_token(value: str | None) -> str | None:
    if not value:
        return None
    parts = value.strip().split(" ", 1)
    if len(parts) != 2 or parts[0].lower() != "bearer":
        return None
    token = parts[1].strip()
    return token or None


def extract_turn_input_items(payload: dict[str, Any]) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []

    instructions = payload.get("instructions")
    if isinstance(instructions, str) and instructions.strip():
        items.append({"type": "text", "text": instructions.strip()})

    input_value = payload.get("input")
    if input_value is None and "messages" in payload:
        input_value = payload.get("messages")

    collect_turn_input_items(input_value, items)
    return items


def collect_turn_input_items(node: Any, out: list[dict[str, Any]]) -> None:
    if node is None:
        return

    if isinstance(node, str):
        text = node.strip()
        if text:
            out.append({"type": "text", "text": text})
        return

    if isinstance(node, list):
        for item in node:
            collect_turn_input_items(item, out)
        return

    if not isinstance(node, dict):
        return

    node_type = node.get("type")
    if isinstance(node_type, str):
        normalized = node_type.strip()

        if normalized in {"text", "input_text", "output_text", "inputText", "outputText"}:
            text = node.get("text")
            if isinstance(text, str) and text.strip():
                out.append({"type": "text", "text": text.strip()})
            return

        if normalized in {"image", "input_image", "image_url", "inputImage", "imageUrl"}:
            image_url = extract_image_url(node)
            if image_url:
                out.append({"type": "image", "url": image_url})
            else:
                image_ref = extract_image_reference_text(node)
                if image_ref:
                    out.append({"type": "text", "text": image_ref})
            return

        if normalized in {"localImage", "local_image"}:
            local_path = normalize_optional_string(node.get("path"))
            if local_path:
                out.append({"type": "localImage", "path": local_path})
            return

        if normalized in {"input_file", "file", "inputFile"}:
            text = extract_input_file_text(node)
            if text:
                out.append({"type": "text", "text": text})
            return

        if normalized == "message":
            collect_turn_input_items(node.get("content"), out)
            return

    # Legacy chat-style message object: {"role":"user","content":...}
    if "role" in node and "content" in node:
        collect_turn_input_items(node.get("content"), out)
        return

    if "content" in node:
        collect_turn_input_items(node.get("content"), out)
        return

    text = node.get("text")
    if isinstance(text, str) and text.strip():
        out.append({"type": "text", "text": text.strip()})


def normalize_optional_string(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    trimmed = value.strip()
    return trimmed if trimmed else None


def extract_image_url(node: dict[str, Any]) -> str | None:
    image_url = node.get("image_url", node.get("imageUrl", node.get("url")))
    if isinstance(image_url, dict):
        candidate = image_url.get("url")
        return normalize_optional_string(candidate)
    return normalize_optional_string(image_url)


def extract_image_reference_text(node: dict[str, Any]) -> str | None:
    file_id = normalize_optional_string(node.get("file_id")) or normalize_optional_string(
        node.get("fileId")
    )
    if not file_id:
        image_url = node.get("image_url", node.get("imageUrl"))
        if isinstance(image_url, dict):
            file_id = normalize_optional_string(image_url.get("file_id")) or normalize_optional_string(
                image_url.get("fileId")
            )
    if not file_id:
        return None
    return f"[image attachment via file_id={file_id}; direct URL not provided]"


def extract_input_file_text(node: dict[str, Any]) -> str | None:
    filename = normalize_optional_string(node.get("filename")) or normalize_optional_string(
        node.get("name")
    )
    mime_type = (
        normalize_optional_string(node.get("mime_type"))
        or normalize_optional_string(node.get("mimeType"))
        or normalize_optional_string(node.get("content_type"))
        or normalize_optional_string(node.get("contentType"))
    )
    file_id = normalize_optional_string(node.get("file_id")) or normalize_optional_string(
        node.get("fileId")
    )

    inline_text = normalize_optional_string(node.get("text"))
    if inline_text:
        return format_file_text_block(filename, mime_type, inline_text)

    file_data = node.get("file_data", node.get("fileData", node.get("data")))
    if isinstance(file_data, str) and file_data.strip():
        decoded = decode_file_data(file_data.strip())
        if decoded is not None:
            textual = decode_bytes_to_text(decoded)
            if textual:
                return format_file_text_block(filename, mime_type, textual)

    label = filename or file_id or "unnamed"
    if file_id:
        return (
            f"Attachment: {label}"
            f"{f' ({mime_type})' if mime_type else ''}"
            f" [file_id={file_id}; content not inlined]"
        )
    return (
        f"Attachment: {label}"
        f"{f' ({mime_type})' if mime_type else ''}"
        " [binary or unsupported file content omitted]"
    )


def format_file_text_block(filename: str | None, mime_type: str | None, text: str) -> str:
    name = filename or "attachment"
    mime = mime_type or "application/octet-stream"
    content = text.strip()
    if not content:
        content = "[empty]"
    max_chars = 20_000
    if len(content) > max_chars:
        content = f"{content[:max_chars]}\n...[truncated]"
    return f"Attachment: {name} ({mime})\n\n{content}"


def decode_file_data(file_data: str) -> bytes | None:
    if file_data.startswith("data:"):
        comma_idx = file_data.find(",")
        if comma_idx == -1:
            return None
        meta = file_data[:comma_idx].lower()
        body = file_data[comma_idx + 1 :]
        if ";base64" in meta:
            try:
                return base64.b64decode(body, validate=False)
            except binascii.Error:
                return None
        return body.encode("utf-8", errors="replace")

    # Try base64 first; fallback to raw text bytes.
    try:
        return base64.b64decode(file_data, validate=True)
    except binascii.Error:
        return file_data.encode("utf-8", errors="replace")


def decode_bytes_to_text(data: bytes) -> str | None:
    if not data:
        return ""
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        try:
            return data.decode("utf-8", errors="replace")
        except Exception:
            return None


def extract_function_tools(payload: dict[str, Any]) -> list[dict[str, Any]]:
    tools = payload.get("tools")
    if tools is None:
        return []
    if not isinstance(tools, list):
        raise ValueError("`tools` must be a list")

    dynamic_tools: list[dict[str, Any]] = []
    seen_names: set[str] = set()
    for tool in tools:
        if not isinstance(tool, dict):
            raise ValueError("each item in `tools` must be an object")
        if tool.get("type") != "function":
            continue

        function_block = tool.get("function")
        if function_block is not None and not isinstance(function_block, dict):
            raise ValueError("function tool `function` must be an object")

        name_raw = tool.get("name")
        if name_raw is None and isinstance(function_block, dict):
            name_raw = function_block.get("name")
        if not isinstance(name_raw, str) or not name_raw.strip():
            raise ValueError("function tool requires non-empty `name`")
        name = name_raw.strip()

        description_raw = tool.get("description")
        if description_raw is None and isinstance(function_block, dict):
            description_raw = function_block.get("description")
        description = description_raw if isinstance(description_raw, str) else ""

        parameters_raw = tool.get("parameters")
        if parameters_raw is None and isinstance(function_block, dict):
            parameters_raw = function_block.get("parameters")
        parameters = parameters_raw if parameters_raw is not None else {"type": "object", "properties": {}}
        if not isinstance(parameters, dict):
            raise ValueError(f"function tool `{name}` `parameters` must be an object")

        if name in seen_names:
            raise ValueError(f"duplicate function tool name: {name}")
        seen_names.add(name)

        dynamic_tools.append(
            {
                "name": name,
                "description": description,
                "inputSchema": parameters,
            }
        )

    return dynamic_tools


def extract_function_call_outputs(payload: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    outputs: dict[str, list[dict[str, Any]]] = {}
    input_value = payload.get("input")
    if input_value is None and "messages" in payload:
        input_value = payload.get("messages")

    collect_function_call_outputs(input_value, outputs)
    return outputs


def collect_function_call_outputs(
    node: Any,
    out: dict[str, list[dict[str, Any]]],
) -> None:
    if node is None:
        return
    if isinstance(node, list):
        for item in node:
            collect_function_call_outputs(item, out)
        return
    if not isinstance(node, dict):
        return

    node_type = node.get("type")
    if node_type in {"function_call_output", "custom_tool_call_output"}:
        call_id_raw = node.get("call_id", node.get("callId"))
        if not isinstance(call_id_raw, str) or not call_id_raw.strip():
            raise ValueError(f"{node_type} requires non-empty `call_id`")
        call_id = call_id_raw.strip()
        out[call_id] = normalize_function_call_output(node.get("output"))
        return

    if "content" in node:
        collect_function_call_outputs(node.get("content"), out)


def normalize_function_call_output(value: Any) -> list[dict[str, Any]]:
    if isinstance(value, str):
        return [{"type": "inputText", "text": value}]

    if isinstance(value, list):
        items: list[dict[str, Any]] = []
        for item in value:
            if not isinstance(item, dict):
                items.append({"type": "inputText", "text": json.dumps(item, ensure_ascii=False)})
                continue
            item_type = item.get("type")
            if item_type in {"input_text", "text", "output_text"}:
                text = item.get("text")
                if isinstance(text, str):
                    items.append({"type": "inputText", "text": text})
                    continue
            if item_type in {"input_image", "image"}:
                image_url = item.get("image_url", item.get("imageUrl", item.get("url")))
                if isinstance(image_url, str) and image_url:
                    items.append({"type": "inputImage", "imageUrl": image_url})
                    continue
            items.append({"type": "inputText", "text": json.dumps(item, ensure_ascii=False)})
        return items or [{"type": "inputText", "text": ""}]

    if value is None:
        return [{"type": "inputText", "text": ""}]

    return [{"type": "inputText", "text": json.dumps(value, ensure_ascii=False)}]


def merge_prompt_with_tool_outputs(
    prompt: str,
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> str:
    lines: list[str] = [
        "以下是客户端执行工具后返回的结果，请基于这些结果继续回答用户问题：",
    ]
    for call_id, content_items in provided_tool_outputs.items():
        text_parts: list[str] = []
        for content in content_items:
            content_type = content.get("type")
            if content_type == "inputText":
                text = content.get("text")
                if isinstance(text, str):
                    text_parts.append(text)
            elif content_type == "inputImage":
                image_url = content.get("imageUrl")
                if isinstance(image_url, str):
                    text_parts.append(f"[image:{image_url}]")
        lines.append(f"- call_id={call_id}: {' '.join(text_parts).strip()}")

    extra = "\n".join(lines).strip()
    if prompt.strip():
        return f"{prompt}\n\n{extra}"
    return extra


def merge_input_items_with_tool_outputs(
    input_items: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> list[dict[str, Any]]:
    if not provided_tool_outputs:
        return input_items

    out = list(input_items)
    extra = merge_prompt_with_tool_outputs("", provided_tool_outputs).strip()
    if extra:
        out.append({"type": "text", "text": extra})
    return out


def ensure_non_empty_turn_input(input_items: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if not input_items:
        return input_items

    has_text = any(
        isinstance(item, dict)
        and item.get("type") == "text"
        and isinstance(item.get("text"), str)
        and item.get("text").strip()
        for item in input_items
    )
    if has_text:
        return input_items

    # Some upstream paths reject all-nontext turns; add a neutral hint to keep
    # image-only requests valid without forcing users to type extra words.
    out = list(input_items)
    out.append({"type": "text", "text": "请根据上传的图片或附件内容进行分析并回答。"})
    return out


def encode_tool_arguments(arguments: Any) -> str:
    if isinstance(arguments, str):
        return arguments
    return json.dumps(arguments, ensure_ascii=False)


def extract_reasoning_options(payload: dict[str, Any]) -> tuple[str | None, str | None]:
    reasoning = payload.get("reasoning")
    if reasoning is None:
        return None, None

    allowed_efforts = {"none", "minimal", "low", "medium", "high", "xhigh"}
    allowed_summaries = {"none", "auto", "concise", "detailed"}

    if isinstance(reasoning, str):
        normalized = reasoning.strip().lower()
        return (normalized, None) if normalized in allowed_efforts else (None, None)

    if not isinstance(reasoning, dict):
        return None, None

    effort_raw = reasoning.get("effort")
    effort = effort_raw.strip().lower() if isinstance(effort_raw, str) else None
    if effort not in allowed_efforts:
        effort = None

    summary_raw = reasoning.get("summary")
    summary = summary_raw.strip().lower() if isinstance(summary_raw, str) else None
    if summary not in allowed_summaries:
        summary = None

    return effort, summary


def extract_request_cwd(payload: dict[str, Any]) -> str | None:
    return normalize_non_empty_string(payload.get("cwd"), "request `cwd`")


def extract_request_config_overrides(payload: dict[str, Any]) -> dict[str, Any] | None:
    tools = payload.get("tools")
    if tools is not None and not isinstance(tools, list):
        raise ValueError("`tools` must be a list")

    mcp_servers: dict[str, Any] = {}
    for tool in tools or []:
        if not isinstance(tool, dict):
            raise ValueError("each item in `tools` must be an object")

        tool_type = tool.get("type")
        if tool_type != "mcp":
            continue

        label, mcp_config = parse_mcp_tool(tool)
        if label in mcp_servers:
            raise ValueError(f"duplicate mcp server label in `tools`: {label}")
        mcp_servers[label] = mcp_config

    # This gateway is Rust-only. Force-disable Codex built-in tool surfaces so
    # the model only sees caller-provided function/MCP tools.
    config: dict[str, Any] = {
        "features.apps": False,
        "features.plugins": False,
        "features.connectors": False,
        "features.shell_tool": False,
        "features.include_apply_patch_tool": False,
        "features.request_permissions_tool": False,
        "features.web_search": False,
        "features.web_search_cached": False,
        "features.web_search_request": False,
        "include_apply_patch_tool": False,
        "allow_login_shell": False,
        "web_search": "disabled",
        "tools.view_image": False,
        # Always clear thread-level MCP servers from local Codex config so this
        # gateway only uses caller-provided tooling.
        "mcp_servers": mcp_servers,
    }
    debug_log(
        "request.config_overrides",
        f"mcp_servers={len(mcp_servers)}",
        f"keys={','.join(config.keys())}",
    )
    return config


def parse_mcp_tool(tool: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    label = tool.get("server_label", tool.get("server_name"))
    if not isinstance(label, str) or not label.strip():
        raise ValueError("mcp tool requires non-empty `server_label` (or `server_name`)")
    server_label = label.strip()

    server_url = tool.get("server_url", tool.get("url"))
    command = tool.get("command")
    url = normalize_non_empty_string(server_url, "mcp tool `server_url`")
    cmd = normalize_non_empty_string(command, "mcp tool `command`")

    has_url = url is not None
    has_command = cmd is not None
    if has_url == has_command:
        raise ValueError(
            f"mcp tool `{server_label}` must provide exactly one transport: "
            "`server_url` (HTTP) or `command` (stdio)"
        )

    config: dict[str, Any] = {}
    if has_url:
        config["url"] = url
        bearer_token = tool.get("bearer_token")
        if bearer_token is not None:
            raise ValueError(
                f"mcp tool `{server_label}` does not allow inline `bearer_token`; "
                "use `bearer_token_env_var` instead"
            )

        bearer_env = tool.get("bearer_token_env_var")
        if bearer_env is not None:
            bearer_env_name = normalize_non_empty_string(
                bearer_env, f"mcp tool `{server_label}` `bearer_token_env_var`"
            )
            if bearer_env_name is None:
                raise ValueError(
                    f"mcp tool `{server_label}` has invalid `bearer_token_env_var`"
                )
            config["bearer_token_env_var"] = bearer_env_name

        http_headers = validate_string_map(
            tool.get("headers"),
            f"mcp tool `{server_label}` `headers`",
        )
        if http_headers:
            config["http_headers"] = http_headers

        env_http_headers = validate_string_map(
            tool.get("env_headers", tool.get("env_http_headers")),
            f"mcp tool `{server_label}` `env_headers`",
        )
        if env_http_headers:
            config["env_http_headers"] = env_http_headers
    else:
        config["command"] = cmd

        args = validate_string_list(tool.get("args"), f"mcp tool `{server_label}` `args`")
        if args is not None:
            config["args"] = args

        env = validate_string_map(tool.get("env"), f"mcp tool `{server_label}` `env`")
        if env:
            config["env"] = env

        env_vars = validate_string_list(
            tool.get("env_vars"),
            f"mcp tool `{server_label}` `env_vars`",
        )
        if env_vars is not None:
            config["env_vars"] = env_vars

        cwd = tool.get("cwd")
        if cwd is not None:
            cwd_value = normalize_non_empty_string(cwd, f"mcp tool `{server_label}` `cwd`")
            if cwd_value is None:
                raise ValueError(f"mcp tool `{server_label}` has invalid `cwd`")
            config["cwd"] = cwd_value

    enabled_tools = validate_string_list(
        tool.get("enabled_tools", tool.get("allowed_tools")),
        f"mcp tool `{server_label}` `enabled_tools`",
    )
    if enabled_tools is not None:
        config["enabled_tools"] = enabled_tools

    disabled_tools = validate_string_list(
        tool.get("disabled_tools", tool.get("blocked_tools")),
        f"mcp tool `{server_label}` `disabled_tools`",
    )
    if disabled_tools is not None:
        config["disabled_tools"] = disabled_tools

    required = tool.get("required")
    if required is not None:
        if not isinstance(required, bool):
            raise ValueError(f"mcp tool `{server_label}` `required` must be a boolean")
        config["required"] = required

    return server_label, config


def normalize_non_empty_string(value: Any, field_name: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"{field_name} must be a string")
    trimmed = value.strip()
    if not trimmed:
        raise ValueError(f"{field_name} must not be empty")
    return trimmed


def validate_string_list(value: Any, field_name: str) -> list[str] | None:
    if value is None:
        return None
    if not isinstance(value, list):
        raise ValueError(f"{field_name} must be a list of strings")

    out: list[str] = []
    for item in value:
        if not isinstance(item, str) or not item.strip():
            raise ValueError(f"{field_name} must contain non-empty strings")
        out.append(item)
    return out


def validate_string_map(value: Any, field_name: str) -> dict[str, str] | None:
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError(f"{field_name} must be an object of string:string pairs")

    out: dict[str, str] = {}
    for key, item in value.items():
        if not isinstance(key, str) or not key:
            raise ValueError(f"{field_name} contains invalid key")
        if not isinstance(item, str):
            raise ValueError(f"{field_name} contains non-string value for key `{key}`")
        out[key] = item
    return out


def collect_text(node: Any, out: list[str]) -> None:
    if node is None:
        return

    if isinstance(node, str):
        out.append(node)
        return

    if isinstance(node, list):
        for item in node:
            collect_text(item, out)
        return

    if not isinstance(node, dict):
        return

    node_type = node.get("type")
    if node_type in {"text", "input_text", "output_text"}:
        text = node.get("text")
        if isinstance(text, str):
            out.append(text)
        return

    if node_type == "message":
        collect_text(node.get("content"), out)
        return

    if "content" in node:
        collect_text(node.get("content"), out)
        return

    text = node.get("text")
    if isinstance(text, str):
        out.append(text)


def make_id(prefix: str) -> str:
    return f"{prefix}_{uuid.uuid4().hex}"


def error_payload(kind: str, message: str) -> dict[str, Any]:
    return {
        "error": {
            "type": kind,
            "message": message,
        }
    }


def to_json_compatible(value: Any) -> Any:
    if value is None:
        return None
    if isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, list):
        return [to_json_compatible(v) for v in value]
    if isinstance(value, dict):
        return {str(k): to_json_compatible(v) for k, v in value.items()}
    if hasattr(value, "model_dump"):
        return to_json_compatible(value.model_dump(mode="json"))
    if hasattr(value, "value"):
        return to_json_compatible(value.value)
    return str(value)


def resolve_codex_bin(cli_value: str | None) -> str | None:
    if cli_value:
        return cli_value

    env_value = os.environ.get("CODEX_GATEWAY_CODEX_BIN")
    if env_value:
        return env_value

    return shutil.which("codex")


def parse_args() -> GatewayConfig:
    parser = argparse.ArgumentParser(description="OpenAI-compatible gateway backed by codex SDK")
    parser.add_argument("--host", default="127.0.0.1", help="Bind host (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=8089, help="Bind port (default: 8089)")
    parser.add_argument("--codex-bin", default=None)
    parser.add_argument(
        "--state-db",
        default=(
            os.environ.get("CODEX_GATEWAY_STATE_DB")
            or str(Path(__file__).with_name("gateway_state.sqlite3"))
        ),
        help="SQLite file for persisting response_id -> thread_id mappings",
    )
    parser.add_argument(
        "--cwd",
        default=os.environ.get("CODEX_GATEWAY_CWD") or str(REPO_ROOT),
        help="Working directory for codex app-server",
    )
    parser.add_argument(
        "--sandbox",
        choices=["read-only", "workspace-write", "danger-full-access"],
        default=os.environ.get("CODEX_GATEWAY_SANDBOX", "read-only"),
        help="Sandbox mode for new/resumed threads",
    )
    args = parser.parse_args()

    return GatewayConfig(
        host=args.host,
        port=args.port,
        codex_bin=resolve_codex_bin(args.codex_bin),
        cwd=args.cwd,
        sandbox=args.sandbox,
        approval_policy="on-request",
        state_db_path=args.state_db,
    )


def main() -> None:
    cfg = parse_args()
    server = GatewayServer((cfg.host, cfg.port), cfg)
    print(f"OpenAI-compatible gateway listening on http://{cfg.host}:{cfg.port}")
    state_log("sdk", f"source={SDK_IMPORT_SOURCE}")
    state_log("startup", f"state_db={cfg.state_db_path}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
