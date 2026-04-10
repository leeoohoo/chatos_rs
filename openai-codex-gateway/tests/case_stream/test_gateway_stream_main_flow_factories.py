#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.main_flow_factories import build_default_stream_main_flow_factories  # noqa: E402


class GatewayStreamMainFlowFactoriesTest(unittest.TestCase):
    def test_build_default_stream_main_flow_factories(self) -> None:
        seen_prefixes: list[str] = []

        def id_factory(prefix: str) -> str:
            seen_prefixes.append(prefix)
            return f"{prefix}_{len(seen_prefixes)}"

        factories = build_default_stream_main_flow_factories(id_factory=id_factory)

        self.assertEqual(factories.message_id_factory(), "msg_1")
        self.assertEqual(factories.function_item_id_factory(), "fc_2")
        self.assertEqual(seen_prefixes, ["msg", "fc"])


if __name__ == "__main__":
    unittest.main()
