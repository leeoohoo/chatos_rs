#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from http import HTTPStatus
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_http.routing import (  # noqa: E402
    build_not_found_body,
    resolve_get_route,
    resolve_post_route,
    response_status_for_body,
)


class GatewayHttpRoutingTest(unittest.TestCase):
    def test_resolve_get_route(self) -> None:
        self.assertEqual(resolve_get_route("/healthz"), "healthz")
        self.assertEqual(resolve_get_route("/v1/models"), "models")
        self.assertEqual(resolve_get_route("/v1/responses"), "not_found")
        self.assertEqual(resolve_get_route("/unknown"), "not_found")

    def test_resolve_post_route(self) -> None:
        self.assertEqual(resolve_post_route("/v1/responses"), "responses")
        self.assertEqual(resolve_post_route("/healthz"), "not_found")
        self.assertEqual(resolve_post_route("/v1/models"), "not_found")
        self.assertEqual(resolve_post_route("/unknown"), "not_found")

    def test_response_status_for_body(self) -> None:
        self.assertEqual(
            response_status_for_body({"status": "completed"}),
            HTTPStatus.OK,
        )
        self.assertEqual(
            response_status_for_body({"status": "in_progress"}),
            HTTPStatus.OK,
        )
        self.assertEqual(
            response_status_for_body({"status": "failed"}),
            HTTPStatus.BAD_GATEWAY,
        )

    def test_build_not_found_body(self) -> None:
        body = build_not_found_body()
        self.assertEqual(body["error"]["type"], "not_found")
        self.assertEqual(body["error"]["message"], "not found")


if __name__ == "__main__":
    unittest.main()
