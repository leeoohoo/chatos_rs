#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ERRORS: list[str] = []


def read(relative_path: str) -> str:
    path = ROOT / relative_path
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        ERRORS.append(f"cannot read {relative_path}: {error}")
        return ""


def rust_files(relative_roots: list[str]) -> list[Path]:
    files: list[Path] = []
    for relative_root in relative_roots:
        root = ROOT / relative_root
        if not root.exists():
            ERRORS.append(f"source root is missing: {relative_root}")
            continue
        if root.is_file():
            if root.suffix == ".rs":
                files.append(root)
            continue
        files.extend(root.rglob("*.rs"))
    return sorted(
        path
        for path in set(files)
        if path.name != "tests.rs" and "tests" not in path.relative_to(ROOT).parts
    )


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def files_containing(files: list[Path], needle: str) -> set[str]:
    matches: set[str] = set()
    for path in files:
        try:
            content = path.read_text(encoding="utf-8")
        except OSError as error:
            ERRORS.append(f"cannot read {relative(path)}: {error}")
            continue
        if needle in content:
            matches.add(relative(path))
    return matches


def require(relative_path: str, needle: str, reason: str) -> None:
    if needle not in read(relative_path):
        ERRORS.append(f"{relative_path}: missing {reason} ({needle!r})")


def forbid(relative_path: str, needles: list[str], reason: str) -> None:
    content = read(relative_path)
    for needle in needles:
        if needle in content:
            ERRORS.append(f"{relative_path}: {reason} ({needle!r})")


def require_absent(relative_path: str, reason: str) -> None:
    path = ROOT / relative_path
    if path.is_file() or (path.is_dir() and any(item.is_file() for item in path.rglob("*"))):
        ERRORS.append(f"{relative_path}: {reason}")


for retired_path in [
    "crates/chatos_cloud_agent_protocol",
    "crates/chatos_cloud_agent_runtime",
    "crates/chatos_mcp_gateway",
    "task_runner_service/backend",
    "chatos/backend/src/modules/cloud_agent_runtime.rs",
    "chatos/backend/src/api/agent_chat/task_runner_callback.rs",
    "memory_engine/backend/src/cloud_agent_queue.rs",
    "memory_engine/backend/src/services/memory_cloud_agent.rs",
    "agent/src/implementations/memory_engine.rs",
    "agent/src/core",
    "agent/src/implementations",
    ".task_runner",
]:
    require_absent(retired_path, "retired server execution plane must stay physically deleted")

server_files = rust_files(
    [
        "chatos/backend/src",
        "memory_engine/backend/src",
        "config_center_service/backend/src",
        "mcp_management_service/backend/src",
        "plugin_management_service/backend/src",
        "user_service/backend/src",
        "crates",
    ]
)
for identifier in [
    "chatos_cloud_agent",
    "CloudAgent",
    "cloud_agent",
    "task_runner_service",
]:
    locations = sorted(files_containing(server_files, identifier))
    if locations:
        ERRORS.append(
            f"retired server execution identifier {identifier!r} returned in: "
            + ", ".join(locations)
        )

memory_agent_files = rust_files(["memory_engine/backend/src"])
for path in memory_agent_files:
    path_text = relative(path)
    forbid(
        path_text,
        ["McpExecutor", ".with_mcp_executor(", ".with_tool_executor("],
        "Memory Engine AI generation must remain tool-less",
    )

require(
    "memory_engine/backend/src/services/memory_ai_generation.rs",
    "generate_summary",
    "Memory Engine-owned model retry entrypoint",
)
require(
    "memory_engine/backend/src/services/ai_pipeline/summary_pipeline.rs",
    "SummaryPipelineState",
    "Memory Engine-owned summary generation",
)
forbid(
    "memory_engine/backend/Cargo.toml",
    ["chatos_ai_runtime", "chatos_agent"],
    "Memory Engine must own its tool-less AI request policy instead of importing an Agent runtime",
)
forbid(
    ".github/workflows/docker-images.yml",
    ["task_runner_service_backend", "chatos-rs-task-runner-backend"],
    "retired Task Runner Service image must not return to the release matrix",
)

require(
    "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+TerminalRelay.swift",
    "case .requestApproval:",
    "the fail-closed macOS user approval path",
)
forbid(
    "clients/macos/Sources/ChatOSConnector/NativeApprovalAgent.swift",
    ["McpManagementClient", "resolveRuntimeSession", "mcpManagement", "MCP_MANAGEMENT"],
    "macOS Command Approval Agent must remain local-only",
)
require(
    "clients/windows/src/ChatOS.Connector/Approval/CommandApprovalCoordinator.cs",
    "The automatic approval reviewer is unavailable; user approval is required.",
    "the Windows fail-closed approval fallback",
)

require(
    "mcp_management_service/backend/src/api/runtime_sessions.rs",
    "if !tool_plane.uses_managed_gateway()",
    "fail-closed rejection for local-only and tool-less Agents",
)
production_files = rust_files(
    [
        "agent/src",
        "clients/shared/rust",
        "local_connector_service/backend/src",
        "mcp_management_service/backend/src",
        "plugins/browser/crates",
        "crates",
    ]
)
for identifier in [
    "TaskRunnerSystemMcpAdapter",
    "LocalConnectorSystemMcpAdapter",
    "SystemMcpHostAdapter",
    "SystemMcpResolveContext",
    "ResolvedSystemMcpBackend",
    "MCP_MANAGEMENT_MODE",
    "MCP_MANAGEMENT_SHADOW",
]:
    locations = sorted(files_containing(production_files, identifier))
    if locations:
        ERRORS.append(
            f"retired Agent Tool Plane identifier {identifier!r} returned in: "
            + ", ".join(locations)
        )

if ERRORS:
    print("Agent Tool Plane architecture boundary violations:")
    for error in ERRORS:
        print(f"  - {error}")
    raise SystemExit(1)

print("[OK] Agent Tool Plane architecture boundaries passed.")
