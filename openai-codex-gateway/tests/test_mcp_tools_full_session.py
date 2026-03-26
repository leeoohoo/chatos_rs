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


def build_tool(fixture_server: str, secret: str) -> dict[str, Any]:
    return {
        "type": "mcp",
        "server_label": "secret_mcp",
        "command": sys.executable,
        "args": [fixture_server],
        "env": {
            "MCP_TEST_SECRET": secret,
        },
    }


def main() -> None:
    base_url = os.environ.get("GATEWAY_BASE_URL", "http://127.0.0.1:8089/v1")
    api_key = os.environ.get("GATEWAY_API_KEY", "dummy-key")
    model = os.environ.get("GATEWAY_TEST_MODEL")
    fixture_server = (
        Path(__file__).resolve().parent / "fixtures" / "mcp_secret_server.py"
    ).as_posix()

    if not os.path.exists(fixture_server):
        raise RuntimeError(f"missing fixture MCP server: {fixture_server}")

    client = OpenAI(base_url=base_url, api_key=api_key)

    secret_1 = f"mcp-secret-turn1-{uuid.uuid4().hex}"
    payload_1: dict[str, Any] = {
        "input": (
            "请调用 MCP 工具 mcp__secret_mcp__get_secret 获取密钥，"
            "并且只输出密钥原文，不要附加其他内容。"
        ),
        "tools": [build_tool(fixture_server, secret_1)],
    }
    if model:
        payload_1["model"] = model

    response_1 = client.responses.create(**payload_1)
    text_1 = (response_1.output_text or "").strip()
    if not text_1:
        raise RuntimeError("turn1 failed: empty output text")
    if secret_1 not in text_1:
        raise RuntimeError(
            "turn1 failed: expected secret not found in output. "
            f"expected={secret_1!r}, got={text_1!r}"
        )

    secret_2 = f"mcp-secret-turn2-{uuid.uuid4().hex}"
    payload_2: dict[str, Any] = {
        "input": (
            "继续会话。再次调用 MCP 工具 mcp__secret_mcp__get_secret，"
            "并且只输出工具返回的密钥。"
        ),
        "previous_response_id": response_1.id,
        "tools": [build_tool(fixture_server, secret_2)],
    }
    if model:
        payload_2["model"] = model

    response_2 = client.responses.create(**payload_2)
    text_2 = (response_2.output_text or "").strip()
    if not text_2:
        raise RuntimeError("turn2 failed: empty output text")
    if secret_2 not in text_2:
        raise RuntimeError(
            "turn2 failed: expected secret not found in output. "
            f"expected={secret_2!r}, got={text_2!r}"
        )

    print("[mcp-tools-full-session] ok")
    print(f"turn1_response_id: {response_1.id}")
    print(f"turn1_output_text: {text_1}")
    print(f"turn2_response_id: {response_2.id}")
    print(f"turn2_output_text: {text_2}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[mcp-tools-full-session] failed: {exc}", file=sys.stderr)
        raise
