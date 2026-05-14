from __future__ import annotations

import hashlib
import json
from typing import Any

from gateway_base.policy import gateway_developer_instructions
from gateway_base.types import GatewayConfig


def build_thread_session_params(
    *,
    cfg: GatewayConfig,
    model: str | None,
    instructions: str | None,
    request_cwd: str | None,
    request_config_overrides: dict[str, Any] | None,
    function_tools: list[dict[str, Any]],
) -> dict[str, Any]:
    params: dict[str, Any] = {
        "approvalPolicy": cfg.approval_policy,
        "sandbox": cfg.sandbox,
        "developerInstructions": gateway_developer_instructions(),
        **({"baseInstructions": instructions} if instructions else {}),
        **({"model": model} if model else {}),
        **({"cwd": request_cwd} if request_cwd else {}),
        **({"config": request_config_overrides} if request_config_overrides else {}),
    }
    if function_tools:
        params["dynamicTools"] = function_tools
    return params


def instructions_fingerprint(instructions: str | None) -> str:
    normalized = (instructions or "").strip()
    if not normalized:
        return ""
    return hashlib.sha256(normalized.encode("utf-8")).hexdigest()


def build_resume_fingerprint(
    thread_session_params: dict[str, Any],
    turn_start_params: dict[str, Any],
) -> str:
    payload = {
        "thread": thread_session_params,
        "turn": turn_start_params,
    }
    serialized = json.dumps(
        payload,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )
    return hashlib.sha256(serialized.encode("utf-8")).hexdigest()


def resolve_thread_id(
    *,
    client: Any,
    store: Any,
    previous_response_id: str | None,
    thread_session_params: dict[str, Any],
    expected_resume_fingerprint: str,
) -> str:
    expected_instructions_fingerprint = instructions_fingerprint(
        thread_session_params.get("baseInstructions")
        if isinstance(thread_session_params.get("baseInstructions"), str)
        else None
    )
    if previous_response_id:
        binding = (
            store.get_thread_binding(previous_response_id)
            if hasattr(store, "get_thread_binding")
            else None
        )
        if binding is None:
            resumed_thread = store.get_thread(previous_response_id)
            binding = (
                {
                    "thread_id": resumed_thread,
                    "instructions_fingerprint": "",
                }
                if resumed_thread
                else None
            )
        if not binding or not binding.get("thread_id"):
            raise ValueError(f"unknown previous_response_id: {previous_response_id}")
        stored_resume_fingerprint = binding.get("resume_fingerprint", "")
        stored_instructions_fingerprint = binding.get("instructions_fingerprint", "")
        can_resume = (
            stored_resume_fingerprint == expected_resume_fingerprint
            if stored_resume_fingerprint
            else stored_instructions_fingerprint == expected_instructions_fingerprint
        )
        if can_resume:
            resumed = client.thread_resume(binding["thread_id"], thread_session_params)
            return resumed.thread.id

        started = client.thread_start(thread_session_params)
        return started.thread.id

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
