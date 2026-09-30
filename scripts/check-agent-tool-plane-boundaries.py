#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import re
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


def require(relative_path: str, needle: str, reason: str) -> None:
    if needle not in read(relative_path):
        ERRORS.append(f"{relative_path}: missing {reason} ({needle!r})")


def forbid(relative_path: str, needles: list[str], reason: str) -> None:
    content = read(relative_path)
    for needle in needles:
        if needle in content:
            ERRORS.append(f"{relative_path}: {reason} ({needle!r})")


for retired_root in ("chatos", "task_runner_service", "mcp_management_service"):
    if (ROOT / retired_root).exists():
        ERRORS.append(f"retired server execution root still exists: {retired_root}")

workspace = read("Cargo.toml")
for retired_member in (
    "chatos/backend",
    "task_runner_service/backend",
    "mcp_management_service/backend",
    "crates/chatos_mcp_gateway",
    "crates/chatos_mcp_management_sdk",
):
    if retired_member in workspace:
        ERRORS.append(f"Cargo workspace still contains retired member: {retired_member}")

local_host_manifests = sorted((ROOT / "clients/local_agent_host").rglob("Cargo.toml"))
if not local_host_manifests:
    ERRORS.append("clients/local_agent_host has no Rust manifests")
for manifest in local_host_manifests:
    content = manifest.read_text(encoding="utf-8")
    for retired_dependency in (
        "task_runner_service",
        "mcp_management_service",
        "chatos/backend",
    ):
        if retired_dependency in content:
            relative = manifest.relative_to(ROOT).as_posix()
            ERRORS.append(
                f"{relative}: Local Agent Host depends on retired server plane "
                f"({retired_dependency!r})"
            )

protocol_source = read("clients/local_agent_host/layers/interface/src/lib.rs")
protocol_match = re.search(
    r"LOCAL_AGENT_PROTOCOL_VERSION:\s*u32\s*=\s*(\d+)", protocol_source
)
if protocol_match is None:
    ERRORS.append("Local Agent Host protocol version declaration is missing")
else:
    protocol_version = protocol_match.group(1)
    require(
        "clients/macos/Sources/ChatOSConnector/NativeLocalAgentHostLifecycle.swift",
        f"static let version = {protocol_version}",
        "the Rust Local Agent Host protocol version",
    )
    require(
        "clients/windows/src/ChatOS.Connector/LocalAgent/WindowsLocalAgentHostLifecycle.cs",
        f"private const int ProtocolVersion = {protocol_version};",
        "the Rust Local Agent Host protocol version",
    )

catalog = "agent/src/catalog.rs"
catalog_text = read(catalog)
conversation_descriptor = catalog_text.find("CHATOS_CONVERSATION_AGENT_DESCRIPTOR")
if conversation_descriptor < 0:
    ERRORS.append(f"{catalog}: conversation Agent descriptor is missing")
else:
    descriptor = catalog_text[conversation_descriptor : conversation_descriptor + 900]
    for required in (
        '"local-agent-host"',
        "AgentToolPlane::Managed",
        "AgentExecutionLocation::ClientEmbedded",
    ):
        if required not in descriptor:
            ERRORS.append(
                f"{catalog}: conversation Agent descriptor is missing {required!r}"
            )

local_descriptor = catalog_text.find("LOCAL_AGENT_EXECUTION_AGENT_DESCRIPTOR")
if local_descriptor < 0:
    ERRORS.append(f"{catalog}: Local Agent execution descriptor is missing")
else:
    descriptor = catalog_text[local_descriptor : local_descriptor + 900]
    for required in (
        '"local-agent-host"',
        "AgentToolPlane::Managed",
        "AgentExecutionLocation::ClientEmbedded",
    ):
        if required not in descriptor:
            ERRORS.append(f"{catalog}: Local Agent descriptor is missing {required!r}")

approval_descriptor = catalog_text.find("LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_DESCRIPTOR")
if approval_descriptor < 0 or "AgentToolPlane::LocalOnly" not in catalog_text[
    approval_descriptor : approval_descriptor + 900
]:
    ERRORS.append(f"{catalog}: Local Command Approval Agent is not fixed to LocalOnly")

