#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

try:
    import server as gateway_server  # noqa: E402
except SystemExit as exc:  # pragma: no cover - environment dependency guard
    gateway_server = None
    IMPORT_ERROR = str(exc)
else:
    IMPORT_ERROR = None


@unittest.skipIf(gateway_server is None, f"gateway import unavailable: {IMPORT_ERROR}")
class GatewayToolGuardsTest(unittest.TestCase):
    def test_request_config_overrides_disable_builtin_codex_tools(self) -> None:
        config = gateway_server.extract_request_config_overrides(
            {
                "tools": [
                    {
                        "type": "mcp",
                        "server_label": "alpha",
                        "server_url": "http://127.0.0.1:9000/mcp",
                    }
                ]
            }
        )

        self.assertIsInstance(config, dict)
        assert config is not None
        self.assertEqual(config["tools"]["view_image"], False)
        self.assertEqual(config["tools"]["web_search"]["enabled"], False)
        self.assertEqual(config["web_search"], "disabled")
        self.assertEqual(
            config["mcp_servers"],
            {
                "alpha": {
                    "url": "http://127.0.0.1:9000/mcp",
                }
            },
        )

    def test_disallowed_dynamic_tool_is_rejected(self) -> None:
        item = gateway_server.DynamicToolCallThreadItem(
            arguments={},
            contentItems=[],
            id="dyn_1",
            status="completed",
            success=False,
            tool="shell_exec",
            type="dynamicToolCall",
        )

        message = gateway_server.describe_disallowed_thread_item(
            item,
            allowed_function_tool_names={"memory_reader_read_file"},
            allowed_mcp_server_labels={"workspace"},
        )
        self.assertIn("未在本次请求中声明的动态工具", message or "")

        allowed_message = gateway_server.describe_disallowed_thread_item(
            gateway_server.DynamicToolCallThreadItem(
                arguments={},
                contentItems=[],
                id="dyn_2",
                status="completed",
                success=True,
                tool="memory_reader_read_file",
                type="dynamicToolCall",
            ),
            allowed_function_tool_names={"memory_reader_read_file"},
            allowed_mcp_server_labels={"workspace"},
        )
        self.assertIsNone(allowed_message)

    def test_mcp_prefixed_tool_call_name_is_allowed(self) -> None:
        self.assertTrue(
            gateway_server.is_allowed_tool_call_name(
                "mcp__workspace__grep_code",
                allowed_function_tool_names={"memory_reader_read_file"},
                allowed_mcp_server_labels={"workspace"},
            )
        )
        self.assertFalse(
            gateway_server.is_allowed_tool_call_name(
                "mcp__rogue__grep_code",
                allowed_function_tool_names={"memory_reader_read_file"},
                allowed_mcp_server_labels={"workspace"},
            )
        )

    def test_disallowed_mcp_server_is_rejected(self) -> None:
        item = gateway_server.McpToolCallThreadItem(
            arguments={},
            id="mcp_1",
            server="rogue_server",
            status="completed",
            tool="grep_code",
            type="mcpToolCall",
        )

        message = gateway_server.describe_disallowed_thread_item(
            item,
            allowed_function_tool_names=set(),
            allowed_mcp_server_labels={"workspace"},
        )
        self.assertIn("未在本次请求中声明的 MCP 服务", message or "")

    def test_builtin_command_execution_is_rejected(self) -> None:
        item = gateway_server.CommandExecutionThreadItem(
            aggregatedOutput="",
            command="pwd",
            commandActions=[],
            cwd="/tmp",
            id="cmd_1",
            status="completed",
            type="commandExecution",
        )

        message = gateway_server.describe_disallowed_thread_item(
            item,
            allowed_function_tool_names=set(),
            allowed_mcp_server_labels=set(),
        )
        self.assertIn("shell/commandExecution", message or "")


if __name__ == "__main__":
    unittest.main()
