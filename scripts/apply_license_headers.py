#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

SPDX = "SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0"
REQUIRED_NOTICE = "Required Notice: Copyright (c) 2025 AI Chat Team"

SOURCE_EXTENSIONS = {
    ".rs",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".mjs",
    ".cjs",
    ".css",
    ".scss",
    ".html",
    ".sh",
    ".ps1",
    ".py",
    ".sql",
    ".toml",
    ".yml",
    ".yaml",
}

SPECIAL_SOURCE_NAMES = {
    ".dockerignore",
    ".env.example",
    "Dockerfile",
    "Makefile",
}

EXCLUDED_DIR_NAMES = {
    ".chatos",
    ".git",
    ".local",
    ".task-runner",
    ".task_runner",
    ".vite",
    "__pycache__",
    "bundled-tools",
    "logs",
    "node_modules",
    "target",
    "target-shared",
}

EXCLUDED_RELATIVE_DIRS = {
    Path("chatos/backend/docs"),
    Path("docs/memory_engine"),
    Path("docs/ponytail"),
}


def is_source_file(path: Path) -> bool:
    return (
        path.name in SPECIAL_SOURCE_NAMES
        or path.name.startswith("Dockerfile.")
        or path.suffix in SOURCE_EXTENSIONS
    )


def is_excluded_relative_dir(path: Path) -> bool:
    return any(path == excluded or excluded in path.parents for excluded in EXCLUDED_RELATIVE_DIRS)


def comment_header(path: Path, newline: str) -> str:
    if path.suffix in {".rs", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"}:
        return f"// {SPDX}{newline}// {REQUIRED_NOTICE}{newline}{newline}"
    if path.suffix == ".sql":
        return f"-- {SPDX}{newline}-- {REQUIRED_NOTICE}{newline}{newline}"
    if path.suffix in {".css", ".scss"}:
        return f"/* {SPDX}{newline} * {REQUIRED_NOTICE}{newline} */{newline}{newline}"
    if path.suffix == ".html":
        return f"<!--{newline}{SPDX}{newline}{REQUIRED_NOTICE}{newline}-->{newline}{newline}"
    return f"# {SPDX}{newline}# {REQUIRED_NOTICE}{newline}{newline}"


def insertion_offset(text: str, path: Path) -> int:
    lines = text.splitlines(keepends=True)
    if not lines:
        return 0

    first_line = lines[0]
    normalized = first_line.strip().lower()
    if first_line.startswith("#!"):
        return len(first_line)
    if path.name == "Dockerfile" or path.name.startswith("Dockerfile."):
        if normalized.startswith("# syntax="):
            return len(first_line)
    if path.suffix == ".html" and normalized.startswith("<!doctype"):
        return len(first_line)
    return 0


def source_files(root: Path) -> list[Path]:
    files: list[Path] = []

    for dirpath, dirnames, filenames in os.walk(root):
        current_dir = Path(dirpath)
        relative_dir = current_dir.relative_to(root)

        kept_dirnames: list[str] = []
        for dirname in sorted(dirnames):
            relative_child = relative_dir / dirname
            if dirname in EXCLUDED_DIR_NAMES or is_excluded_relative_dir(relative_child):
                continue
            kept_dirnames.append(dirname)
        dirnames[:] = kept_dirnames

        for filename in sorted(filenames):
            path = current_dir / filename
            if is_source_file(path):
                files.append(path)

    return files


def has_required_notice(text: str) -> bool:
    return REQUIRED_NOTICE in "\n".join(text.splitlines()[:20])


def add_header(path: Path) -> bool:
    data = path.read_bytes()
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return False
    if has_required_notice(text):
        return False

    newline = "\r\n" if "\r\n" in text and text.count("\r\n") >= max(1, text.count("\n") // 2) else "\n"
    bom = ""
    body = text
    if body.startswith("\ufeff"):
        bom = "\ufeff"
        body = body[1:]

    offset = insertion_offset(body, path)
    header = comment_header(path, newline)
    if not body.strip():
        header = header.rstrip() + newline
        next_text = bom + header
    else:
        next_text = bom + body[:offset] + header + body[offset:]
    path.write_text(next_text, encoding="utf-8", newline="")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description="Check or add Chat OS license headers.")
    parser.add_argument("--write", action="store_true", help="Add missing license headers.")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    missing: list[Path] = []
    updated: list[Path] = []

    for path in source_files(root):
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        if has_required_notice(text):
            continue
        if args.write and add_header(path):
            updated.append(path.relative_to(root))
        else:
            missing.append(path.relative_to(root))

    if args.write:
        print(f"updated license headers: {len(updated)}")
        return 0

    if missing:
        print(f"missing license headers: {len(missing)}")
        for path in missing[:50]:
            print(path)
        if len(missing) > 50:
            print(f"... {len(missing) - 50} more")
        return 1

    print("license headers ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
