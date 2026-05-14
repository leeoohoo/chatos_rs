from __future__ import annotations

import json
from typing import Any


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
        parameters = (
            parameters_raw
            if parameters_raw is not None
            else {"type": "object", "properties": {}}
        )
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
