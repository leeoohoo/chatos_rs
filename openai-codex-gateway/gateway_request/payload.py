from __future__ import annotations

import base64
import binascii
import json
from typing import Any

from gateway_base.logging import debug_log


def extract_bearer_token(value: str | None) -> str | None:
    if not value:
        return None
    parts = value.strip().split(" ", 1)
    if len(parts) != 2 or parts[0].lower() != "bearer":
        return None
    token = parts[1].strip()
    return token or None


def extract_turn_input_items(payload: dict[str, Any]) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []

    instructions = payload.get("instructions")
    if isinstance(instructions, str) and instructions.strip():
        items.append({"type": "text", "text": instructions.strip()})

    input_value = payload.get("input")
    if input_value is None and "messages" in payload:
        input_value = payload.get("messages")

    collect_turn_input_items(input_value, items)
    return items


def collect_turn_input_items(node: Any, out: list[dict[str, Any]]) -> None:
    if node is None:
        return

    if isinstance(node, str):
        text = node.strip()
        if text:
            out.append({"type": "text", "text": text})
        return

    if isinstance(node, list):
        for item in node:
            collect_turn_input_items(item, out)
        return

    if not isinstance(node, dict):
        return

    node_type = node.get("type")
    if isinstance(node_type, str):
        normalized = node_type.strip()

        if normalized in {"text", "input_text", "output_text", "inputText", "outputText"}:
            text = node.get("text")
            if isinstance(text, str) and text.strip():
                out.append({"type": "text", "text": text.strip()})
            return

        if normalized in {"image", "input_image", "image_url", "inputImage", "imageUrl"}:
            image_url = extract_image_url(node)
            if image_url:
                out.append({"type": "image", "url": image_url})
            else:
                image_ref = extract_image_reference_text(node)
                if image_ref:
                    out.append({"type": "text", "text": image_ref})
            return

        if normalized in {"localImage", "local_image"}:
            local_path = normalize_optional_string(node.get("path"))
            if local_path:
                out.append({"type": "localImage", "path": local_path})
            return

        if normalized in {"input_file", "file", "inputFile"}:
            text = extract_input_file_text(node)
            if text:
                out.append({"type": "text", "text": text})
            return

        if normalized == "message":
            collect_turn_input_items(node.get("content"), out)
            return

    # Legacy chat-style message object: {"role":"user","content":...}
    if "role" in node and "content" in node:
        collect_turn_input_items(node.get("content"), out)
        return

    if "content" in node:
        collect_turn_input_items(node.get("content"), out)
        return

    text = node.get("text")
    if isinstance(text, str) and text.strip():
        out.append({"type": "text", "text": text.strip()})


