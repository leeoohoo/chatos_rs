from __future__ import annotations

import uuid
from typing import Any


def make_id(prefix: str) -> str:
    return f"{prefix}_{uuid.uuid4().hex}"


def error_payload(kind: str, message: str) -> dict[str, Any]:
    return {
        "error": {
            "type": kind,
            "message": message,
        }
    }


def to_json_compatible(value: Any) -> Any:
    if value is None:
        return None
    if isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, list):
        return [to_json_compatible(v) for v in value]
    if isinstance(value, dict):
        return {str(k): to_json_compatible(v) for k, v in value.items()}
    if hasattr(value, "model_dump"):
        return to_json_compatible(value.model_dump(mode="json"))
    if hasattr(value, "value"):
        return to_json_compatible(value.value)
    return str(value)
