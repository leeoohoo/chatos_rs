from __future__ import annotations

import json
import sys
import traceback
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import urlparse

from gateway_base.logging import debug_log, reasoning_log
from gateway_base.utils import error_payload
from gateway_core.state_store import ResponseThreadStore
from gateway_http.io import encode_json_body, serialize_sse_event
from gateway_http.routing import (
    build_not_found_body,
    resolve_get_route,
    resolve_post_route,
    response_status_for_body,
)
from gateway_request.parser import parse_responses_request
from gateway_request.payload import extract_bearer_token
from gateway_runtime.bridge import CodexBridge
from gateway_stream.headers import write_default_stream_response_headers
from gateway_stream.main_flow_execution import (
    run_stream_main_flow_with_default_orchestration,
)


class GatewayServer(ThreadingHTTPServer):
    def __init__(self, server_address: tuple[str, int], cfg: Any):
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
        route = resolve_get_route(path)
        try:
            if route == "healthz":
                self._write_json(HTTPStatus.OK, {"ok": True})
                return

            if route == "models":
                api_key = extract_bearer_token(self.headers.get("Authorization"))
                body = self.gateway.bridge.list_models(api_key)
                self._write_json(HTTPStatus.OK, body)
                return

            self._write_json(HTTPStatus.NOT_FOUND, build_not_found_body())
        except Exception as exc:  # noqa: BLE001
            debug_log("http.get.error", f"path={path}", f"error={exc}")
            traceback.print_exc(file=sys.stderr)
            self._write_json(
                HTTPStatus.INTERNAL_SERVER_ERROR,
                error_payload("server_error", str(exc)),
            )

    def do_POST(self) -> None:
        path = urlparse(self.path).path
        route = resolve_post_route(path)
        if route != "responses":
            self._write_json(HTTPStatus.NOT_FOUND, build_not_found_body())
            return

        try:
            payload = self._read_json_body()
            request_context = parse_responses_request(
                payload,
                authorization_header=self.headers.get("Authorization"),
            )
            debug_log(
                "http.request",
                "POST /v1/responses",
                f"stream={request_context.stream}",
                f"cwd={request_context.request_cwd or self.gateway.cfg.cwd or 'default'}",
                f"tools={request_context.requested_tools_count}",
                f"function_tools={len(request_context.function_tools)}",
                f"tool_outputs={len(request_context.provided_tool_outputs)}",
            )
            reasoning_log(
                "request.received",
                f"stream={request_context.stream}",
                f"reasoning_field={'yes' if 'reasoning' in payload else 'no'}",
                f"reasoning_type={type(request_context.reasoning_raw).__name__ if 'reasoning' in payload else 'missing'}",
                f"effort={request_context.reasoning_effort or 'none'}",
                f"summary={request_context.reasoning_summary or 'none'}",
                f"model={payload.get('model') if isinstance(payload.get('model'), str) else 'default'}",
                f"prev_response={'yes' if isinstance(payload.get('previous_response_id'), str) and payload.get('previous_response_id') else 'no'}",
            )

            if request_context.stream:
                self._handle_stream(
                    payload,
                    request_context.api_key,
                    request_context.request_cwd,
                    request_context.request_config_overrides,
                    request_context.function_tools,
                    request_context.provided_tool_outputs,
                )
                return

            _, body = self.gateway.bridge.create_response(
                payload=payload,
                api_key=request_context.api_key,
                request_cwd=request_context.request_cwd,
                request_config_overrides=request_context.request_config_overrides,
                function_tools=request_context.function_tools,
                provided_tool_outputs=request_context.provided_tool_outputs,
            )
            status = response_status_for_body(body)
            self._write_json(status, body)
        except ValueError as exc:
            debug_log("http.post.invalid_request", f"path={path}", f"error={exc}")
            self._write_json(HTTPStatus.BAD_REQUEST, error_payload("invalid_request", str(exc)))
        except Exception as exc:  # noqa: BLE001
            debug_log("http.post.error", f"path={path}", f"error={exc}")
            traceback.print_exc(file=sys.stderr)
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
        write_default_stream_response_headers(target=self)

        run_stream_main_flow_with_default_orchestration(
            payload=payload,
            bridge=self.gateway.bridge,
            store=self.gateway.store,
            api_key=api_key,
            request_cwd=request_cwd,
            request_config_overrides=request_config_overrides,
            function_tools=function_tools,
            provided_tool_outputs=provided_tool_outputs,
            default_cwd=self.gateway.cfg.cwd,
            send_sse=self._send_sse,
            write=self.wfile.write,
            flush=self.wfile.flush,
            connection_target=self,
            debug_logger=debug_log,
        )

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
        encoded = encode_json_body(body)
        try:
            self.send_response(status)
            self._write_common_headers()
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(encoded)
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            debug_log("http.write_json.disconnected", f"status={int(status)}")
        finally:
            self.close_connection = True

    def _send_sse(self, data: dict[str, Any]) -> None:
        self.wfile.write(serialize_sse_event(data))
        self.wfile.flush()
