from __future__ import annotations

import argparse
import os
import shutil
from pathlib import Path

from gateway_base.types import GatewayConfig

# Keep behavior stable after moving this module into gateway_core/.
# runtime.py is now one directory deeper than before.
REPO_ROOT = Path(__file__).resolve().parents[2]
GATEWAY_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_STATE_DB_PATH = REPO_ROOT / ".local" / "openai-codex-gateway" / "gateway_state.sqlite3"


def resolve_codex_bin(cli_value: str | None) -> str | None:
    if cli_value:
        return cli_value

    env_value = os.environ.get("CODEX_GATEWAY_CODEX_BIN")
    if env_value:
        return env_value

    return shutil.which("codex")


def resolve_state_db_path() -> str:
    env_value = os.environ.get("CODEX_GATEWAY_STATE_DB")
    if env_value and env_value.strip():
        return env_value.strip()

    target = DEFAULT_STATE_DB_PATH
    legacy = GATEWAY_ROOT / "gateway_state.sqlite3"
    if target != legacy and not target.exists() and legacy.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        try:
            legacy.replace(target)
            for suffix in ("-wal", "-shm"):
                legacy_sidecar = Path(f"{legacy}{suffix}")
                target_sidecar = Path(f"{target}{suffix}")
                if legacy_sidecar.exists() and not target_sidecar.exists():
                    legacy_sidecar.replace(target_sidecar)
        except OSError:
            return str(legacy)
    return str(target)


def parse_args() -> GatewayConfig:
    parser = argparse.ArgumentParser(description="OpenAI-compatible gateway backed by codex SDK")
    parser.add_argument("--host", default="127.0.0.1", help="Bind host (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=8089, help="Bind port (default: 8089)")
    parser.add_argument("--codex-bin", default=None)
    parser.add_argument(
        "--state-db",
        default=resolve_state_db_path(),
        help="SQLite file for persisting response_id -> thread_id mappings",
    )
    parser.add_argument(
        "--cwd",
        default=os.environ.get("CODEX_GATEWAY_CWD") or str(REPO_ROOT),
        help="Working directory for codex app-server",
    )
    parser.add_argument(
        "--sandbox",
        choices=["read-only", "workspace-write", "danger-full-access"],
        default=os.environ.get("CODEX_GATEWAY_SANDBOX", "read-only"),
        help="Sandbox mode for new/resumed threads",
    )
    args = parser.parse_args()

    return GatewayConfig(
        host=args.host,
        port=args.port,
        codex_bin=resolve_codex_bin(args.codex_bin),
        cwd=args.cwd,
        sandbox=args.sandbox,
        approval_policy="on-request",
        state_db_path=args.state_db,
    )
