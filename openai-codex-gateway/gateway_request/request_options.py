from __future__ import annotations

from typing import Any

from gateway_base.logging import debug_log

from .common import (
    normalize_non_empty_string,
    validate_string_list,
    validate_string_map,
)


def extract_bearer_token(value: str | None) -> str | None:
    if not value:
        return None
    parts = value.strip().split(" ", 1)
    if len(parts) != 2 or parts[0].lower() != "bearer":
        return None
    token = parts[1].strip()
    return token or None


def extract_reasoning_options(payload: dict[str, Any]) -> tuple[str | None, str | None]:
    reasoning = payload.get("reasoning")
    if reasoning is None:
        return None, None

    allowed_efforts = {"none", "minimal", "low", "medium", "high", "xhigh"}
    allowed_summaries = {"none", "auto", "concise", "detailed"}

    if isinstance(reasoning, str):
        normalized = reasoning.strip().lower()
        return (normalized, None) if normalized in allowed_efforts else (None, None)

    if not isinstance(reasoning, dict):
        return None, None

    effort_raw = reasoning.get("effort")
    effort = effort_raw.strip().lower() if isinstance(effort_raw, str) else None
    if effort not in allowed_efforts:
        effort = None

    summary_raw = reasoning.get("summary")
    summary = summary_raw.strip().lower() if isinstance(summary_raw, str) else None
    if summary not in allowed_summaries:
        summary = None

    return effort, summary


def extract_request_cwd(payload: dict[str, Any]) -> str | None:
    return normalize_non_empty_string(payload.get("cwd"), "request `cwd`")


def extract_request_config_overrides(payload: dict[str, Any]) -> dict[str, Any] | None:
    tools = payload.get("tools")
    if tools is not None and not isinstance(tools, list):
        raise ValueError("`tools` must be a list")

    mcp_servers: dict[str, Any] = {}
    for tool in tools or []:
        if not isinstance(tool, dict):
            raise ValueError("each item in `tools` must be an object")

        tool_type = tool.get("type")
        if tool_type != "mcp":
            continue

        label, mcp_config = parse_mcp_tool(tool)
        if label in mcp_servers:
            raise ValueError(f"duplicate mcp server label in `tools`: {label}")
        mcp_servers[label] = mcp_config

    tools_config = {
        "view_image": False,
        "web_search": {
            "enabled": False,
        },
    }
    config: dict[str, Any] = {
        "tools": tools_config,
        "web_search": "disabled",
        "mcp_servers": mcp_servers,
    }
    debug_log(
        "request.config_overrides",
        f"mcp_servers={len(mcp_servers)}",
        f"keys={','.join(config.keys())}",
    )
    return config


def parse_mcp_tool(tool: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    label = tool.get("server_label", tool.get("server_name"))
    if not isinstance(label, str) or not label.strip():
        raise ValueError("mcp tool requires non-empty `server_label` (or `server_name`)")
    server_label = label.strip()

    server_url = tool.get("server_url", tool.get("url"))
    command = tool.get("command")
    url = normalize_non_empty_string(server_url, "mcp tool `server_url`")
    cmd = normalize_non_empty_string(command, "mcp tool `command`")

    has_url = url is not None
    has_command = cmd is not None
    if has_url == has_command:
        raise ValueError(
            f"mcp tool `{server_label}` must provide exactly one transport: "
            "`server_url` (HTTP) or `command` (stdio)"
        )

    config: dict[str, Any] = {}
    if has_url:
        config["url"] = url
        bearer_token = tool.get("bearer_token")
        if bearer_token is not None:
            raise ValueError(
                f"mcp tool `{server_label}` does not allow inline `bearer_token`; "
                "use `bearer_token_env_var` instead"
            )

        bearer_env = tool.get("bearer_token_env_var")
        if bearer_env is not None:
            bearer_env_name = normalize_non_empty_string(
                bearer_env, f"mcp tool `{server_label}` `bearer_token_env_var`"
            )
            if bearer_env_name is None:
                raise ValueError(
                    f"mcp tool `{server_label}` has invalid `bearer_token_env_var`"
                )
            config["bearer_token_env_var"] = bearer_env_name

        http_headers = validate_string_map(
            tool.get("headers"),
            f"mcp tool `{server_label}` `headers`",
        )
        if http_headers:
            config["http_headers"] = http_headers

        env_http_headers = validate_string_map(
            tool.get("env_headers", tool.get("env_http_headers")),
            f"mcp tool `{server_label}` `env_headers`",
        )
        if env_http_headers:
            config["env_http_headers"] = env_http_headers
    else:
        config["command"] = cmd

        args = validate_string_list(tool.get("args"), f"mcp tool `{server_label}` `args`")
        if args is not None:
            config["args"] = args

        env = validate_string_map(tool.get("env"), f"mcp tool `{server_label}` `env`")
        if env:
            config["env"] = env

        env_vars = validate_string_list(
            tool.get("env_vars"),
            f"mcp tool `{server_label}` `env_vars`",
        )
        if env_vars is not None:
            config["env_vars"] = env_vars

        cwd = tool.get("cwd")
        if cwd is not None:
            cwd_value = normalize_non_empty_string(cwd, f"mcp tool `{server_label}` `cwd`")
            if cwd_value is None:
                raise ValueError(f"mcp tool `{server_label}` has invalid `cwd`")
            config["cwd"] = cwd_value

    enabled_tools = validate_string_list(
        tool.get("enabled_tools", tool.get("allowed_tools")),
        f"mcp tool `{server_label}` `enabled_tools`",
    )
    if enabled_tools is not None:
        config["enabled_tools"] = enabled_tools

    disabled_tools = validate_string_list(
        tool.get("disabled_tools", tool.get("blocked_tools")),
        f"mcp tool `{server_label}` `disabled_tools`",
    )
    if disabled_tools is not None:
        config["disabled_tools"] = disabled_tools

    required = tool.get("required")
    if required is not None:
        if not isinstance(required, bool):
            raise ValueError(f"mcp tool `{server_label}` `required` must be a boolean")
        config["required"] = required

    return server_label, config
