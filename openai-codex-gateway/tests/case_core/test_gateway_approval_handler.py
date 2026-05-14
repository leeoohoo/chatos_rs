#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_base.types import ToolCallRecord  # noqa: E402
from gateway_runtime.approval_handler import handle_server_request  # noqa: E402
from gateway_runtime.turn_state import TurnRuntimeState  # noqa: E402


class GatewayApprovalHandlerTest(unittest.TestCase):
    def test_declines_builtin_approval_requests(self) -> None:
        state = TurnRuntimeState()
        response = handle_server_request(
            method="item/commandExecution/requestApproval",
            params=None,
            state=state,
            allowed_function_tool_names=set(),
            allowed_mcp_server_labels=set(),
            tool_calls=[],
            seen_call_ids=set(),
            provided_tool_outputs={},
        )

        self.assertEqual(response, {"decision": "decline"})
        self.assertIsNone(state.disallowed_tool_error)

    def test_rejects_request_permissions_with_runtime_error(self) -> None:
        state = TurnRuntimeState()
        response = handle_server_request(
            method="item/permissions/requestApproval",
            params={},
            state=state,
            allowed_function_tool_names=set(),
            allowed_mcp_server_labels=set(),
            tool_calls=[],
            seen_call_ids=set(),
            provided_tool_outputs={},
        )

        self.assertEqual(response, {"permissions": {}})
        self.assertIn("request_permissions", state.disallowed_tool_error or "")

    def test_accepts_declared_mcp_elicitation(self) -> None:
        state = TurnRuntimeState()
        response = handle_server_request(
            method="mcpServer/elicitation/request",
            params={"serverName": "workspace"},
            state=state,
            allowed_function_tool_names=set(),
            allowed_mcp_server_labels={"workspace"},
            tool_calls=[],
            seen_call_ids=set(),
            provided_tool_outputs={},
        )

        self.assertEqual(response, {"action": "accept", "content": {}})
        self.assertIsNone(state.disallowed_tool_error)

    def test_rejects_undeclared_tool_call(self) -> None:
        state = TurnRuntimeState()
        tool_calls: list[ToolCallRecord] = []
        response = handle_server_request(
            method="item/tool/call",
            params={"callId": "call_1", "tool": "shell_exec", "arguments": {"cmd": "pwd"}},
            state=state,
            allowed_function_tool_names={"memory_reader_read_file"},
            allowed_mcp_server_labels={"workspace"},
            tool_calls=tool_calls,
            seen_call_ids=set(),
            provided_tool_outputs={},
        )

        self.assertFalse(response["success"])
        self.assertEqual(response["contentItems"][0]["text"], "DISALLOWED_TOOL_CALL")
        self.assertIn("未在本次请求中声明的动态工具", state.disallowed_tool_error or "")
        self.assertEqual(tool_calls, [])

    def test_returns_provided_tool_output_and_records_call_once(self) -> None:
        state = TurnRuntimeState()
        tool_calls: list[ToolCallRecord] = []
        seen_call_ids: set[str] = set()
        params = {"callId": "call_1", "tool": "get_weather", "arguments": {"city": "Shanghai"}}
        provided_tool_outputs = {
            "call_1": [{"type": "inputText", "text": "sunny"}],
        }

        first = handle_server_request(
            method="item/tool/call",
            params=params,
            state=state,
            allowed_function_tool_names={"get_weather"},
            allowed_mcp_server_labels=set(),
            tool_calls=tool_calls,
            seen_call_ids=seen_call_ids,
            provided_tool_outputs=provided_tool_outputs,
        )
        second = handle_server_request(
            method="item/tool/call",
            params=params,
            state=state,
            allowed_function_tool_names={"get_weather"},
            allowed_mcp_server_labels=set(),
            tool_calls=tool_calls,
            seen_call_ids=seen_call_ids,
            provided_tool_outputs=provided_tool_outputs,
        )

        self.assertTrue(first["success"])
        self.assertEqual(first["contentItems"][0]["text"], "sunny")
        self.assertEqual(second["contentItems"][0]["text"], "sunny")
        self.assertEqual(len(tool_calls), 1)
        self.assertEqual(tool_calls[0].call_id, "call_1")
        self.assertEqual(tool_calls[0].name, "get_weather")

    def test_marks_missing_tool_output_as_deferred(self) -> None:
        state = TurnRuntimeState()
        tool_calls: list[ToolCallRecord] = []
        response = handle_server_request(
            method="item/tool/call",
            params={"callId": "call_2", "tool": "get_weather", "arguments": {}},
            state=state,
            allowed_function_tool_names={"get_weather"},
            allowed_mcp_server_labels=set(),
            tool_calls=tool_calls,
            seen_call_ids=set(),
            provided_tool_outputs={},
        )

        self.assertTrue(response["success"])
        self.assertEqual(
            response["contentItems"][0]["text"],
            "TOOL_OUTPUT_DEFERRED call_id=call_2",
        )
        self.assertTrue(state.missing_tool_output_detected)
        self.assertEqual(len(tool_calls), 1)


if __name__ == "__main__":
    unittest.main()
