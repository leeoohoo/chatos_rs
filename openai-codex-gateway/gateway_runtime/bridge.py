from __future__ import annotations

import time
from typing import Any, Callable

from gateway_base.logging import debug_log, reasoning_log, state_log
from gateway_base.policy import (
    deny_approval,
    extract_allowed_function_tool_names,
    extract_allowed_mcp_server_labels,
    gateway_developer_instructions,
    is_allowed_tool_call_name,
)
from gateway_base.types import GatewayConfig, ToolCallRecord, TurnResult
from gateway_base.utils import error_payload, make_id, to_json_compatible
from gateway_runtime.sdk_types import (
    AgentMessageDeltaNotification,
    AgentMessageThreadItem,
    AppServerClient,
    AppServerConfig,
    ItemCompletedNotification,
    ItemStartedNotification,
    ModelListResponse,
    ReasoningSummaryTextDeltaNotification,
    ReasoningTextDeltaNotification,
    ReasoningThreadItem,
    ThreadTokenUsageUpdatedNotification,
    TurnCompletedNotification,
)
from gateway_runtime.tool_guard import describe_disallowed_thread_item
from gateway_runtime.turn_state import TurnRuntimeState
from create_response.completion import finalize_create_response
from create_response.parser import parse_create_response_context
from create_response.turn_runner import run_create_response_turn


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
        event_method = getattr(event, "method", "unknown")
        payload = event.payload

        if (
            isinstance(payload, (ItemStartedNotification, ItemCompletedNotification))
            and payload.turn_id == turn_id
        ):
            item = payload.item.root
            tool_violation = describe_disallowed_thread_item(
                item,
                allowed_function_tool_names=allowed_function_tool_names,
                allowed_mcp_server_labels=allowed_mcp_server_labels,
            )
            if tool_violation and state.disallowed_tool_error is None:
                state.disallowed_tool_error = tool_violation
                state_log(
                    "run_turn.disallowed_thread_item",
                    f"method={event_method}",
                    f"type={getattr(item, 'type', 'unknown')}",
                    f"detail={tool_violation}",
                )
            if tool_violation:
                return False

        if isinstance(payload, AgentMessageDeltaNotification) and payload.turn_id == turn_id:
            state.output_text += payload.delta
            if on_delta:
                on_delta(payload.delta)
            return False

        if isinstance(payload, ReasoningTextDeltaNotification) and payload.turn_id == turn_id:
            state.reasoning_event_count += 1
            reasoning_log(
                "sdk.event",
                f"method={event_method}",
                "type=reasoning_text_delta",
                f"turn_id={payload.turn_id}",
                f"chars={len(payload.delta)}",
            )
            state.reasoning_text += payload.delta
            if on_reasoning_delta:
                on_reasoning_delta(payload.delta)
            return False

        if (
            isinstance(payload, ReasoningSummaryTextDeltaNotification)
            and payload.turn_id == turn_id
        ):
            state.reasoning_event_count += 1
            reasoning_log(
                "sdk.event",
                f"method={event_method}",
                "type=reasoning_summary_delta",
                f"turn_id={payload.turn_id}",
                f"chars={len(payload.delta)}",
            )
            state.reasoning_text += payload.delta
            if on_reasoning_delta:
                on_reasoning_delta(payload.delta)
            return False

        if isinstance(payload, ItemCompletedNotification) and payload.turn_id == turn_id:
            item = payload.item.root
            if isinstance(item, AgentMessageThreadItem) and item.text:
                state.output_text = item.text
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
                    f"used_fallback={'yes' if not state.reasoning_text and bool(fallback_text) else 'no'}",
                )
                if not state.reasoning_text:
                    state.reasoning_text = fallback_text
                    if state.reasoning_text and on_reasoning_delta:
                        on_reasoning_delta(state.reasoning_text)
            return False

        if (
            isinstance(payload, ThreadTokenUsageUpdatedNotification)
            and payload.turn_id == turn_id
        ):
            state.reasoning_tokens = payload.token_usage.last.reasoning_output_tokens
            state.usage = {
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
                f"reasoning_tokens={state.reasoning_tokens}",
            )
            return False

        if isinstance(payload, TurnCompletedNotification) and payload.turn.id == turn_id:
            state.status = payload.turn.status.value
            if state.disallowed_tool_error:
                state.status = "failed"
            reasoning_log(
                "turn.completed",
                f"turn_id={turn_id}",
                f"status={state.status}",
                f"reasoning_chars={len(state.reasoning_text)}",
                f"reasoning_tokens={state.reasoning_tokens}",
                f"reasoning_events={state.reasoning_event_count}",
            )
            if (reasoning_effort or reasoning_summary) and not state.reasoning_text:
                reasoning_log(
                    "turn.reasoning_missing",
                    f"turn_id={turn_id}",
                    f"reasoning_tokens={state.reasoning_tokens}",
                    f"reasoning_requested_effort={reasoning_effort or 'none'}",
                    f"reasoning_requested_summary={reasoning_summary or 'none'}",
                )
            if payload.turn.error is not None:
                state.error = {
                    "message": payload.turn.error.message,
                    "codex_error_info": to_json_compatible(payload.turn.error.codex_error_info),
                }
            if state.disallowed_tool_error:
                state.error = {
                    "message": state.disallowed_tool_error,
                    "codex_error_info": {
                        "gateway_error": "disallowed_tool_use",
                    },
                }
            return True

        return False

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

        def handle_server_request(method: str, params: dict[str, Any] | None) -> dict[str, Any]:
            payload = params or {}
            if method in {"item/commandExecution/requestApproval", "item/fileChange/requestApproval"}:
                state_log("run_turn.builtin_request_declined", f"method={method}")
                return {"decision": "decline"}

            if method == "item/permissions/requestApproval":
                if state.disallowed_tool_error is None:
                    state.disallowed_tool_error = "Codex 内置 request_permissions 工具已被 gateway 禁用"
                state_log("run_turn.builtin_request_declined", f"method={method}")
                return {"permissions": {}}

            if method == "mcpServer/elicitation/request":
                server_name_raw = payload.get("serverName")
                server_name = (
                    server_name_raw.strip()
                    if isinstance(server_name_raw, str)
                    else ""
                )
                if server_name in allowed_mcp_server_labels:
                    state_log(
                        "run_turn.mcp_elicitation_accepted",
                        f"server={server_name}",
                    )
                    return {
                        "action": "accept",
                        "content": {},
                    }
                if state.disallowed_tool_error is None:
                    state.disallowed_tool_error = (
                        "Codex 尝试为未声明的 MCP 服务申请调用权限："
                        f"{server_name or 'unknown'}"
                    )
                state_log(
                    "run_turn.mcp_elicitation_declined",
                    f"server={server_name or 'unknown'}",
                )
                return {
                    "action": "decline",
                    "content": None,
                }

            if method != "item/tool/call":
                return {}

            call_id_raw = payload.get("callId")
            tool_name_raw = payload.get("tool")
            arguments = payload.get("arguments")

            call_id = call_id_raw if isinstance(call_id_raw, str) and call_id_raw else make_id("call")
            tool_name = tool_name_raw if isinstance(tool_name_raw, str) and tool_name_raw else "unknown_tool"

            if not is_allowed_tool_call_name(
                tool_name,
                allowed_function_tool_names=allowed_function_tool_names,
                allowed_mcp_server_labels=allowed_mcp_server_labels,
            ):
                if state.disallowed_tool_error is None:
                    state.disallowed_tool_error = (
                        "Codex 尝试调用未在本次请求中声明的动态工具："
                        f"{tool_name}"
                    )
                state_log(
                    "run_turn.disallowed_dynamic_tool",
                    f"name={tool_name}",
                    f"call_id={call_id}",
                )
                return {
                    "contentItems": [
                        {
                            "type": "inputText",
                            "text": "DISALLOWED_TOOL_CALL",
                        }
                    ],
                    "success": False,
                }

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

            state.missing_tool_output_detected = True
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
                if (state.missing_tool_output_detected or state.disallowed_tool_error) and not state.interrupt_sent:
                    try:
                        client.turn_interrupt(thread_id, turn_id)
                    except Exception:
                        pass
                    state.interrupt_sent = True

                if self._process_turn_notification(
                    event=event,
                    turn_id=turn_id,
                    state=state,
                    allowed_function_tool_names=allowed_function_tool_names,
                    allowed_mcp_server_labels=allowed_mcp_server_labels,
                    on_delta=on_delta,
                    on_reasoning_delta=on_reasoning_delta,
                    reasoning_effort=reasoning_effort,
                    reasoning_summary=reasoning_summary,
                ):
                    break
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
