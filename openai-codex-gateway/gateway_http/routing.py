from __future__ import annotations

from http import HTTPStatus
from typing import Any, Literal

from gateway_base.utils import error_payload


HEALTHZ_PATH = "/healthz"
MODELS_PATH = "/v1/models"
RESPONSES_PATH = "/v1/responses"

GetRoute = Literal["healthz", "models", "not_found"]
PostRoute = Literal["responses", "not_found"]


def resolve_get_route(path: str) -> GetRoute:
    if path == HEALTHZ_PATH:
        return "healthz"
    if path == MODELS_PATH:
        return "models"
    return "not_found"


def resolve_post_route(path: str) -> PostRoute:
    if path == RESPONSES_PATH:
        return "responses"
    return "not_found"


def build_not_found_body() -> dict[str, Any]:
    return error_payload("not_found", "not found")


def response_status_for_body(body: dict[str, Any]) -> HTTPStatus:
    return HTTPStatus.OK if body.get("status") != "failed" else HTTPStatus.BAD_GATEWAY
