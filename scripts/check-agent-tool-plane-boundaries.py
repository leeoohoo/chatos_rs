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
    "agent",
    "crates/chatos_ai_runtime",
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
    "crates/chatos_model_transport/src/builder.rs",
    "crates/chatos_model_transport/src/memory_context",
    "crates/chatos_model_transport/src/memory_context.rs",
    "crates/chatos_model_transport/src/runtime",
    "crates/chatos_model_transport/src/runtime.rs",
    "crates/chatos_model_transport/src/task",
    "crates/chatos_model_transport/src/task.rs",
    "crates/chatos_model_transport/src/tool_runtime",
    "crates/chatos_model_transport/src/tool_runtime.rs",
    "mcp/provider_skills/task-runner-service.md",
    ".harness/pipelines/images/image-task-runner-backend.yml",
    "official_website_service/frontend/public/showcase/task-runner.png",
    "mcp_management_service/backend",
    "crates/chatos_mcp_management_sdk",
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
    "memory_engine/backend/src/services/memory_model_runtime.rs",
    "MemoryModelJobRuntime",
    "Memory Engine-owned tool-free model job boundary",
)
require(
    "memory_engine/backend/src/services/ai_pipeline/summary_pipeline.rs",
    "SummaryPipelineState",
    "Memory Engine-owned summary generation",
)
require(
    "memory_engine/backend/src/ai/client/request.rs",
    "request_responses",
    "Memory Engine-owned Responses request and stream parser",
)
require(
    "memory_engine/backend/src/ai/client/request.rs",
    "request_chat_completions",
    "Memory Engine-owned Chat Completions request and stream parser",
)
require(
    "memory_engine/backend/src/services/ai_pipeline/overflow.rs",
    "is_context_overflow_error",
    "Memory Engine-owned context overflow classification",
)
require(
    "memory_engine/backend/src/ai/retry.rs",
    "transient_retry_backoff_ms",
    "Memory Engine-owned bounded transient retry policy",
)
forbid(
    "memory_engine/backend/Cargo.toml",
    ["chatos_ai_runtime", "chatos_model_transport", "chatos_agent"],
    "Memory Engine must own its tool-less AI request policy instead of importing an Agent runtime",
)
for path in memory_agent_files:
    path_text = relative(path)
    forbid(
        path_text,
        ["ManagedMemoryAgentRuntime", "build_managed_memory_agent_runtime"],
        "Memory Engine must not restore the retired managed Agent runtime abstraction",
    )
forbid(
    "crates/chatos_model_transport/Cargo.toml",
    ["local-agent-loop", "chatos_mcp_runtime", "memory_engine_sdk"],
    "server model transport must not regain an Agent loop, tool runtime, or Memory Engine client",
)
forbid(
    ".github/workflows/docker-images.yml",
    ["task_runner_service_backend", "chatos-rs-task-runner-backend"],
    "retired Task Runner Service image must not return to the release matrix",
)
forbid(
    ".drone.yml",
    ["task_runner_service_backend", "backend-task-runner"],
    "retired Task Runner Service must not return to CI",
)
forbid(
    "official_website_service/frontend/scripts/capture-showcase.mjs",
    ["task-runner", "39091"],
    "the website showcase capture must not target the retired Task Runner service",
)
for readme in ["README.md", "README.zh-CN.md"]:
    forbid(
        readme,
        ["showcase/task-runner.png"],
        "the repository overview must not publish a retired Task Runner service screenshot",
    )
for website_source in [
    "official_website_service/backend/src/service_status.rs",
    "official_website_service/backend/src/site_manifest.rs",
]:
    forbid(
        website_source,
        ["OFFICIAL_WEBSITE_STATUS_TASK_RUNNER_URL", "TASK_RUNNER_BACKEND_PORT"],
        "the website must not publish or probe a retired Task Runner service",
    )

require(
    "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+TerminalRelay.swift",
    "case .requestApproval:",
    "the fail-closed macOS user approval path",
)
forbid(
    "clients/macos/Sources/ChatOSConnector/NativeApprovalAgent.swift",
    [
        "McpManagementClient",
        "resolveRuntimeSession",
        "mcpManagement",
        "MCP_MANAGEMENT",
        "AgentRuntime().run",
        "AgentChatModelClient",
        "GatewayModelConfigDTO",
        "apiKey",
        "baseURL",
    ],
    "macOS Command Approval observer must not regain a model, tool, or secret runtime",
)
forbid(
    "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+TerminalRelay.swift",
    ["includeSecret: true", "include_secret=true"],
    "macOS approval production path must not request provider secrets",
)
require(
    "clients/shared/rust/chatos_local_agent_protocol/src/ipc.rs",
    "CreateApprovalReview(Box<CreateApprovalReviewCommand>)",
    "typed approval Run creation without provider configuration fields",
)
require(
    "clients/shared/rust/chatos_agent_profiles/src/approval.rs",
    'pub const APPROVAL_DECISION_TOOL: &str = "approval_decision"',
    "shared approval profile terminal decision boundary",
)
require(
    "clients/windows/src/ChatOS.Connector/Approval/CommandApprovalCoordinator.cs",
    "The automatic approval reviewer is unavailable; user approval is required.",
    "the Windows fail-closed approval fallback",
)

production_files = rust_files(
    [
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
