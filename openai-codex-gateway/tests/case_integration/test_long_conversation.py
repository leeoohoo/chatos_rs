#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import re
import sys
from dataclasses import dataclass
from typing import Any

try:
    from openai import OpenAI
except ModuleNotFoundError as exc:  # pragma: no cover
    raise SystemExit(
        "Missing dependency: openai\n"
        "Install with: python -m pip install openai"
    ) from exc


@dataclass
class Turn:
    prompt: str
    label: str


def parse_json_like(text: str) -> dict[str, Any]:
    text = text.strip()
    if not text:
        raise ValueError("empty output")

    try:
        return json.loads(text)
    except json.JSONDecodeError:
        match = re.search(r"\{.*\}", text, re.DOTALL)
        if not match:
            raise ValueError("output is not JSON-like")
        return json.loads(match.group(0))


def main() -> None:
    base_url = os.environ.get("GATEWAY_BASE_URL", "http://127.0.0.1:8089/v1")
    api_key = os.environ.get("GATEWAY_API_KEY", "dummy-key")
    model = os.environ.get("GATEWAY_TEST_MODEL")

    client = OpenAI(base_url=base_url, api_key=api_key)

    turns = [
        Turn(
            label="t1-memory-seed",
            prompt=(
                "请记住以下事实，并且只回复“记住了”四个字："
                "我的名字是小李；我住在上海；我最喜欢的语言是Rust；"
                "我的宠物是一只猫，名字叫阿布。"
            ),
        ),
        Turn(
            label="t2-recall-language",
            prompt="我最喜欢什么语言？只回复语言名称。",
        ),
        Turn(
            label="t3-recall-pet",
            prompt="我的宠物叫什么名字？只回复名字。",
        ),
        Turn(
            label="t4-recall-city",
            prompt="我住在哪个城市？只回复城市名。",
        ),
        Turn(
            label="t5-summary-json",
            prompt=(
                "请只输出一行 JSON（不要 markdown），格式如下："
                '{"name":"","city":"","favorite_language":"","pet_name":""}'
                "，并填入我们对话里你记住的信息。"
            ),
        ),
    ]

    previous_response_id: str | None = None
    outputs: list[tuple[str, str, str]] = []

    for idx, turn in enumerate(turns, start=1):
        payload: dict[str, Any] = {
            "input": turn.prompt,
        }
        if model:
            payload["model"] = model
        if previous_response_id:
            payload["previous_response_id"] = previous_response_id

        response = client.responses.create(**payload)
        response_id = response.id
        output_text = (response.output_text or "").strip()
        print(f"turn {idx} response_id: {response_id}")

        if not response_id:
            raise RuntimeError(f"turn {idx} failed: empty response id")
        if not output_text:
            raise RuntimeError(f"turn {idx} failed: empty output text")

        outputs.append((turn.label, response_id, output_text))
        previous_response_id = response_id

    final_json = parse_json_like(outputs[-1][2])

    # 软校验：检查关键记忆是否存在（忽略大小写）
    expect = {
        "name": "小李",
        "city": "上海",
        "favorite_language": "rust",
        "pet_name": "阿布",
    }

    name = str(final_json.get("name", ""))
    city = str(final_json.get("city", ""))
    lang = str(final_json.get("favorite_language", "")).lower()
    pet = str(final_json.get("pet_name", ""))

    if expect["name"] not in name:
        raise RuntimeError(f"long-session check failed: name mismatch: {name!r}")
    if expect["city"] not in city:
        raise RuntimeError(f"long-session check failed: city mismatch: {city!r}")
    if expect["favorite_language"] not in lang:
        raise RuntimeError(f"long-session check failed: favorite_language mismatch: {lang!r}")
    if expect["pet_name"] not in pet:
        raise RuntimeError(f"long-session check failed: pet_name mismatch: {pet!r}")

    print("[long-conversation] ok")
    for label, resp_id, text in outputs:
        print(f"{label}: {resp_id}")
        print(f"  output: {text}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"[long-conversation] failed: {exc}", file=sys.stderr)
        raise
