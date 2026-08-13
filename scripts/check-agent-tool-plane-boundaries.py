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
        {
            path
            for path in files
            if path.name != "tests.rs" and "tests" not in path.relative_to(ROOT).parts
        }
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


def require_exact_locations(
    label: str,
    files: list[Path],
    needle: str,
    expected: set[str],
) -> None:
    actual = files_containing(files, needle)
    if actual == expected:
        return
    missing = sorted(expected - actual)
    unexpected = sorted(actual - expected)
    if missing:
        ERRORS.append(f"{label}: expected locations missing: {', '.join(missing)}")
    if unexpected:
        ERRORS.append(f"{label}: unexpected locations: {', '.join(unexpected)}")


cloud_agent_roots = [
    "chatos/backend/src",
    "task_runner_service/backend/src",
    "project_management_service/backend/src",
    "memory_engine/backend/src",
]
cloud_agent_files = rust_files(cloud_agent_roots)

require_exact_locations(
    "managed Runtime Session resolution",
    rust_files(["crates/chatos_mcp_gateway/src"]),
    ".resolve_runtime_session(",
    {"crates/chatos_mcp_gateway/src/lib.rs"},
)

require_exact_locations(
    "shared MCP Management Gateway construction",
    cloud_agent_files,
    "McpManagementGatewayBuilder::new(",
    {
        "chatos/backend/src/modules/conversation_runtime/runtime_context/mcp_management_gateway.rs",
        "task_runner_service/backend/src/services/run_model_phase/setup/preparation/mcp_management_gateway.rs",
        "project_management_service/backend/src/services/environment_agent/mcp_management_gateway.rs",
    },
)
require_exact_locations(
    "cloud Agent direct Runtime Session resolution",
    cloud_agent_files,
    ".resolve_runtime_session(",
    set(),
)
for gateway_path in [
    "task_runner_service/backend/src/services/run_model_phase/setup/preparation/mcp_management_gateway.rs",
    "project_management_service/backend/src/services/environment_agent/mcp_management_gateway.rs",
]:
    forbid(
        gateway_path,
        ["McpManagementClient::new", ".resolve_runtime_session(", "format!(\"Bearer"],
        "cloud Agent must use the shared MCP Management Gateway builder",
    )

expected_executor_gateway_assembly = {
    "task_runner_service/backend/src/services/run_model_phase/setup/preparation.rs",
    "project_management_service/backend/src/services/environment_agent/runtime/analysis.rs",
}
require_exact_locations(
    "cloud Agent MCP executor assembly",
    cloud_agent_files,
    ".with_http_server(",
    expected_executor_gateway_assembly,
)

chatos_runtime_files = rust_files(["chatos/backend/src/modules/conversation_runtime"])
forbid(
    "chatos/backend/src/modules/conversation_runtime/runtime_context/mcp_management_gateway.rs",
    ["McpHttpServer {", "McpManagementClient::new", ".resolve_runtime_session("],
    "ChatOS must use the shared MCP Management Gateway builder",
)
forbid(
    "chatos/backend/src/modules/conversation_runtime/runtime_context.rs",
    ["McpStdioServer {", "McpBuiltinServer {", ".with_builtin_servers("],
    "ChatOS cloud runtime must not construct direct MCP providers",
)
require(
    "chatos/backend/src/modules/conversation_runtime/runtime_context.rs",
    "empty_mcp_server_bundle()",
    "an empty runtime MCP bundle before gateway resolution",
)
require(
    "chatos/backend/src/modules/conversation_runtime/runtime_context.rs",
    "http_servers.push(server);",
    "the single MCP Management gateway insertion",
)

task_runner_model_files = rust_files(
    ["task_runner_service/backend/src/services/run_model_phase"]
)
for path in task_runner_model_files:
    path_text = relative(path)
    forbid(
        path_text,
        [
            ".with_builtin_servers(",
            ".with_builtin_registry(",
            ".with_stdio_server(",
            ".with_stdio_servers(",
        ],
        "Task Runner model execution must only use the MCP Management gateway",
    )
