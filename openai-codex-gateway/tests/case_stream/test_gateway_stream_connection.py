#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.connection import (  # noqa: E402
    make_close_connection_marker,
    make_close_connection_setter,
)


class GatewayStreamConnectionTest(unittest.TestCase):
    def test_make_close_connection_setter_default_attr(self) -> None:
        class Target:
            close_connection = False

        target = Target()
        setter = make_close_connection_setter(target=target)
        setter(True)

        self.assertTrue(target.close_connection)

    def test_make_close_connection_setter_custom_attr(self) -> None:
        class Target:
            should_close = False

        target = Target()
        setter = make_close_connection_setter(target=target, attr_name="should_close")
        setter(True)

        self.assertTrue(target.should_close)

    def test_make_close_connection_marker_sets_true(self) -> None:
        values: list[bool] = []

        marker = make_close_connection_marker(
            set_close_connection=lambda value: values.append(value)
        )
        marker()

        self.assertEqual(values, [True])

    def test_make_close_connection_marker_can_be_called_multiple_times(self) -> None:
        count = 0

        def set_close_connection(_value: bool) -> None:
            nonlocal count
            count += 1

        marker = make_close_connection_marker(set_close_connection=set_close_connection)
        marker()
        marker()

        self.assertEqual(count, 2)


if __name__ == "__main__":
    unittest.main()
