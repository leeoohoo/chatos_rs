#!/usr/bin/env python3
from __future__ import annotations

import io
import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_base.traceback import make_traceback_printer  # noqa: E402


class GatewayTracebackTest(unittest.TestCase):
    def test_make_traceback_printer_uses_given_stream(self) -> None:
        calls: list[dict[str, Any]] = []
        stream = io.StringIO()

        def traceback_printer(**kwargs: Any) -> None:
            calls.append(kwargs)

        print_traceback = make_traceback_printer(
            traceback_printer=traceback_printer,
            error_stream=stream,
        )
        print_traceback()

        self.assertEqual(len(calls), 1)
        self.assertIs(calls[0]["file"], stream)

    def test_make_traceback_printer_callable_can_repeat(self) -> None:
        call_count = 0

        def traceback_printer(**_kwargs: Any) -> None:
            nonlocal call_count
            call_count += 1

        print_traceback = make_traceback_printer(
            traceback_printer=traceback_printer,
            error_stream=io.StringIO(),
        )
        print_traceback()
        print_traceback()

        self.assertEqual(call_count, 2)


if __name__ == "__main__":
    unittest.main()
