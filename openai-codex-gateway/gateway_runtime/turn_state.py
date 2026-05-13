from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class TurnRuntimeState:
    output_text: str = ""
    reasoning_text: str = ""
    reasoning_tokens: int = 0
    reasoning_event_count: int = 0
    usage: dict[str, Any] | None = None
    status: str = "failed"
    error: dict[str, Any] | None = None
    missing_tool_output_detected: bool = False
    interrupt_sent: bool = False
    disallowed_tool_error: str | None = None