forbid(
    "mcp/src/backend.rs",
    ["ServiceHttp", "Chatos", "LocalConnector"],
    "system MCP execution must expose only the Local Agent Host",
)
require(
    "mcp/src/backend.rs",
    "LocalAgentHost",
    "the only system MCP implementation host",
)
forbid(
    "mcp/src/catalog.rs",
    ['owner_service: "chatos"', "SystemMcpHost::Chatos", "ServiceHttp"],
    "system MCP catalog must not publish a server execution host",
)
forbid(
    "crates/chatos_plugin_management_sdk/src/dto.rs",
    ["    TaskManager,", "Self::TaskManager"],
    "the retired Task Manager must not remain in the public system MCP contract",
)
forbid(
    "crates/chatos_mcp_runtime/src/builtin_catalog.rs",
    ["    TaskManager,", "Self::TaskManager"],
    "the retired Task Manager must not remain in the builtin runtime contract",
)
forbid(
    "crates/chatos_plugin_management_sdk/src/dto.rs",
    ["RequirementSurveyRead", "RequirementSurveyWrite"],
    "requirement surveys must remain a Local Agent Host application protocol, not a system MCP",
)
forbid(
    "crates/chatos_mcp_runtime/src/builtin_catalog.rs",
    ["RequirementSurveyRead", "RequirementSurveyWrite"],
    "the builtin MCP runtime must not restore the retired requirement-survey tools",
)
forbid(
    "mcp/src/catalog.rs",
    ["builtin_requirement_survey_read", "builtin_requirement_survey_write"],
    "the system MCP catalog must not restore requirement-survey execution",
)

for retired_survey_path in (
    "clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/AgentRequirementSurveyRepository.swift",
    "clients/macos/Sources/ChatOSConnector/AgentGroupChatStore/SQLiteAgentGroupChatStore+RequirementSurvey.swift",
    "clients/windows/src/ChatOS.Connector/AgentTeams/AgentRequirementSurveySkillCatalog.cs",
    "clients/windows/src/ChatOS.Connector/AgentTeams/AgentTeamToolExecutor.Surveys.cs",
    "clients/windows/src/ChatOS.Connector/AgentTeams/SqliteAgentTeamStore.Surveys.cs",
):
    if (ROOT / retired_survey_path).exists():
        ERRORS.append(
            f"{retired_survey_path}: retired client requirement-survey implementation returned"
        )

forbid(
    "clients/windows/src/ChatOS.Connector/Persistence/LocalStateDatabase.cs",
    ["CREATE TABLE IF NOT EXISTS agent_requirement_surveys"],
    "Windows client storage must not recreate the retired Agent Team survey table",
)
forbid(
    "plugin_management_service/backend/src/store.rs",
    ["RETIRED_TASK_MANAGER", "is_retired_task_manager_mcp"],
    "Plugin Marketplace must not carry old Task Manager data compatibility",
)
forbid(
    "plugin_management_service/backend/src/seed.rs",
    [
        "RETIRED_SYSTEM_AGENT_KEYS",
        "remove_retired_system_agents",
        "remove_retired_system_mcps",
    ],
    "Plugin Marketplace seed must not migrate retired execution-plane data",
)
forbid(
    "plugin_management_service/backend/src/state.rs",
    ["remove_retired_direct_local_mcps"],
    "Plugin Marketplace startup must not migrate retired Local Connector MCP data",
)
forbid(
    "admin_console/src/modules/config-center/pages.tsx",
    ["chatos-backend", "task-runner", "mcp-management-service"],
    "Configuration Center UI must not hide retired service data",
)
forbid(
    "admin_console/src/modules/config-center/QueueOperationsPanel.tsx",
    ["task-runner", "mcp-management"],
    "queue operations UI must not filter retired execution services",
)

require(
    "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+Approval.swift",
    "case .requestApproval:",
    "the fail-closed macOS user approval path",
)
forbid(
    "clients/macos/Sources/ChatOSConnector/NativeApprovalAgent.swift",
    ["McpManagementClient", "resolveRuntimeSession", "MCP_MANAGEMENT"],
    "macOS command approval must remain local-only",
)
require(
    "clients/windows/src/ChatOS.Connector/Approval/CommandApprovalCoordinator.cs",
    "The automatic approval reviewer is unavailable; user approval is required.",
    "the fail-closed Windows approval fallback",
)

memory_roots = [
    ROOT / "memory_engine/backend/src",
    ROOT / "agent/src/implementations/memory_engine.rs",
]
for root in memory_roots:
    files = [root] if root.is_file() else sorted(root.rglob("*.rs"))
    for path in files:
        content = path.read_text(encoding="utf-8")
        for forbidden in ("McpExecutor", ".with_mcp_executor(", ".with_tool_executor("):
            if forbidden in content:
                relative = path.relative_to(ROOT).as_posix()
                ERRORS.append(
                    f"{relative}: Memory Engine tool_plane=none boundary violated "
                    f"({forbidden!r})"
                )

if ERRORS:
    print("Agent Tool Plane architecture boundary violations:")
    for error in ERRORS:
        print(f"  - {error}")
    raise SystemExit(1)

print("[OK] Local Agent Tool Plane architecture boundaries passed.")
