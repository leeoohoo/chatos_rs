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
    secret = f"mcp-secret-{uuid.uuid4().hex}"
    fixture_server = (
        Path(__file__).resolve().parents[1] / "fixtures" / "mcp_secret_server.py"
    ).as_posix()

    if not os.path.exists(fixture_server):
        raise RuntimeError(f"missing fixture MCP server: {fixture_server}")

    client = OpenAI(base_url=base_url, api_key=api_key)

    payload: dict[str, Any] = {
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
    if model:
        payload["model"] = model

    response = client.responses.create(**payload)
    output_text = (response.output_text or "").strip()

    if not output_text:
        raise RuntimeError("mcp-tools test failed: empty output text")

    if secret not in output_text:
        raise RuntimeError(
            "mcp-tools test failed: response does not contain tool-returned secret. "
            f"expected substring={secret!r}, got={output_text!r}"
        )

    print("[mcp-tools] ok")
    print(f"response_id: {response.id}")
    print(f"output_text: {output_text}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[mcp-tools] failed: {exc}", file=sys.stderr)
        raise
