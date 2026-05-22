#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_base.policy import gateway_developer_instructions  # noqa: E402
from gateway_base.types import GatewayConfig  # noqa: E402
from gateway_runtime.thread_session import (  # noqa: E402
    build_resume_fingerprint,
    build_thread_session_params,
    build_turn_start_params,
    resolve_thread_id,
)


class FakeClient:
    def __init__(self) -> None:
        self.start_calls: list[dict[str, object]] = []

    def thread_start(self, params: dict[str, object]) -> SimpleNamespace:
        self.start_calls.append(params)
        return SimpleNamespace(thread=SimpleNamespace(id="thread_started"))


class FakeStore:
    def __init__(self, mapping: dict[str, str] | None = None) -> None:
        self.mapping = mapping or {}
        self.lookups: list[str] = []

    def get_thread(self, response_id: str) -> str | None:
        self.lookups.append(response_id)
        return self.mapping.get(response_id)


class GatewayThreadSessionTest(unittest.TestCase):
    def setUp(self) -> None:
        self.cfg = GatewayConfig(
            host="127.0.0.1",
            port=8091,
            codex_bin=None,
            cwd="/workspace",
            sandbox="workspace-write",
            approval_policy="never",
            state_db_path="/tmp/gateway.sqlite3",
        )

    def test_build_thread_session_params_includes_optional_fields(self) -> None:
        function_tools = [{"name": "weather"}]
        params = build_thread_session_params(
            cfg=self.cfg,
            model="codex-1",
            instructions="请总结",
            request_cwd="/tmp/demo",
            request_config_overrides={"mcp_servers": {"workspace": {"url": "http://127.0.0.1"}}},
            function_tools=function_tools,
        )

        self.assertEqual(params["approvalPolicy"], "never")
        self.assertEqual(params["sandbox"], "workspace-write")
        self.assertEqual(params["developerInstructions"], gateway_developer_instructions())
        self.assertEqual(params["baseInstructions"], "请总结")
        self.assertEqual(params["model"], "codex-1")
        self.assertEqual(params["cwd"], "/tmp/demo")
        self.assertEqual(params["config"]["mcp_servers"]["workspace"]["url"], "http://127.0.0.1")
        self.assertIs(params["dynamicTools"], function_tools)

    def test_build_thread_session_params_omits_empty_optionals(self) -> None:
        params = build_thread_session_params(
            cfg=self.cfg,
            model=None,
            instructions=None,
            request_cwd=None,
            request_config_overrides=None,
            function_tools=[],
        )

        self.assertNotIn("baseInstructions", params)
        self.assertNotIn("model", params)
        self.assertNotIn("cwd", params)
        self.assertNotIn("config", params)
        self.assertNotIn("dynamicTools", params)

    def test_resolve_thread_id_starts_new_thread(self) -> None:
        client = FakeClient()
        store = FakeStore()
        thread_id = resolve_thread_id(
            client=client,
            store=store,
            thread_session_params={
                "approvalPolicy": "never",
                "baseInstructions": "请总结",
            },
            expected_resume_fingerprint="same_fp",
        )

        self.assertEqual(thread_id, "thread_started")
        self.assertEqual(store.lookups, [])
        self.assertEqual(
            client.start_calls,
            [{"approvalPolicy": "never", "baseInstructions": "请总结"}],
        )

    def test_build_turn_start_params_includes_only_present_fields(self) -> None:
        params = build_turn_start_params(
            request_cwd="/tmp/demo",
            model="codex-1",
            reasoning_effort="high",
            reasoning_summary="concise",
        )

        self.assertEqual(
            params,
            {
                "cwd": "/tmp/demo",
                "model": "codex-1",
                "effort": "high",
                "summary": "concise",
            },
        )

    def test_build_turn_start_params_omits_absent_fields(self) -> None:
        self.assertEqual(
            build_turn_start_params(
                request_cwd=None,
                model=None,
                reasoning_effort=None,
                reasoning_summary=None,
            ),
            {},
        )


if __name__ == "__main__":
    unittest.main()
