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

    alpha_secret = f"alpha-{uuid.uuid4().hex}"
    beta_secret = f"beta-{uuid.uuid4().hex}"

    tools = [
        {
            "type": "function",
            "name": "read_alpha_secret",
            "description": "Read alpha secret from local app method.",
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": False,
            },
        },
        {
            "type": "function",
            "name": "read_beta_secret",
            "description": "Read beta secret from local app method.",
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": False,
            },
        },
    ]

    response_payload: dict[str, Any] = {
        "input": (
            "必须通过工具读取两个秘密值。请调用 read_alpha_secret 和 read_beta_secret，"
            "然后只输出一行 JSON："
            '{"alpha":"","beta":""}'
            "。不要猜测，不要输出解释。"
        ),
        "tools": tools,
    }
    if model:
        response_payload["model"] = model

    max_rounds = 8
    response = None
    tool_rounds = 0
    total_tool_calls = 0
    max_calls_in_one_round = 0
    call_count_by_name: dict[str, int] = {}

    for _ in range(max_rounds):
        response = client.responses.create(**response_payload)
        calls = extract_function_calls(response)
        if not calls:
            break

        tool_rounds += 1
        total_tool_calls += len(calls)
        max_calls_in_one_round = max(max_calls_in_one_round, len(calls))

        outputs: list[dict[str, Any]] = []
        for call in calls:
            name = call["name"]
            call_count_by_name[name] = call_count_by_name.get(name, 0) + 1
            _ = parse_args(call["arguments"])

            if name == "read_alpha_secret":
                output_payload = {"alpha": alpha_secret}
            elif name == "read_beta_secret":
                output_payload = {"beta": beta_secret}
            else:
                raise RuntimeError(f"unexpected function name: {name}")

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
        raise RuntimeError("function-tools-multi failed: exceeded max rounds without final text")

    if response is None:
        raise RuntimeError("function-tools-multi failed: empty response")
    if total_tool_calls < 2:
        raise RuntimeError(
            f"function-tools-multi failed: expected at least 2 tool calls, got {total_tool_calls}"
        )

    output_text = (response.output_text or "").strip()
    if not output_text:
        raise RuntimeError("function-tools-multi failed: final output_text is empty")
    if alpha_secret not in output_text:
        raise RuntimeError(
            "function-tools-multi failed: final output missing alpha secret. "
            f"expected substring={alpha_secret!r}, got={output_text!r}"
        )
    if beta_secret not in output_text:
        raise RuntimeError(
            "function-tools-multi failed: final output missing beta secret. "
            f"expected substring={beta_secret!r}, got={output_text!r}"
        )

    print("[function-tools-multi] ok")
    print(f"final_response_id: {response.id}")
    print(f"output_text: {output_text}")
    print(f"tool_rounds: {tool_rounds}")
    print(f"total_tool_calls: {total_tool_calls}")
    print(f"max_calls_in_one_round: {max_calls_in_one_round}")
    print(
        "call_count_by_name: "
        + ", ".join(f"{name}={count}" for name, count in sorted(call_count_by_name.items()))
    )

    if max_calls_in_one_round < 2:
        print(
            "warning: model did not issue multi-tool calls in a single response this run; "
            "client loop still handled all returned calls correctly."
        )


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[function-tools-multi] failed: {exc}", file=sys.stderr)
        raise
