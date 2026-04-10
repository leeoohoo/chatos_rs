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

    first_payload: dict[str, Any] = {
        "input": "请记住：我的名字是小李。只回复“记住了”。",
    }
    if model:
        first_payload["model"] = model

    first = client.responses.create(**first_payload)
    if not (first.id and first.id.strip()):
        raise RuntimeError("continuous-session test failed: first response id is empty")

    second_payload: dict[str, Any] = {
        "input": "我叫什么名字？只回复名字，不要解释。",
        "previous_response_id": first.id,
    }
    if model:
        second_payload["model"] = model

    second = client.responses.create(**second_payload)

    first_text = (first.output_text or "").strip()
    second_text = (second.output_text or "").strip()
    if not second_text:
        raise RuntimeError("continuous-session test failed: second output_text is empty")

    print("[continuous-session] ok")
    print(f"first_response_id: {first.id}")
    print(f"first_output_text: {first_text}")
    print(f"second_response_id: {second.id}")
    print(f"second_output_text: {second_text}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[continuous-session] failed: {exc}", file=sys.stderr)
        raise
