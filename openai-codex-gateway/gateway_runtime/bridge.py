from __future__ import annotations

import time
from typing import Any, Callable

from gateway_base.logging import debug_log, reasoning_log
from gateway_base.policy import (
    deny_approval,
    extract_allowed_function_tool_names,
    extract_allowed_mcp_server_labels,
)
from gateway_base.types import GatewayConfig, ToolCallRecord, TurnResult
from gateway_runtime.approval_handler import handle_server_request
from gateway_runtime.sdk_types import (
    AppServerClient,
    AppServerConfig,
    ModelListResponse,
)
from gateway_runtime.thread_session import (
    build_resume_fingerprint,
    build_thread_session_params,
    build_turn_start_params,
    resolve_thread_id,
)
from gateway_runtime.turn_loop import drive_turn_notifications
from gateway_runtime.turn_event_processing import process_turn_notification
from gateway_runtime.turn_state import TurnRuntimeState
from create_response.completion import finalize_create_response
from create_response.parser import parse_create_response_context
from create_response.turn_runner import run_create_response_turn
from gateway_base.utils import make_id


class CodexBridge:
    def __init__(self, cfg: GatewayConfig, store: Any) -> None:
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

    @staticmethod
    def _process_turn_notification(
        *,
        event: Any,
        turn_id: str,
        state: TurnRuntimeState,
        allowed_function_tool_names: set[str],
        allowed_mcp_server_labels: set[str],
        on_delta: Callable[[str], None] | None,
        on_reasoning_delta: Callable[[str], None] | None,
        reasoning_effort: str | None,
        reasoning_summary: str | None,
    ) -> bool:
        return process_turn_notification(
            event=event,
            turn_id=turn_id,
            state=state,
            allowed_function_tool_names=allowed_function_tool_names,
            allowed_mcp_server_labels=allowed_mcp_server_labels,
            on_delta=on_delta,
            on_reasoning_delta=on_reasoning_delta,
            reasoning_effort=reasoning_effort,
            reasoning_summary=reasoning_summary,
        )

    def _run_turn(
        self,
        *,
        input_items: list[dict[str, Any]],
        instructions: str | None,
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
        state = TurnRuntimeState()
        tool_calls: list[ToolCallRecord] = []
        seen_call_ids: set[str] = set()
        allowed_function_tool_names = extract_allowed_function_tool_names(function_tools)
        allowed_mcp_server_labels = extract_allowed_mcp_server_labels(request_config_overrides)

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
        if allowed_mcp_server_labels:
            debug_log(
                "run_turn.mcp_servers",
                ", ".join(sorted(allowed_mcp_server_labels)),
            )

        def approval_handler(method: str, params: dict[str, Any] | None) -> dict[str, Any]:
            return handle_server_request(
                method=method,
                params=params,
                state=state,
                allowed_function_tool_names=allowed_function_tool_names,
                allowed_mcp_server_labels=allowed_mcp_server_labels,
                tool_calls=tool_calls,
                seen_call_ids=seen_call_ids,
                provided_tool_outputs=provided_tool_outputs,
            )

        config = self._app_server_config(api_key)
        client = AppServerClient(config=config, approval_handler=approval_handler)

        try:
            client.start()
            client.initialize()

            thread_session_params = build_thread_session_params(
                cfg=self._cfg,
                model=model,
                instructions=instructions,
                request_cwd=request_cwd,
                request_config_overrides=request_config_overrides,
                function_tools=function_tools,
            )
            turn_start_params = build_turn_start_params(
                request_cwd=request_cwd,
                model=model,
                reasoning_effort=reasoning_effort,
                reasoning_summary=reasoning_summary,
            )
            resume_fingerprint = build_resume_fingerprint(
                thread_session_params,
                turn_start_params,
            )
            thread_id = resolve_thread_id(
                client=client,
                store=self._store,
                previous_response_id=previous_response_id,
                thread_session_params=thread_session_params,
                expected_resume_fingerprint=resume_fingerprint,
            )

            turn_started = client.turn_start(
                thread_id,
                input_items,
                params=turn_start_params,
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

            drive_turn_notifications(
                client=client,
                thread_id=thread_id,
                turn_id=turn_id,
                state=state,
                allowed_function_tool_names=allowed_function_tool_names,
                allowed_mcp_server_labels=allowed_mcp_server_labels,
                on_delta=on_delta,
                on_reasoning_delta=on_reasoning_delta,
                reasoning_effort=reasoning_effort,
                reasoning_summary=reasoning_summary,
                process_notification=self._process_turn_notification,
            )
        finally:
            client.close()

        debug_log(
            "run_turn.done",
            f"status={state.status}",
            f"tool_calls={len(tool_calls)}",
            f"output_chars={len(state.output_text)}",
            f"reasoning_chars={len(state.reasoning_text)}",
        )

        return TurnResult(
            thread_id=thread_id,
            turn_id=turn_id,
            instructions=instructions,
            resume_fingerprint=resume_fingerprint,
            output_text=state.output_text,
            reasoning_text=state.reasoning_text,
            status=state.status,
            usage=state.usage,
            error=state.error,
            tool_calls=tool_calls,
        )

    def list_models(self, api_key: str | None) -> dict[str, Any]:
        config = self._app_server_config(api_key)
        client = AppServerClient(config=config, approval_handler=deny_approval)
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
        context = parse_create_response_context(
            payload,
            provided_tool_outputs=provided_tool_outputs,
        )

        result = run_create_response_turn(
            bridge=self,
            context=context,
            api_key=api_key,
            request_cwd=request_cwd,
            request_config_overrides=request_config_overrides,
            function_tools=function_tools,
            provided_tool_outputs=provided_tool_outputs,
            on_delta=on_delta,
        )

        response_id, body = finalize_create_response(
            store=self._store,
            result=result,
            model_name=context.model_name,
            previous_response_id=context.previous_response_id,
            response_tools=context.response_tools,
            provided_tool_outputs=provided_tool_outputs,
            created_at=int(time.time()),
            response_id_factory=lambda: make_id("resp"),
            message_id_factory=lambda: make_id("msg"),
            function_item_id_factory=lambda: make_id("fc"),
        )
        return response_id, body
