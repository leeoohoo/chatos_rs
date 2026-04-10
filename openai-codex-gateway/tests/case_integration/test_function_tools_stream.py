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


def resolve_model(client: OpenAI, model: str | None) -> str:
    if isinstance(model, str) and model.strip():
        return model.strip()

    models = client.models.list()
    data = getattr(models, "data", None)
    if not data:
        raise RuntimeError(
            "function-tools-stream test failed: no models available from gateway. "
            "Please set GATEWAY_TEST_MODEL explicitly."
        )

    first = data[0]
    model_id = getattr(first, "id", None)
    if not isinstance(model_id, str) or not model_id.strip():
        raise RuntimeError(
            "function-tools-stream test failed: invalid model id in models.list() response. "
            "Please set GATEWAY_TEST_MODEL explicitly."
        )
    return model_id.strip()


def main() -> None:
    base_url = os.environ.get("GATEWAY_BASE_URL", "http://127.0.0.1:8089/v1")
    api_key = os.environ.get("GATEWAY_API_KEY", "dummy-key")
    model = os.environ.get("GATEWAY_TEST_MODEL")
    client = OpenAI(base_url=base_url, api_key=api_key)
    resolved_model = resolve_model(client, model)

    alpha_secret = f"alpha-stream-{uuid.uuid4().hex}"
    beta_secret = f"beta-stream-{uuid.uuid4().hex}"

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

    first_payload: dict[str, Any] = {
        "model": resolved_model,
        "input": (
            "必须通过工具读取两个秘密值。请调用 read_alpha_secret 和 read_beta_secret，"
            "先不要直接回答最终内容。"
        ),
        "tools": tools,
    }

    first_event_types: list[str] = []
    with client.responses.stream(**first_payload) as stream:
        for event in stream:
            event_type = getattr(event, "type", "")
            if isinstance(event_type, str) and event_type:
                first_event_types.append(event_type)
        first_response = stream.get_final_response()

    first_calls = extract_function_calls(first_response)
    if len(first_calls) < 2:
        raise RuntimeError(
            f"function-tools-stream test failed: expected >=2 function_call items, got {len(first_calls)}"
        )

    call_names = [call["name"] for call in first_calls]
    if "read_alpha_secret" not in call_names:
        raise RuntimeError("function-tools-stream test failed: missing read_alpha_secret call")
    if "read_beta_secret" not in call_names:
        raise RuntimeError("function-tools-stream test failed: missing read_beta_secret call")

    outputs: list[dict[str, Any]] = []
    for call in first_calls:
        if call["name"] == "read_alpha_secret":
            output_payload = {"alpha": alpha_secret}
        elif call["name"] == "read_beta_secret":
            output_payload = {"beta": beta_secret}
        else:
            continue
        outputs.append(
            {
                "type": "function_call_output",
                "call_id": call["call_id"],
                "output": json.dumps(output_payload, ensure_ascii=False),
            }
        )

    if len(outputs) < 2:
        raise RuntimeError(
            "function-tools-stream test failed: insufficient function_call_output items built"
        )

    second_payload: dict[str, Any] = {
        "model": resolved_model,
        "previous_response_id": first_response.id,
        "input": [
            *outputs,
            {
                "type": "input_text",
                "text": "现在请直接输出 alpha 和 beta 的原文值，不要解释。",
            },
        ],
        "tools": tools,
    }

    second_event_types: list[str] = []
    delta_chunks: list[str] = []
    with client.responses.stream(**second_payload) as stream:
        for event in stream:
            event_type = getattr(event, "type", "")
            if isinstance(event_type, str) and event_type:
                second_event_types.append(event_type)
                if event_type == "response.output_text.delta":
                    delta = getattr(event, "delta", "")
                    if isinstance(delta, str):
                        delta_chunks.append(delta)
        second_response = stream.get_final_response()

    output_text = (second_response.output_text or "").strip()
    if not output_text:
        raise RuntimeError("function-tools-stream test failed: final output_text is empty")
    if alpha_secret not in output_text:
        raise RuntimeError(
            "function-tools-stream test failed: final output missing alpha secret. "
            f"expected substring={alpha_secret!r}, got={output_text!r}"
        )
    if beta_secret not in output_text:
        raise RuntimeError(
            "function-tools-stream test failed: final output missing beta secret. "
            f"expected substring={beta_secret!r}, got={output_text!r}"
        )

    if "response.function_call_arguments.done" not in first_event_types:
        raise RuntimeError(
            "function-tools-stream test failed: missing response.function_call_arguments.done in first stream"
        )
    if "response.output_item.done" not in first_event_types:
        raise RuntimeError(
            "function-tools-stream test failed: missing response.output_item.done in first stream"
        )
    if "response.completed" not in second_event_types:
        raise RuntimeError(
            "function-tools-stream test failed: missing response.completed in second stream"
        )

    print("[function-tools-stream] ok")
    print(f"model: {resolved_model}")
    print(f"first_response_id: {first_response.id}")
    print(f"first_function_call_count: {len(first_calls)}")
    print(
        "first_event_types: "
        + ", ".join(first_event_types)
    )
    print(f"second_response_id: {second_response.id}")
    print(f"second_output_text: {output_text}")
    print(f"second_delta_count: {len(delta_chunks)}")
    print(
        "second_event_types: "
        + ", ".join(second_event_types)
    )


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[function-tools-stream] failed: {exc}", file=sys.stderr)
        raise
