#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from http import HTTPStatus
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_stream.headers import (  # noqa: E402
    write_default_stream_response_headers,
    write_stream_response_headers,
)


class GatewayStreamHeadersTest(unittest.TestCase):
    def test_write_stream_response_headers(self) -> None:
        calls: list[tuple[str, object]] = []

        def send_response(status: HTTPStatus) -> None:
            calls.append(("send_response", status))

        def write_common_headers() -> None:
            calls.append(("write_common_headers", None))

        def send_header(name: str, value: str) -> None:
            calls.append(("send_header", (name, value)))

        def end_headers() -> None:
            calls.append(("end_headers", None))

        write_stream_response_headers(
            send_response=send_response,
            write_common_headers=write_common_headers,
            send_header=send_header,
            end_headers=end_headers,
        )

        self.assertEqual(calls[0], ("send_response", HTTPStatus.OK))
        self.assertEqual(calls[1], ("write_common_headers", None))
        self.assertEqual(calls[2], ("send_header", ("Content-Type", "text/event-stream")))
        self.assertEqual(calls[3], ("send_header", ("Cache-Control", "no-cache")))
        self.assertEqual(calls[4], ("send_header", ("Connection", "close")))
        self.assertEqual(calls[5], ("end_headers", None))
        self.assertEqual(len(calls), 6)

    def test_write_default_stream_response_headers(self) -> None:
        class Target:
            def send_response(self, _status: HTTPStatus) -> None:  # pragma: no cover
                raise AssertionError("not expected")

            def _write_common_headers(self) -> None:  # pragma: no cover
                raise AssertionError("not expected")

            def send_header(self, _name: str, _value: str) -> None:  # pragma: no cover
                raise AssertionError("not expected")

            def end_headers(self) -> None:  # pragma: no cover
                raise AssertionError("not expected")

        target = Target()
        calls: list[dict[str, object]] = []

        def headers_writer(**kwargs: object) -> None:
            calls.append(kwargs)

        write_default_stream_response_headers(
            target=target,
            headers_writer=headers_writer,
        )

        self.assertEqual(len(calls), 1)
        kwargs = calls[0]
        send_response = kwargs["send_response"]
        write_common_headers = kwargs["write_common_headers"]
        send_header = kwargs["send_header"]
        end_headers = kwargs["end_headers"]

        self.assertIs(getattr(send_response, "__self__", None), target)
        self.assertIs(getattr(send_response, "__func__", None), Target.send_response)
        self.assertIs(getattr(write_common_headers, "__self__", None), target)
        self.assertIs(
            getattr(write_common_headers, "__func__", None),
            Target._write_common_headers,
        )
        self.assertIs(getattr(send_header, "__self__", None), target)
        self.assertIs(getattr(send_header, "__func__", None), Target.send_header)
        self.assertIs(getattr(end_headers, "__self__", None), target)
        self.assertIs(getattr(end_headers, "__func__", None), Target.end_headers)


if __name__ == "__main__":
    unittest.main()
