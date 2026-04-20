from __future__ import annotations

import json
from typing import Any


def encode_json_body(body: dict[str, Any]) -> bytes:
    return json.dumps(body, ensure_ascii=False).encode("utf-8")


def serialize_sse_event(data: dict[str, Any]) -> bytes:
    payload = json.dumps(data, ensure_ascii=False)
    event_name = data.get("type")
    chunks: list[bytes] = []
    if isinstance(event_name, str) and event_name:
        chunks.append(f"event: {event_name}\n".encode("utf-8"))
    chunks.append(f"data: {payload}\n\n".encode("utf-8"))
    return b"".join(chunks)
