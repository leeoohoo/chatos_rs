from __future__ import annotations

import os
import sys
from typing import Any


def debug_enabled() -> bool:
    value = os.environ.get("GATEWAY_DEBUG", "")
    return value.strip().lower() in {"1", "true", "yes", "on"}


def debug_log(*parts: Any) -> None:
    if not debug_enabled():
        return
    message = " ".join(str(part) for part in parts)
    print(f"[gateway] {message}", file=sys.stderr, flush=True)


def reasoning_log(*parts: Any) -> None:
    message = " ".join(str(part) for part in parts)
    print(f"[gateway.reasoning] {message}", file=sys.stderr, flush=True)


def state_log(*parts: Any) -> None:
    message = " ".join(str(part) for part in parts)
    print(f"[gateway.state] {message}", file=sys.stderr, flush=True)
