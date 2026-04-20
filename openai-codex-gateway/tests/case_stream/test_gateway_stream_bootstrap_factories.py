#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.bootstrap_factories import (  # noqa: E402
    build_default_stream_bootstrap_factories,
)


class GatewayStreamBootstrapFactoriesTest(unittest.TestCase):
    def test_build_default_stream_bootstrap_factories(self) -> None:
        id_calls: list[str] = []
        now_calls = 0

        def id_factory(prefix: str) -> str:
            id_calls.append(prefix)
            return f"{prefix}_123"

        def time_factory() -> float:
            nonlocal now_calls
            now_calls += 1
            return 456.78

        factories = build_default_stream_bootstrap_factories(
            id_factory=id_factory,
            time_factory=time_factory,
        )

        self.assertEqual(factories.response_id_factory(), "resp_123")
        self.assertEqual(id_calls, ["resp"])
        self.assertEqual(factories.created_at_factory(), 456)
        self.assertEqual(now_calls, 1)

    def test_build_default_stream_bootstrap_factories_custom_prefix(self) -> None:
        factories = build_default_stream_bootstrap_factories(
            id_factory=lambda prefix: f"{prefix}_x",
            time_factory=lambda: 1.9,
            response_id_prefix="r",
        )

        self.assertEqual(factories.response_id_factory(), "r_x")
        self.assertEqual(factories.created_at_factory(), 1)


if __name__ == "__main__":
    unittest.main()
