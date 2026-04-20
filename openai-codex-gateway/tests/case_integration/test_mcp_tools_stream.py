#!/usr/bin/env python3
from __future__ import annotations

import os
import sys
import uuid
from pathlib import Path
from typing import Any

try:
    from openai import OpenAI
except ModuleNotFoundError as exc:  # pragma: no cover
    raise SystemExit(
        "Missing dependency: openai\n"
        "Install with: python -m pip install openai"
    ) from exc


def main() -> None:
    base_url = os.environ.get("GATEWAY_BASE_URL", "http://127.0.0.1:8089/v1")
    api_key = os.environ.get("GATEWAY_API_KEY", "dummy-key")
    model = os.environ.get("GATEWAY_TEST_MODEL")
    secret = f"mcp-secret-stream-{uuid.uuid4().hex}"
    fixture_server = (
        Path(__file__).resolve().parents[1] / "fixtures" / "mcp_secret_server.py"
    ).as_posix()

    if not os.path.exists(fixture_server):
        raise RuntimeError(f"missing fixture MCP server: {fixture_server}")

    client = OpenAI(base_url=base_url, api_key=api_key)
    resolved_model = resolve_model(client, model)

    payload: dict[str, Any] = {
        "model": resolved_model,
        "input": (
            "请调用 MCP 工具 mcp__secret_mcp__get_secret 获取密钥，"
            "并且只输出密钥原文，不要附加其他内容。"
        ),
        "tools": [
            {
                "type": "mcp",
                "server_label": "secret_mcp",
                "command": sys.executable,
                "args": [fixture_server],
                "env": {
                    "MCP_TEST_SECRET": secret,
                },
            }
        ],
    }

    event_types: list[str] = []
    event_counts: dict[str, int] = {}
    delta_chunks: list[str] = []
    done_text_from_event = ""
    with client.responses.stream(**payload) as stream:
        for event in stream:
            event_type = getattr(event, "type", "")
            if isinstance(event_type, str) and event_type:
                event_types.append(event_type)
                event_counts[event_type] = event_counts.get(event_type, 0) + 1

                if event_type == "response.output_text.delta":
                    delta = getattr(event, "delta", "")
                    if isinstance(delta, str):
                        delta_chunks.append(delta)
                elif event_type == "response.output_text.done":
                    done_text = getattr(event, "text", "")
                    if isinstance(done_text, str):
                        done_text_from_event = done_text
        final_response = stream.get_final_response()

    raw_output_text = final_response.output_text or ""
    output_text = raw_output_text.strip()
    stream_text_from_delta = "".join(delta_chunks)
    stream_text_from_delta_stripped = stream_text_from_delta.strip()

    if not output_text:
        raise RuntimeError("mcp-tools-stream test failed: empty output text")
    if secret not in output_text:
        raise RuntimeError(
            "mcp-tools-stream test failed: response does not contain tool-returned secret. "
            f"expected substring={secret!r}, got={output_text!r}"
        )
    if not event_types:
        raise RuntimeError("mcp-tools-stream test failed: no stream events received")
    if not delta_chunks:
        raise RuntimeError("mcp-tools-stream test failed: no response.output_text.delta received")
    if stream_text_from_delta_stripped != output_text:
        raise RuntimeError(
            "mcp-tools-stream test failed: parsed delta text mismatch. "
            f"delta_text={stream_text_from_delta!r}, final_output={raw_output_text!r}"
        )
    if done_text_from_event and done_text_from_event.strip() != output_text:
        raise RuntimeError(
            "mcp-tools-stream test failed: response.output_text.done text mismatch. "
            f"done_text={done_text_from_event!r}, final_output={raw_output_text!r}"
        )

    print("[mcp-tools-stream] ok")
    print(f"response_id: {final_response.id}")
    print(f"output_text: {output_text}")
    print(f"parsed_delta_text: {stream_text_from_delta}")
    if done_text_from_event:
        print(f"parsed_done_text: {done_text_from_event}")
    print(f"delta_count: {len(delta_chunks)}")
    print(f"event_count: {len(event_types)}")
    print(
        "event_counts: "
        + ", ".join(f"{name}={count}" for name, count in sorted(event_counts.items()))
    )
    print(f"model: {resolved_model}")


def resolve_model(client: OpenAI, model: str | None) -> str:
    if isinstance(model, str) and model.strip():
        return model.strip()

    models = client.models.list()
    data = getattr(models, "data", None)
    if not data:
        raise RuntimeError(
            "mcp-tools-stream test failed: no models available from gateway. "
            "Please set GATEWAY_TEST_MODEL explicitly."
        )

    first = data[0]
    model_id = getattr(first, "id", None)
    if not isinstance(model_id, str) or not model_id.strip():
        raise RuntimeError(
            "mcp-tools-stream test failed: invalid model id in models.list() response. "
            "Please set GATEWAY_TEST_MODEL explicitly."
        )
    return model_id.strip()


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[mcp-tools-stream] failed: {exc}", file=sys.stderr)
        raise
