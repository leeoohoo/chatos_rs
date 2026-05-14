from __future__ import annotations

from typing import Any

from gateway_base.policy import gateway_developer_instructions
from gateway_base.types import GatewayConfig


def build_thread_session_params(
    *,
    cfg: GatewayConfig,
    model: str | None,
    request_cwd: str | None,
    request_config_overrides: dict[str, Any] | None,
    function_tools: list[dict[str, Any]],
) -> dict[str, Any]:
    params: dict[str, Any] = {
        "approvalPolicy": cfg.approval_policy,
        "sandbox": cfg.sandbox,
        "developerInstructions": gateway_developer_instructions(),
        **({"model": model} if model else {}),
        **({"cwd": request_cwd} if request_cwd else {}),
        **({"config": request_config_overrides} if request_config_overrides else {}),
    }
    if function_tools:
        params["dynamicTools"] = function_tools
    return params


def resolve_thread_id(
    *,
    client: Any,
    store: Any,
    previous_response_id: str | None,
    thread_session_params: dict[str, Any],
) -> str:
    if previous_response_id:
        resumed_thread = store.get_thread(previous_response_id)
        if not resumed_thread:
            raise ValueError(f"unknown previous_response_id: {previous_response_id}")
        resumed = client.thread_resume(resumed_thread, thread_session_params)
        return resumed.thread.id

    started = client.thread_start(thread_session_params)
    return started.thread.id


def build_turn_start_params(
    *,
    request_cwd: str | None,
    model: str | None,
    reasoning_effort: str | None,
    reasoning_summary: str | None,
) -> dict[str, Any]:
    return {
        **({"cwd": request_cwd} if request_cwd else {}),
        **({"model": model} if model else {}),
        **({"effort": reasoning_effort} if reasoning_effort else {}),
        **({"summary": reasoning_summary} if reasoning_summary else {}),
    }
