#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
import uuid
from typing import Any

try:
    from openai import OpenAI
except ModuleNotFoundError as exc:  # pragma: no cover
    raise SystemExit(
        "Missing dependency: openai\n"
        "Install with: python -m pip install openai"
    ) from exc


def item_value(item: Any, key: str, default: Any = None) -> Any:
    if isinstance(item, dict):
        return item.get(key, default)
    return getattr(item, key, default)


def extract_function_calls(response: Any) -> list[dict[str, str]]:
    out: list[dict[str, str]] = []
    items = item_value(response, "output", [])
    if not isinstance(items, list):
        return out

    for item in items:
        if item_value(item, "type") != "function_call":
            continue
        call_id = item_value(item, "call_id")
        name = item_value(item, "name")
        arguments = item_value(item, "arguments", "")
        if isinstance(call_id, str) and isinstance(name, str):
            out.append(
                {
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments if isinstance(arguments, str) else "",
                }
            )
    return out


def parse_args(arguments: str) -> dict[str, Any]:
    raw = arguments.strip()
    if not raw:
        return {}
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"invalid tool arguments json: {arguments!r}") from exc
    if not isinstance(value, dict):
        raise RuntimeError(f"tool arguments must be a JSON object, got: {value!r}")
    return value


def main() -> None:
    base_url = os.environ.get("GATEWAY_BASE_URL", "http://127.0.0.1:8089/v1")
    api_key = os.environ.get("GATEWAY_API_KEY", "dummy-key")
    model = os.environ.get("GATEWAY_TEST_MODEL")
    client = OpenAI(base_url=base_url, api_key=api_key)

    secret = f"local-tool-secret-{uuid.uuid4().hex}"
    nonce = f"nonce-{uuid.uuid4().hex[:8]}"

    tools = [
        {
            "type": "function",
            "name": "read_runtime_secret",
            "description": "Read a runtime secret from local application memory.",
            "parameters": {
                "type": "object",
                "properties": {
                    "nonce": {"type": "string"},
                },
                "required": ["nonce"],
                "additionalProperties": False,
            },
        }
    ]

    response_payload: dict[str, Any] = {
        "input": (
            "你必须调用函数 read_runtime_secret，参数 nonce 为 "
            f"{json.dumps(nonce, ensure_ascii=False)}。"
            "拿到工具结果后，只输出 secret 的原文，不要附加其他内容。"
        ),
        "tools": tools,
    }
    if model:
        response_payload["model"] = model

    max_rounds = 6
    response = None
    tool_rounds = 0
    tool_call_count = 0

    for _ in range(max_rounds):
        response = client.responses.create(**response_payload)
        calls = extract_function_calls(response)
        if not calls:
            break

        tool_rounds += 1
        tool_call_count += len(calls)
        outputs: list[dict[str, Any]] = []
        for call in calls:
            if call["name"] != "read_runtime_secret":
                raise RuntimeError(f"unexpected function name: {call['name']}")
            args = parse_args(call["arguments"])
            call_nonce = args.get("nonce")
            if not isinstance(call_nonce, str):
                raise RuntimeError(f"`nonce` must be string, got {args!r}")
            output_payload = {
                "secret": secret,
                "nonce": call_nonce,
            }
            outputs.append(
                {
                    "type": "function_call_output",
                    "call_id": call["call_id"],
                    "output": json.dumps(output_payload, ensure_ascii=False),
                }
            )

        response_payload = {
            "input": outputs,
            "previous_response_id": response.id,
            "tools": tools,
        }
        if model:
            response_payload["model"] = model
    else:
        raise RuntimeError("function-tools-single failed: exceeded max rounds without final text")

    if response is None:
        raise RuntimeError("function-tools-single failed: empty response")

    output_text = (response.output_text or "").strip()
    if not output_text:
        raise RuntimeError("function-tools-single failed: final output_text is empty")
    if secret not in output_text:
        raise RuntimeError(
            "function-tools-single failed: final output does not include secret. "
            f"expected substring={secret!r}, got={output_text!r}"
        )

    print("[function-tools-single] ok")
    print(f"final_response_id: {response.id}")
    print(f"output_text: {output_text}")
    print(f"tool_rounds: {tool_rounds}")
    print(f"tool_call_count: {tool_call_count}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[function-tools-single] failed: {exc}", file=sys.stderr)
        raise
