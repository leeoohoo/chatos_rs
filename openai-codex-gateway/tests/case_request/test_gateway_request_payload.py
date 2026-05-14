#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_request.payload import (  # noqa: E402
    collect_text,
    ensure_non_empty_turn_input,
    extract_function_call_outputs,
    extract_request_config_overrides,
    extract_request_instructions,
    extract_turn_input_items,
)


class GatewayRequestPayloadTest(unittest.TestCase):
    def test_extract_turn_input_items_handles_nested_files_and_messages(self) -> None:
        payload = {
            "instructions": "请先阅读附件",
            "input": [
                {
                    "type": "message",
                    "content": [
                        {"type": "input_text", "text": " 你好 "},
                        {
                            "type": "input_file",
                            "filename": "note.txt",
                            "mime_type": "text/plain",
                            "file_data": "data:text/plain;base64,aGVsbG8gZ2F0ZXdheQ==",
                        },
                    ],
                },
                {
                    "type": "local_image",
                    "path": " /tmp/example.png ",
                },
            ],
        }

        items = extract_turn_input_items(payload)

        self.assertEqual(items[0], {"type": "text", "text": "你好"})
        self.assertEqual(items[1]["type"], "text")
        self.assertIn("Attachment: note.txt (text/plain)", items[1]["text"])
        self.assertIn("hello gateway", items[1]["text"])
        self.assertEqual(items[2], {"type": "localImage", "path": "/tmp/example.png"})

    def test_extract_request_instructions_returns_trimmed_string(self) -> None:
        self.assertEqual(
            extract_request_instructions({"instructions": "  请先阅读附件  "}),
            "请先阅读附件",
        )
        self.assertIsNone(extract_request_instructions({"instructions": "   "}))

    def test_ensure_non_empty_turn_input_adds_hint_for_image_only_turn(self) -> None:
        items = ensure_non_empty_turn_input(
            [
                {
                    "type": "image",
                    "url": "https://example.com/image.png",
                }
            ]
        )

        self.assertEqual(items[0]["type"], "image")
        self.assertEqual(items[-1]["type"], "text")
        self.assertIn("请根据上传的图片或附件内容进行分析并回答", items[-1]["text"])

    def test_extract_function_call_outputs_normalizes_mixed_content(self) -> None:
        payload = {
            "input": [
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": [
                        {"type": "text", "text": "done"},
                        {"type": "image", "url": "https://example.com/tool.png"},
                        {"unexpected": True},
                    ],
                }
            ]
        }

        outputs = extract_function_call_outputs(payload)

        self.assertEqual(outputs["call_1"][0], {"type": "inputText", "text": "done"})
        self.assertEqual(
            outputs["call_1"][1],
            {"type": "inputImage", "imageUrl": "https://example.com/tool.png"},
        )
        self.assertEqual(
            outputs["call_1"][2],
            {"type": "inputText", "text": '{"unexpected": true}'},
        )

    def test_extract_request_config_overrides_supports_stdio_mcp_tools(self) -> None:
        config = extract_request_config_overrides(
            {
                "tools": [
                    {
                        "type": "mcp",
                        "server_label": "workspace",
                        "command": "node",
                        "args": ["server.js"],
                        "env": {"TOKEN": "secret"},
                        "env_vars": ["PATH"],
                        "cwd": "/tmp/project",
                        "enabled_tools": ["grep_code"],
                        "disabled_tools": ["write_file"],
                        "required": True,
                    }
                ]
            }
        )

        self.assertIsNotNone(config)
        assert config is not None
        self.assertEqual(config["tools"]["view_image"], False)
        self.assertEqual(
            config["mcp_servers"]["workspace"],
            {
                "command": "node",
                "args": ["server.js"],
                "env": {"TOKEN": "secret"},
                "env_vars": ["PATH"],
                "cwd": "/tmp/project",
                "enabled_tools": ["grep_code"],
                "disabled_tools": ["write_file"],
                "required": True,
            },
        )

    def test_extract_request_config_overrides_rejects_inline_bearer_token(self) -> None:
        with self.assertRaises(ValueError):
            extract_request_config_overrides(
                {
                    "tools": [
                        {
                            "type": "mcp",
                            "server_label": "workspace",
                            "server_url": "http://127.0.0.1:9000/mcp",
                            "bearer_token": "secret",
                        }
                    ]
                }
            )

    def test_collect_text_flattens_nested_message_content(self) -> None:
        collected: list[str] = []
        collect_text(
            {
                "type": "message",
                "content": [
                    {"type": "text", "text": "alpha"},
                    {
                        "content": [
                            {"type": "input_text", "text": "beta"},
                            "gamma",
                        ]
                    },
                ],
            },
            collected,
        )

        self.assertEqual(collected, ["alpha", "beta", "gamma"])


if __name__ == "__main__":
    unittest.main()