require(
    "task_runner_service/backend/src/services/run_model_phase/setup/preparation.rs",
    ".with_http_server(mcp_management_server)",
    "single-server MCP Management executor assembly",
)

project_environment_analysis = (
    "project_management_service/backend/src/services/environment_agent/runtime/analysis.rs"
)
require(
    project_environment_analysis,
    "resolve_project_environment_mcp(",
    "MCP Management Runtime Session resolution",
)
require(
    project_environment_analysis,
    ".with_http_server(gateway.server().clone())",
    "single-server MCP Management executor assembly",
)
require(
    project_environment_analysis,
    "client.close_runtime_session(session_ref).await",
    "terminal Runtime Session cleanup",
)
require(
    project_environment_analysis,
    "gateway.close().await",
    "failed-start Runtime Session cleanup",
)
forbid(
    project_environment_analysis,
    ["jsonrpc_http_call(", "McpStdioServer", ".with_builtin_servers("],
    "Project Environment Agent must not call a Provider directly",
)

local_approval = "local_connector_client/core/src/approval/ai_agent.rs"
require(local_approval, '"tool_plane": "local_only"', "local-only tool-plane metadata")
require(
    local_approval,
    ".with_tool_executor_arc(tool_executor)",
    "the device-local approval executor",
)
forbid(
    local_approval,
    [
        "McpManagementClient",
        "resolve_runtime_session",
        "mcp_server_url",
        "runtime_token",
        "MCP_MANAGEMENT",
    ],
    "Local Command Approval Agent must never enter the cloud MCP Tool Plane",
)

catalog = "agent/src/catalog.rs"
catalog_text = read(catalog)
approval_descriptor = catalog_text.find(
    "LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_DESCRIPTOR"
)
if approval_descriptor < 0 or "AgentToolPlane::LocalOnly" not in catalog_text[
    approval_descriptor : approval_descriptor + 700
]:
    ERRORS.append(f"{catalog}: Local Command Approval Agent is not fixed to LocalOnly")

runtime_sessions = "mcp_management_service/backend/src/api/runtime_sessions.rs"
require(
    runtime_sessions,
    "if !tool_plane.uses_managed_gateway()",
    "fail-closed rejection for local-only and tool-less Agents",
)

memory_agent_files = rust_files(
    ["memory_engine/backend/src", "agent/src/implementations/memory_engine.rs"]
)
for path in memory_agent_files:
    path_text = relative(path)
    forbid(
        path_text,
        ["McpExecutor", ".with_mcp_executor(", ".with_tool_executor("],
        "Memory Engine Agents are tool_plane=none",
    )

production_roots = cloud_agent_roots + [
    "agent/src",
    "local_connector_client/core/src",
    "local_connector_service/backend/src",
    "mcp_management_service/backend/src",
    "crates",
]
production_files = rust_files(production_roots)
retired_identifiers = [
    "TaskRunnerSystemMcpAdapter",
    "LocalConnectorSystemMcpAdapter",
    "SystemMcpHostAdapter",
    "SystemMcpResolveContext",
    "ResolvedSystemMcpBackend",
    "MCP_MANAGEMENT_MODE",
    "MCP_MANAGEMENT_SHADOW",
]
for identifier in retired_identifiers:
    locations = sorted(files_containing(production_files, identifier))
    if locations:
        ERRORS.append(
            f"retired Agent Tool Plane identifier {identifier!r} returned in: "
            + ", ".join(locations)
        )

