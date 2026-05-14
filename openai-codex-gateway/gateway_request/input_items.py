from __future__ import annotations

import base64
import binascii
from typing import Any

from .common import normalize_optional_string


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

    if "role" in node and "content" in node:
        collect_turn_input_items(node.get("content"), out)
        return

    if "content" in node:
        collect_turn_input_items(node.get("content"), out)
        return

    text = node.get("text")
    if isinstance(text, str) and text.strip():
        out.append({"type": "text", "text": text.strip()})


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

    out = list(input_items)
    out.append({"type": "text", "text": "请根据上传的图片或附件内容进行分析并回答。"})
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
