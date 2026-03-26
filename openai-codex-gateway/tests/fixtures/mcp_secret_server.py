#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
from typing import Any

PROTOCOL_VERSION = "2025-06-18"


def write_message(message: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def write_result(request_id: Any, result: dict[str, Any]) -> None:
    write_message(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result,
        }
    )


def write_error(request_id: Any, code: int, message: str) -> None:
    write_message(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {
                "code": code,
                "message": message,
            },
        }
    )


def tools_list_result() -> dict[str, Any]:
    return {
        "tools": [
            {
                "name": "get_secret",
                "description": "Return secret token from MCP_TEST_SECRET.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": False,
                },
            }
        ]
    }


def tool_call_result(params: dict[str, Any]) -> dict[str, Any]:
    tool_name = params.get("name")
    if tool_name != "get_secret":
        return {
            "content": [{"type": "text", "text": f"unknown tool: {tool_name}"}],
            "isError": True,
        }

    secret = os.environ.get("MCP_TEST_SECRET", "")
    if not secret:
        secret = "MCP_TEST_SECRET_NOT_SET"

    return {
        "content": [{"type": "text", "text": secret}],
        "structuredContent": {"secret": secret},
        "isError": False,
    }


def main() -> None:
    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue

        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue

        if not isinstance(message, dict):
            continue

        method = message.get("method")
        request_id = message.get("id")
        params = message.get("params")
        request_params = params if isinstance(params, dict) else {}

        if not isinstance(method, str):
            continue

        # Notifications have no id; ignore.
        if request_id is None:
            continue

        if method == "initialize":
            write_result(
                request_id,
                {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {
                            "listChanged": False,
                        }
                    },
                    "serverInfo": {
                        "name": "gateway-test-mcp",
                        "title": "Gateway Test MCP",
                        "version": "0.1.0",
                    },
                },
            )
            continue

        if method == "tools/list":
            write_result(request_id, tools_list_result())
            continue

        if method == "tools/call":
            write_result(request_id, tool_call_result(request_params))
            continue

        if method == "resources/list":
            write_result(request_id, {"resources": []})
            continue

        if method == "resources/templates/list":
            write_result(request_id, {"resourceTemplates": []})
            continue

        if method == "ping":
            write_result(request_id, {})
            continue

        write_error(request_id, -32601, f"method not found: {method}")


if __name__ == "__main__":
    main()