require(
    "local_connector_service/backend/src/main.rs",
    "build_public_router",
    "a dedicated public Local Connector router",
)
require(
    "local_connector_service/backend/src/main.rs",
    "build_internal_router",
    "a dedicated internal Local Connector router",
)
require(
    "local_connector_service/backend/src/main.rs",
    "axum_server::bind_rustls",
    "mandatory TLS on the Local Connector internal listener",
)
for config_path, env_key in [
    ("chatos/backend/src/config.rs", "CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL"),
    (
        "project_management_service/backend/src/config.rs",
        "PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL",
    ),
    (
        "mcp_management_service/backend/src/config.rs",
        "MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL",
    ),
]:
    require(config_path, f'require_https_base_url(\n            "{env_key}"', "strict HTTPS Local Connector validation")
    require(
        config_path,
        'build_mtls_http_client(',
        "a certificate-bound Local Connector HTTP client",
    )

forbid(
    "mcp_management_service/backend/src/config.rs",
    ['resolve_service_base_url(\n            "local-connector-service"'],
    "Local Connector internal mTLS routing must not be replaced by public service discovery",
)
require(
    "chatos/backend/src/api/terminals/ws_handlers.rs",
    "connect_async_tls_with_config",
    "mTLS for Local Connector terminal WebSocket forwarding",
)
compose = read("docker/compose.yml")
for env_key in [
    "CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL",
    "PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL",
    "MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL",
]:
    if f"{env_key}: http://" in compose:
        ERRORS.append(
            f"docker/compose.yml: {env_key} must never route an internal caller over plain HTTP"
        )
    if f"{env_key}: https://local-connector-service-backend:39232" not in compose:
        ERRORS.append(
            f"docker/compose.yml: {env_key} is not pinned to the Local Connector mTLS listener"
        )

require(
    "chatos/backend/src/lib.rs",
    "api::public_router()",
    "a dedicated public ChatOS router",
)
require(
    "chatos/backend/src/lib.rs",
    "api::internal_router()",
    "a dedicated internal ChatOS router",
)
require(
    "chatos/backend/src/lib.rs",
    "axum_server::bind_rustls",
    "mandatory TLS on the ChatOS internal listener",
)
require(
    "task_runner_service/backend/src/config/env_support.rs",
    'require_https_base_url(\n            "TASK_RUNNER_CHATOS_CALLBACK_URL"',
    "strict HTTPS ChatOS callback validation",
)
require(
    "task_runner_service/backend/src/config/env_support.rs",
    'required_bootstrap_path("CHATOS_MTLS_CLIENT_IDENTITY_PATH")',
    "a certificate-bound Task Runner ChatOS client",
)
require(
    "mcp_management_service/backend/src/config.rs",
    'require_https_base_url(\n            "MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL"',
    "strict HTTPS ChatOS provider validation",
)
require(
    "mcp_management_service/backend/src/config.rs",
    'required_path("CHATOS_MTLS_CLIENT_IDENTITY_PATH")',
    "a certificate-bound MCP Management ChatOS client",
)
forbid(
    "task_runner_service/backend/src/main.rs",
    ['resolve_service_url(\n                "chatos-backend"'],
    "ChatOS internal mTLS callback routing must not be replaced by public service discovery",
)
forbid(
    "mcp_management_service/backend/src/config.rs",
    ['resolve_service_base_url(\n            "chatos-backend"'],
    "ChatOS internal mTLS provider routing must not be replaced by public service discovery",
)
for env_key, expected in [
    (
        "TASK_RUNNER_CHATOS_CALLBACK_URL",
        "https://chatos-backend:3999/api/agent/chat/task-runner/callback",
    ),
    ("MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL", "https://chatos-backend:3999"),
]:
    if f"{env_key}: http://" in compose:
        ERRORS.append(
            f"docker/compose.yml: {env_key} must never route an internal caller over plain HTTP"
        )
    if f"{env_key}: {expected}" not in compose:
        ERRORS.append(
            f"docker/compose.yml: {env_key} is not pinned to the ChatOS mTLS listener"
        )

if ERRORS:
    print("Agent Tool Plane architecture boundary violations:")
    for error in ERRORS:
        print(f"  - {error}")
    raise SystemExit(1)

print("[OK] Agent Tool Plane architecture boundaries passed.")
