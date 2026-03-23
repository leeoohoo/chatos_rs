#!/usr/bin/env python3
from __future__ import annotations

import os
import sys
from typing import Any

try:
    from openai import OpenAI
except ModuleNotFoundError as exc:  # pragma: no cover
    raise SystemExit(
        "Missing dependency: openai\n"
        "Install with: python -m pip install openai"
    ) from exc


def main() -> None:
    base_url = os.environ.get("GATEWAY_BASE_URL", "http://127.0.0.1:8089/v1")
    api_key = os.environ.get("GATEWAY_API_KEY", "dummy-key")
    model = os.environ.get("GATEWAY_TEST_MODEL")

    client = OpenAI(base_url=base_url, api_key=api_key)

    payload: dict[str, Any] = {
        "input": "请用一句话介绍 Rust 语言。",
    }
    if model:
        payload["model"] = model

    response = client.responses.create(**payload)

    output_text = response.output_text or ""
    if not output_text.strip():
        raise RuntimeError("single-turn test failed: output_text is empty")

    print("[single-turn] ok")
    print(f"response_id: {response.id}")
    print(f"output_text: {output_text}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[single-turn] failed: {exc}", file=sys.stderr)
        raise
