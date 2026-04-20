#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from typing import Any


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.error_boundary import run_stream_with_error_boundary  # noqa: E402


class GatewayStreamErrorBoundaryTest(unittest.TestCase):
    def test_run_stream_with_error_boundary_success(self) -> None:
        run_count = 0
        error_calls = 0

        def run_main_flow() -> None:
            nonlocal run_count
            run_count += 1

        def error_handler(**_kwargs: Any) -> None:
            nonlocal error_calls
            error_calls += 1

        run_stream_with_error_boundary(
            run_main_flow=run_main_flow,
            send_event=lambda _event: None,
            send_done_marker=lambda: None,
            debug_logger=lambda *_parts: None,
            print_traceback=lambda: None,
            error_handler=error_handler,
        )

        self.assertEqual(run_count, 1)
        self.assertEqual(error_calls, 0)

    def test_run_stream_with_error_boundary_broken_pipe(self) -> None:
        error_calls = 0

        def run_main_flow() -> None:
            raise BrokenPipeError()

        def error_handler(**_kwargs: Any) -> None:
            nonlocal error_calls
            error_calls += 1

        run_stream_with_error_boundary(
            run_main_flow=run_main_flow,
            send_event=lambda _event: None,
            send_done_marker=lambda: None,
            debug_logger=lambda *_parts: None,
            print_traceback=lambda: None,
            error_handler=error_handler,
        )

        self.assertEqual(error_calls, 0)

    def test_run_stream_with_error_boundary_exception(self) -> None:
        error_calls: list[dict[str, Any]] = []
        send_event = lambda _event: None
        send_done_marker = lambda: None
        debug_logger = lambda *_parts: None
        print_traceback = lambda: None

        def run_main_flow() -> None:
            raise RuntimeError("boom")

        def error_handler(**kwargs: Any) -> None:
            error_calls.append(kwargs)

        run_stream_with_error_boundary(
            run_main_flow=run_main_flow,
            send_event=send_event,
            send_done_marker=send_done_marker,
            debug_logger=debug_logger,
            print_traceback=print_traceback,
            error_handler=error_handler,
        )

        self.assertEqual(len(error_calls), 1)
        kwargs = error_calls[0]
        self.assertIsInstance(kwargs["exc"], RuntimeError)
        self.assertEqual(str(kwargs["exc"]), "boom")
        self.assertIs(kwargs["send_event"], send_event)
        self.assertIs(kwargs["send_done_marker"], send_done_marker)
        self.assertIs(kwargs["debug_logger"], debug_logger)
        self.assertIs(kwargs["print_traceback"], print_traceback)


if __name__ == "__main__":
    unittest.main()
