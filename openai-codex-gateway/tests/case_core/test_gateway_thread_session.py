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
    instructions_fingerprint,
    resolve_thread_id,
)


class FakeClient:
    def __init__(self) -> None:
        self.resume_calls: list[tuple[str, dict[str, object]]] = []
        self.start_calls: list[dict[str, object]] = []

    def thread_resume(self, thread_id: str, params: dict[str, object]) -> SimpleNamespace:
        self.resume_calls.append((thread_id, params))
        return SimpleNamespace(thread=SimpleNamespace(id="thread_resumed"))

    def thread_start(self, params: dict[str, object]) -> SimpleNamespace:
        self.start_calls.append(params)
        return SimpleNamespace(thread=SimpleNamespace(id="thread_started"))


class FakeStore:
    def __init__(self, mapping: dict[str, str] | None = None) -> None:
        self.mapping = mapping or {}
        self.lookups: list[str] = []
        self.bindings: dict[str, dict[str, str]] = {}

    def get_thread(self, response_id: str) -> str | None:
        self.lookups.append(response_id)
        return self.mapping.get(response_id)

    def get_thread_binding(self, response_id: str) -> dict[str, str] | None:
        self.lookups.append(response_id)
        return self.bindings.get(response_id)


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

    def test_resolve_thread_id_resumes_existing_thread(self) -> None:
        client = FakeClient()
        store = FakeStore()
        store.bindings["resp_prev"] = {
            "thread_id": "thread_saved",
            "instructions_fingerprint": instructions_fingerprint("请总结"),
        }
        thread_id = resolve_thread_id(
            client=client,
            store=store,
            previous_response_id="resp_prev",
            thread_session_params={
                "approvalPolicy": "never",
                "baseInstructions": "请总结",
            },
            expected_resume_fingerprint="same_fp",
        )

        self.assertEqual(thread_id, "thread_resumed")
        self.assertEqual(store.lookups, ["resp_prev"])
        self.assertEqual(
            client.resume_calls,
            [("thread_saved", {"approvalPolicy": "never", "baseInstructions": "请总结"})],
        )
        self.assertEqual(client.start_calls, [])

    def test_resolve_thread_id_starts_new_thread_when_instructions_change(self) -> None:
        client = FakeClient()
        store = FakeStore()
        store.bindings["resp_prev"] = {
            "thread_id": "thread_saved",
            "instructions_fingerprint": instructions_fingerprint("旧规则"),
        }

        thread_id = resolve_thread_id(
            client=client,
            store=store,
            previous_response_id="resp_prev",
            thread_session_params={
                "approvalPolicy": "never",
                "baseInstructions": "新规则",
            },
            expected_resume_fingerprint="changed_fp",
        )

        self.assertEqual(thread_id, "thread_started")
        self.assertEqual(store.lookups, ["resp_prev"])
        self.assertEqual(client.resume_calls, [])
        self.assertEqual(
            client.start_calls,
            [{"approvalPolicy": "never", "baseInstructions": "新规则"}],
        )

    def test_resolve_thread_id_starts_new_thread_without_previous_response(self) -> None:
        client = FakeClient()
        store = FakeStore()
        thread_id = resolve_thread_id(
            client=client,
            store=store,
            previous_response_id=None,
            thread_session_params={"approvalPolicy": "never"},
            expected_resume_fingerprint="",
        )

        self.assertEqual(thread_id, "thread_started")
        self.assertEqual(store.lookups, [])
        self.assertEqual(client.start_calls, [{"approvalPolicy": "never"}])
        self.assertEqual(client.resume_calls, [])

    def test_resolve_thread_id_rejects_unknown_previous_response_id(self) -> None:
        with self.assertRaises(ValueError):
            resolve_thread_id(
                client=FakeClient(),
                store=FakeStore(),
                previous_response_id="resp_missing",
                thread_session_params={"approvalPolicy": "never"},
                expected_resume_fingerprint="",
            )

    def test_resolve_thread_id_starts_new_thread_when_resume_fingerprint_changes(self) -> None:
        client = FakeClient()
        store = FakeStore()
        old_thread_params = {
            "approvalPolicy": "never",
            "baseInstructions": "请总结",
            "dynamicTools": [{"name": "tool_a"}],
        }
        old_turn_params = {
            "model": "codex-1",
            "summary": "concise",
        }
        store.bindings["resp_prev"] = {
            "thread_id": "thread_saved",
            "instructions_fingerprint": instructions_fingerprint("请总结"),
            "resume_fingerprint": build_resume_fingerprint(
                old_thread_params,
                old_turn_params,
            ),
        }
        new_thread_params = {
            "approvalPolicy": "never",
            "baseInstructions": "请总结",
            "dynamicTools": [{"name": "tool_b"}],
        }

        thread_id = resolve_thread_id(
            client=client,
            store=store,
            previous_response_id="resp_prev",
            thread_session_params=new_thread_params,
            expected_resume_fingerprint=build_resume_fingerprint(
                new_thread_params,
                {
                    "model": "codex-1",
                    "summary": "concise",
                },
            ),
        )

        self.assertEqual(thread_id, "thread_started")
        self.assertEqual(store.lookups, ["resp_prev"])
        self.assertEqual(client.resume_calls, [])
        self.assertEqual(client.start_calls, [new_thread_params])

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