def normalize_optional_string(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    trimmed = value.strip()
    return trimmed if trimmed else None


def extract_image_url(node: dict[str, Any]) -> str | None:
    image_url = node.get("image_url", node.get("imageUrl", node.get("url")))
    if isinstance(image_url, dict):
        candidate = image_url.get("url")
        return normalize_optional_string(candidate)
    return normalize_optional_string(image_url)


def extract_image_reference_text(node: dict[str, Any]) -> str | None:
    file_id = normalize_optional_string(node.get("file_id")) or normalize_optional_string(
        node.get("fileId")
    )
    if not file_id:
        image_url = node.get("image_url", node.get("imageUrl"))
        if isinstance(image_url, dict):
            file_id = normalize_optional_string(image_url.get("file_id")) or normalize_optional_string(
                image_url.get("fileId")
            )
    if not file_id:
        return None
    return f"[image attachment via file_id={file_id}; direct URL not provided]"


def extract_input_file_text(node: dict[str, Any]) -> str | None:
    filename = normalize_optional_string(node.get("filename")) or normalize_optional_string(
        node.get("name")
    )
    mime_type = (
        normalize_optional_string(node.get("mime_type"))
        or normalize_optional_string(node.get("mimeType"))
        or normalize_optional_string(node.get("content_type"))
        or normalize_optional_string(node.get("contentType"))
    )
    file_id = normalize_optional_string(node.get("file_id")) or normalize_optional_string(
        node.get("fileId")
    )

    inline_text = normalize_optional_string(node.get("text"))
    if inline_text:
        return format_file_text_block(filename, mime_type, inline_text)

    file_data = node.get("file_data", node.get("fileData", node.get("data")))
    if isinstance(file_data, str) and file_data.strip():
        decoded = decode_file_data(file_data.strip())
        if decoded is not None:
            textual = decode_bytes_to_text(decoded)
            if textual:
                return format_file_text_block(filename, mime_type, textual)

    label = filename or file_id or "unnamed"
    if file_id:
        return (
            f"Attachment: {label}"
            f"{f' ({mime_type})' if mime_type else ''}"
            f" [file_id={file_id}; content not inlined]"
        )
    return (
        f"Attachment: {label}"
        f"{f' ({mime_type})' if mime_type else ''}"
        " [binary or unsupported file content omitted]"
    )


def format_file_text_block(filename: str | None, mime_type: str | None, text: str) -> str:
    name = filename or "attachment"
    mime = mime_type or "application/octet-stream"
    content = text.strip()
    if not content:
        content = "[empty]"
    max_chars = 20_000
    if len(content) > max_chars:
        content = f"{content[:max_chars]}\n...[truncated]"
    return f"Attachment: {name} ({mime})\n\n{content}"


def decode_file_data(file_data: str) -> bytes | None:
    if file_data.startswith("data:"):
        comma_idx = file_data.find(",")
        if comma_idx == -1:
            return None
        meta = file_data[:comma_idx].lower()
        body = file_data[comma_idx + 1 :]
        if ";base64" in meta:
            try:
                return base64.b64decode(body, validate=False)
            except binascii.Error:
                return None
        return body.encode("utf-8", errors="replace")

    # Try base64 first; fallback to raw text bytes.
    try:
        return base64.b64decode(file_data, validate=True)
    except binascii.Error:
        return file_data.encode("utf-8", errors="replace")


def decode_bytes_to_text(data: bytes) -> str | None:
    if not data:
        return ""
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        try:
            return data.decode("utf-8", errors="replace")
        except Exception:
            return None


def extract_function_tools(payload: dict[str, Any]) -> list[dict[str, Any]]:
    tools = payload.get("tools")
    if tools is None:
        return []
    if not isinstance(tools, list):
        raise ValueError("`tools` must be a list")

    dynamic_tools: list[dict[str, Any]] = []
    seen_names: set[str] = set()
    for tool in tools:
        if not isinstance(tool, dict):
            raise ValueError("each item in `tools` must be an object")
        if tool.get("type") != "function":
            continue

        function_block = tool.get("function")
        if function_block is not None and not isinstance(function_block, dict):
            raise ValueError("function tool `function` must be an object")

        name_raw = tool.get("name")
        if name_raw is None and isinstance(function_block, dict):
            name_raw = function_block.get("name")
        if not isinstance(name_raw, str) or not name_raw.strip():
            raise ValueError("function tool requires non-empty `name`")
        name = name_raw.strip()

        description_raw = tool.get("description")
        if description_raw is None and isinstance(function_block, dict):
            description_raw = function_block.get("description")
        description = description_raw if isinstance(description_raw, str) else ""

        parameters_raw = tool.get("parameters")
        if parameters_raw is None and isinstance(function_block, dict):
            parameters_raw = function_block.get("parameters")
        parameters = parameters_raw if parameters_raw is not None else {"type": "object", "properties": {}}
        if not isinstance(parameters, dict):
            raise ValueError(f"function tool `{name}` `parameters` must be an object")

        if name in seen_names:
            raise ValueError(f"duplicate function tool name: {name}")
        seen_names.add(name)

        dynamic_tools.append(
            {
                "name": name,
                "description": description,
                "inputSchema": parameters,
            }
        )

    return dynamic_tools


def extract_function_call_outputs(payload: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    outputs: dict[str, list[dict[str, Any]]] = {}
    input_value = payload.get("input")
    if input_value is None and "messages" in payload:
        input_value = payload.get("messages")

    collect_function_call_outputs(input_value, outputs)
    return outputs


def collect_function_call_outputs(
    node: Any,
    out: dict[str, list[dict[str, Any]]],
) -> None:
    if node is None:
        return
    if isinstance(node, list):
        for item in node:
            collect_function_call_outputs(item, out)
        return
    if not isinstance(node, dict):
        return

    node_type = node.get("type")
    if node_type in {"function_call_output", "custom_tool_call_output"}:
        call_id_raw = node.get("call_id", node.get("callId"))
        if not isinstance(call_id_raw, str) or not call_id_raw.strip():
            raise ValueError(f"{node_type} requires non-empty `call_id`")
        call_id = call_id_raw.strip()
        out[call_id] = normalize_function_call_output(node.get("output"))
        return

    if "content" in node:
        collect_function_call_outputs(node.get("content"), out)


def normalize_function_call_output(value: Any) -> list[dict[str, Any]]:
    if isinstance(value, str):
        return [{"type": "inputText", "text": value}]

    if isinstance(value, list):
        items: list[dict[str, Any]] = []
        for item in value:
            if not isinstance(item, dict):
                items.append({"type": "inputText", "text": json.dumps(item, ensure_ascii=False)})
                continue
            item_type = item.get("type")
            if item_type in {"input_text", "text", "output_text"}:
                text = item.get("text")
                if isinstance(text, str):
                    items.append({"type": "inputText", "text": text})
                    continue
            if item_type in {"input_image", "image"}:
                image_url = item.get("image_url", item.get("imageUrl", item.get("url")))
                if isinstance(image_url, str) and image_url:
                    items.append({"type": "inputImage", "imageUrl": image_url})
                    continue
            items.append({"type": "inputText", "text": json.dumps(item, ensure_ascii=False)})
        return items or [{"type": "inputText", "text": ""}]

    if value is None:
        return [{"type": "inputText", "text": ""}]

    return [{"type": "inputText", "text": json.dumps(value, ensure_ascii=False)}]


def merge_prompt_with_tool_outputs(
    prompt: str,
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> str:
    lines: list[str] = [
        "以下是客户端执行工具后返回的结果，请基于这些结果继续回答用户问题：",
    ]
    for call_id, content_items in provided_tool_outputs.items():
        text_parts: list[str] = []
        for content in content_items:
            content_type = content.get("type")
            if content_type == "inputText":
                text = content.get("text")
                if isinstance(text, str):
                    text_parts.append(text)
            elif content_type == "inputImage":
                image_url = content.get("imageUrl")
                if isinstance(image_url, str):
                    text_parts.append(f"[image:{image_url}]")
        lines.append(f"- call_id={call_id}: {' '.join(text_parts).strip()}")

    extra = "\n".join(lines).strip()
    if prompt.strip():
        return f"{prompt}\n\n{extra}"
    return extra


def merge_input_items_with_tool_outputs(
    input_items: list[dict[str, Any]],
    provided_tool_outputs: dict[str, list[dict[str, Any]]],
) -> list[dict[str, Any]]:
    if not provided_tool_outputs:
        return input_items

    out = list(input_items)
    extra = merge_prompt_with_tool_outputs("", provided_tool_outputs).strip()
    if extra:
        out.append({"type": "text", "text": extra})
    return out


def ensure_non_empty_turn_input(input_items: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if not input_items:
        return input_items

    has_text = any(
        isinstance(item, dict)
        and item.get("type") == "text"
        and isinstance(item.get("text"), str)
        and item.get("text").strip()
        for item in input_items
    )
    if has_text:
        return input_items

    # Some upstream paths reject all-nontext turns; add a neutral hint to keep
    # image-only requests valid without forcing users to type extra words.
    out = list(input_items)
    out.append({"type": "text", "text": "请根据上传的图片或附件内容进行分析并回答。"})
    return out


def encode_tool_arguments(arguments: Any) -> str:
    if isinstance(arguments, str):
        return arguments
    return json.dumps(arguments, ensure_ascii=False)


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

    # This gateway is Rust-only. Force-disable Codex built-in tool surfaces so
    # the model only sees caller-provided function/MCP tools.
    tools_config = {
        "view_image": False,
        "web_search": {
            "enabled": False,
        },
    }
    config: dict[str, Any] = {
        "tools": tools_config,
        "web_search": "disabled",
        # Always clear thread-level MCP servers from local Codex config so this
        # gateway only uses caller-provided tooling.
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


def normalize_non_empty_string(value: Any, field_name: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"{field_name} must be a string")
    trimmed = value.strip()
    if not trimmed:
        raise ValueError(f"{field_name} must not be empty")
    return trimmed


def validate_string_list(value: Any, field_name: str) -> list[str] | None:
    if value is None:
        return None
    if not isinstance(value, list):
        raise ValueError(f"{field_name} must be a list of strings")

    out: list[str] = []
    for item in value:
        if not isinstance(item, str) or not item.strip():
            raise ValueError(f"{field_name} must contain non-empty strings")
        out.append(item)
    return out


def validate_string_map(value: Any, field_name: str) -> dict[str, str] | None:
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError(f"{field_name} must be an object of string:string pairs")

    out: dict[str, str] = {}
    for key, item in value.items():
        if not isinstance(key, str) or not key:
            raise ValueError(f"{field_name} contains invalid key")
        if not isinstance(item, str):
            raise ValueError(f"{field_name} contains non-string value for key `{key}`")
        out[key] = item
    return out


def collect_text(node: Any, out: list[str]) -> None:
    if node is None:
        return

    if isinstance(node, str):
        out.append(node)
        return

    if isinstance(node, list):
        for item in node:
            collect_text(item, out)
        return

    if not isinstance(node, dict):
        return

    node_type = node.get("type")
    if node_type in {"text", "input_text", "output_text"}:
        text = node.get("text")
        if isinstance(text, str):
            out.append(text)
        return

    if node_type == "message":
        collect_text(node.get("content"), out)
        return

    if "content" in node:
        collect_text(node.get("content"), out)
        return

    text = node.get("text")
    if isinstance(text, str):
        out.append(text)
