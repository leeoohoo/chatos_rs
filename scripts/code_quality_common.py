# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import re
import subprocess
from pathlib import Path
from typing import Iterable


SOURCE_SUFFIXES = {
    ".cjs",
    ".js",
    ".jsx",
    ".mjs",
    ".ps1",
    ".py",
    ".rs",
    ".sh",
    ".ts",
    ".tsx",
}
EXCLUDED_DIRECTORY_NAMES = {
    ".cache",
    ".git",
    ".local",
    ".vite",
    "__pycache__",
    "build",
    "bundled-tools",
    "coverage",
    "dist",
    "docs",
    "fixtures",
    "generated",
    "node_modules",
    "target",
    "target-shared",
    "testdata",
    "tests",
    "vendor",
}
EXCLUDED_ROOTS = {".github"}
TEST_FILE_PATTERNS = (
    re.compile(r"(^|[._-])tests?\.(?:c?m?js|jsx|py|rs|ts|tsx)$", re.IGNORECASE),
    re.compile(r"\.(?:spec|test)\.(?:c?m?js|jsx|ts|tsx)$", re.IGNORECASE),
    re.compile(r"^test_.*\.py$", re.IGNORECASE),
    re.compile(r".*_test\.py$", re.IGNORECASE),
)
DIFF_HUNK_PATTERN = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")
EMPTY_GIT_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"


def normalize_relative_path(path: str | Path) -> str:
    normalized = str(path).replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return normalized.lstrip("/")


def is_production_source(path: str | Path) -> bool:
    relative = normalize_relative_path(path)
    candidate = Path(relative)
    if not candidate.parts or candidate.parts[0] in EXCLUDED_ROOTS:
        return False
    if candidate.suffix.lower() not in SOURCE_SUFFIXES:
        return False
    if any(part.lower() in EXCLUDED_DIRECTORY_NAMES for part in candidate.parts[:-1]):
        return False
    if any(pattern.search(candidate.name) for pattern in TEST_FILE_PATTERNS):
        return False
    return True


def _git_output(root: Path, args: Iterable[str], *, text: bool = True) -> str | bytes:
    text_options = {"encoding": "utf-8", "errors": "replace"} if text else {}
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=text,
        **text_options,
    )
    return result.stdout


def is_git_repository(root: Path) -> bool:
    try:
        _git_output(root, ["rev-parse", "--is-inside-work-tree"])
    except (OSError, subprocess.CalledProcessError):
        return False
    return True


def git_commit_exists(root: Path, revision: str) -> bool:
    if not revision or set(revision) == {"0"}:
        return False
    try:
        _git_output(root, ["cat-file", "-e", f"{revision}^{{commit}}"])
    except (OSError, subprocess.CalledProcessError):
        return False
    return True


def resolve_git_comparison(
    root: Path, requested_base: str | None, requested_head: str | None
) -> tuple[str | None, str | None, str | None]:
    if not requested_base:
        return None, None, None
    if set(requested_base) == {"0"}:
        return (
            EMPTY_GIT_TREE,
            "HEAD",
            "Push event has no previous revision; using the empty tree.",
        )
    if git_commit_exists(root, requested_base):
        head = requested_head if requested_head and git_commit_exists(root, requested_head) else "HEAD"
        return requested_base, head, None
    if git_commit_exists(root, "HEAD^"):
        return "HEAD^", "HEAD", f"Requested diff base {requested_base!r} is unavailable; using HEAD^."
    return (
        EMPTY_GIT_TREE,
        "HEAD",
        f"Requested diff base {requested_base!r} is unavailable; using the empty tree.",
    )


def iter_repository_files(root: Path) -> list[str]:
    if is_git_repository(root):
        raw = _git_output(
            root,
            ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
            text=False,
        )
        assert isinstance(raw, bytes)
        return sorted(
            normalize_relative_path(item.decode("utf-8", errors="surrogateescape"))
            for item in raw.split(b"\0")
            if item
        )

    files = []
    for candidate in root.rglob("*"):
        if candidate.is_file():
            files.append(normalize_relative_path(candidate.relative_to(root)))
    return sorted(files)


def read_source_lines(root: Path, relative_path: str) -> list[str]:
    return (root / relative_path).read_text(encoding="utf-8", errors="replace").splitlines()


def parse_unified_diff_added_lines(diff_text: str) -> dict[str, set[int]]:
    added_lines: dict[str, set[int]] = {}
    current_path: str | None = None
    current_line: int | None = None

    for line in diff_text.splitlines():
        if line.startswith("+++ "):
            raw_path = line[4:].strip()
            if raw_path == "/dev/null":
                current_path = None
            else:
                current_path = normalize_relative_path(
                    raw_path[2:] if raw_path.startswith("b/") else raw_path
                )
            current_line = None
            continue

        hunk_match = DIFF_HUNK_PATTERN.match(line)
        if hunk_match:
            current_line = int(hunk_match.group(1))
            continue

        if current_path is None or current_line is None:
            continue
        if line.startswith("+"):
            added_lines.setdefault(current_path, set()).add(current_line)
            current_line += 1
        elif line.startswith("-"):
            continue
        elif line.startswith("\\"):
            continue
        else:
            current_line += 1

    return added_lines


def collect_added_lines(
    root: Path,
    *,
    base_revision: str | None = None,
    head_revision: str | None = None,
) -> dict[str, set[int]]:
    if not is_git_repository(root):
        return {
            path: set(range(1, len(read_source_lines(root, path)) + 1))
            for path in iter_repository_files(root)
            if is_production_source(path)
        }

    if base_revision:
        target_revision = head_revision or "HEAD"
        diff_args = [
            "-c",
            "core.quotepath=false",
            "diff",
            "--unified=0",
            "--no-ext-diff",
            "--no-color",
            "--diff-filter=ACMR",
            base_revision,
            target_revision,
            "--",
        ]
    else:
        diff_args = [
            "-c",
            "core.quotepath=false",
            "diff",
            "--unified=0",
            "--no-ext-diff",
            "--no-color",
            "--diff-filter=ACMR",
            "HEAD",
            "--",
        ]

    diff_text = _git_output(root, diff_args)
    assert isinstance(diff_text, str)
    added_lines = parse_unified_diff_added_lines(diff_text)

    if not base_revision:
        raw_untracked = _git_output(
            root,
            ["ls-files", "-z", "--others", "--exclude-standard"],
            text=False,
        )
        assert isinstance(raw_untracked, bytes)
        for item in raw_untracked.split(b"\0"):
            if not item:
                continue
            relative_path = normalize_relative_path(
                item.decode("utf-8", errors="surrogateescape")
            )
            if is_production_source(relative_path):
                added_lines[relative_path] = set(
                    range(1, len(read_source_lines(root, relative_path)) + 1)
                )

    return added_lines
