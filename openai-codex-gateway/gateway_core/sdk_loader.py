from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Any


def resolve_bundled_sdk_candidates(gateway_root: Path) -> list[Path]:
    return [
        gateway_root / "vendor",
    ]


def resolve_local_sdk_candidates(repo_root: Path) -> list[Path]:
    return [
        repo_root / "sdk" / "python" / "src",
        repo_root / "chat_app_server_rs" / "docs" / "codex" / "sdk" / "python" / "src",
    ]


def load_sdk_imports(
    *,
    repo_root: Path,
    gateway_root: Path,
) -> tuple[str, tuple[Any, ...]]:
    mode = os.environ.get("CODEX_GATEWAY_SDK_MODE", "auto").strip().lower()
    if mode not in {"auto", "installed", "local"}:
        mode = "auto"

    errors: list[str] = []

    def try_import() -> tuple[Any, ...]:
        from codex_app_server.client import AppServerClient, AppServerConfig
        from codex_app_server.generated.v2_all import (
            AgentMessageDeltaNotification,
            AgentMessageThreadItem,
            CommandExecutionThreadItem,
            DynamicToolCallThreadItem,
            FileChangeThreadItem,
            ImageViewThreadItem,
            ItemCompletedNotification,
            ItemStartedNotification,
            McpToolCallThreadItem,
            ModelListResponse,
            ReasoningSummaryTextDeltaNotification,
            ReasoningTextDeltaNotification,
            ReasoningThreadItem,
            ThreadTokenUsageUpdatedNotification,
            TurnCompletedNotification,
            WebSearchThreadItem,
        )

        return (
            AppServerClient,
            AppServerConfig,
            AgentMessageDeltaNotification,
            AgentMessageThreadItem,
            CommandExecutionThreadItem,
            DynamicToolCallThreadItem,
            FileChangeThreadItem,
            ImageViewThreadItem,
            ItemCompletedNotification,
            ItemStartedNotification,
            McpToolCallThreadItem,
            ModelListResponse,
            ReasoningSummaryTextDeltaNotification,
            ReasoningTextDeltaNotification,
            ReasoningThreadItem,
            ThreadTokenUsageUpdatedNotification,
            TurnCompletedNotification,
            WebSearchThreadItem,
        )

    if mode in {"auto", "local"}:
        for candidate in resolve_bundled_sdk_candidates(gateway_root):
            if not (candidate / "codex_app_server" / "client.py").exists():
                continue
            if str(candidate) not in sys.path:
                sys.path.insert(0, str(candidate))
            try:
                imports = try_import()
                return f"bundled:{candidate}", imports
            except ModuleNotFoundError as exc:
                errors.append(f"bundled sdk import failed from {candidate}: {exc}")

    if mode in {"auto", "installed"}:
        try:
            imports = try_import()
            return "installed", imports
        except ModuleNotFoundError as exc:
            errors.append(f"installed package import failed: {exc}")

    if mode in {"auto", "local"}:
        for candidate in resolve_local_sdk_candidates(repo_root):
            if not (candidate / "codex_app_server" / "client.py").exists():
                continue
            if str(candidate) not in sys.path:
                sys.path.insert(0, str(candidate))
            try:
                imports = try_import()
                return f"local:{candidate}", imports
            except ModuleNotFoundError as exc:
                errors.append(f"local sdk import failed from {candidate}: {exc}")

    local_install_hints: list[str] = []
    for candidate in resolve_local_sdk_candidates(repo_root):
        if (candidate / "codex_app_server" / "client.py").exists():
            sdk_project_dir = candidate.parent if candidate.name == "src" else candidate
            local_install_hints.append(
                f"  cd {sdk_project_dir}\n  python -m pip install -e ."
            )

    hint_lines = [
        "Missing Codex Python SDK.",
        "",
        "Preferred: use the SDK bundled inside openai-codex-gateway/vendor.",
        "Alternative: install the official SDK into your current Python environment.",
        "Fallback: install one of the bundled local SDK copies:",
    ]
    if local_install_hints:
        hint_lines.extend(local_install_hints)
    else:
        hint_lines.append("  (no local sdk/python candidate found)")
    if errors:
        hint_lines.extend(["", "Import attempts:"])
        hint_lines.extend(f"  - {err}" for err in errors)
    raise SystemExit("\n".join(hint_lines))
