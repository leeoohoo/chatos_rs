from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class GatewayConfig:
    host: str
    port: int
    codex_bin: str | None
    cwd: str | None
    sandbox: str
    approval_policy: str
    state_db_path: str


@dataclass
class ToolCallRecord:
    call_id: str
    name: str
    arguments: Any


@dataclass
class TurnResult:
    thread_id: str
    turn_id: str
    output_text: str
    reasoning_text: str
    status: str
    usage: dict[str, Any] | None
    error: dict[str, Any] | None
    tool_calls: list[ToolCallRecord]
