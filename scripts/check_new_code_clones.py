#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import argparse
import hashlib
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path

from code_quality_common import (
    collect_added_lines,
    is_production_source,
    iter_repository_files,
    read_source_lines,
    resolve_git_comparison,
)


DEFAULT_MIN_LINES = 25
COMMENT_ONLY_PATTERNS = (
    re.compile(r"^//"),
    re.compile(r"^/\*"),
    re.compile(r"^\*"),
    re.compile(r"^\*/$"),
    re.compile(r"^#(?:\s|!|$)"),
    re.compile(r"^--(?:\s|$)"),
)


@dataclass(frozen=True)
class SourceSequence:
    path: str
    normalized_lines: tuple[str, ...]
    physical_lines: tuple[int, ...]


@dataclass(frozen=True)
class CloneViolation:
    first_path: str
    first_start: int
    first_end: int
    second_path: str
    second_start: int
    second_end: int
    significant_lines: int


def normalize_code_line(line: str) -> str | None:
    stripped = line.strip()
    if not stripped or any(pattern.match(stripped) for pattern in COMMENT_ONLY_PATTERNS):
        return None
    return re.sub(r"\s+", "", stripped)


def rust_cfg_test_lines(lines: list[str]) -> set[int]:
    skipped: set[int] = set()
    pending_test_item = False
    skipping_item = False
    brace_depth = 0
    saw_open_brace = False

    for line_number, line in enumerate(lines, start=1):
        stripped = line.strip()
        if not skipping_item and stripped.startswith("#[cfg(test)]"):
            pending_test_item = True
            skipped.add(line_number)
            continue
        if pending_test_item and not skipping_item:
            skipped.add(line_number)
            if stripped.startswith("#["):
                continue
            if ";" in stripped and "{" not in stripped:
                pending_test_item = False
                continue
            skipping_item = True
            pending_test_item = False

        if skipping_item:
            skipped.add(line_number)
            brace_depth += line.count("{") - line.count("}")
            saw_open_brace = saw_open_brace or "{" in line
            if saw_open_brace and brace_depth <= 0:
                skipping_item = False
                brace_depth = 0
                saw_open_brace = False

    return skipped


def build_source_sequence(path: str, lines: list[str]) -> SourceSequence:
    normalized_lines: list[str] = []
    physical_lines: list[int] = []
    skipped_lines = rust_cfg_test_lines(lines) if path.endswith(".rs") else set()
    for line_number, line in enumerate(lines, start=1):
        if line_number in skipped_lines:
            continue
        normalized = normalize_code_line(line)
        if normalized is None:
            continue
        normalized_lines.append(normalized)
        physical_lines.append(line_number)
    return SourceSequence(path, tuple(normalized_lines), tuple(physical_lines))


def window_digest(lines: tuple[str, ...], start: int, length: int) -> bytes:
    digest = hashlib.blake2b(digest_size=16)
    for line in lines[start : start + length]:
        digest.update(line.encode("utf-8", errors="replace"))
        digest.update(b"\0")
    return digest.digest()


def _window_touches_added_lines(
    sequence: SourceSequence,
    start: int,
    length: int,
    added_lines: set[int],
) -> bool:
    return any(
        line_number in added_lines
        for line_number in sequence.physical_lines[start : start + length]
    )


def _extend_clone(
    first: SourceSequence,
    first_index: int,
    second: SourceSequence,
    second_index: int,
    minimum_lines: int,
) -> tuple[int, int, int]:
    first_start = first_index
    second_start = second_index
    while (
        first_start > 0
        and second_start > 0
        and first.normalized_lines[first_start - 1]
        == second.normalized_lines[second_start - 1]
    ):
        first_start -= 1
        second_start -= 1

    length = minimum_lines
    while (
        first_start + length < len(first.normalized_lines)
        and second_start + length < len(second.normalized_lines)
        and first.normalized_lines[first_start + length]
        == second.normalized_lines[second_start + length]
    ):
        length += 1
    return first_start, second_start, length


def find_clone_violations(
    sequences: dict[str, SourceSequence],
    added_lines: dict[str, set[int]],
    *,
    minimum_lines: int,
) -> list[CloneViolation]:
    changed_windows: dict[bytes, list[tuple[str, int]]] = {}
    for path, changed_line_numbers in added_lines.items():
        sequence = sequences.get(path)
        if not sequence or len(sequence.normalized_lines) < minimum_lines:
            continue
        for start in range(len(sequence.normalized_lines) - minimum_lines + 1):
            if not _window_touches_added_lines(
                sequence, start, minimum_lines, changed_line_numbers
            ):
                continue
            digest = window_digest(sequence.normalized_lines, start, minimum_lines)
            changed_windows.setdefault(digest, []).append((path, start))

    if not changed_windows:
        return []

    matching_windows: dict[bytes, list[tuple[str, int]]] = {
        digest: [] for digest in changed_windows
    }
    for path, sequence in sequences.items():
        if len(sequence.normalized_lines) < minimum_lines:
            continue
        for start in range(len(sequence.normalized_lines) - minimum_lines + 1):
            digest = window_digest(sequence.normalized_lines, start, minimum_lines)
            if digest in matching_windows:
                matching_windows[digest].append((path, start))

    violations: dict[tuple[str, int, str, int], CloneViolation] = {}
    for digest, changed_occurrences in changed_windows.items():
        for changed_path, changed_index in changed_occurrences:
            changed_sequence = sequences[changed_path]
            changed_window = changed_sequence.normalized_lines[
                changed_index : changed_index + minimum_lines
            ]
            for match_path, match_index in matching_windows[digest]:
                if changed_path == match_path and changed_index == match_index:
                    continue
                match_sequence = sequences[match_path]
                if (
                    match_sequence.normalized_lines[
                        match_index : match_index + minimum_lines
                    ]
                    != changed_window
                ):
                    continue
                if changed_path == match_path and abs(changed_index - match_index) < minimum_lines:
                    continue

                first_start, second_start, clone_length = _extend_clone(
                    changed_sequence,
                    changed_index,
                    match_sequence,
                    match_index,
                    minimum_lines,
                )
                if changed_path == match_path:
                    first_end_index = first_start + clone_length - 1
                    second_end_index = second_start + clone_length - 1
                    if not (
                        first_end_index < second_start or second_end_index < first_start
                    ):
                        continue

                endpoints = sorted(
                    [(changed_path, first_start), (match_path, second_start)]
                )
                key = (endpoints[0][0], endpoints[0][1], endpoints[1][0], endpoints[1][1])
                if key in violations:
                    continue

                first_path, first_index = endpoints[0]
                second_path, second_index = endpoints[1]
                first_sequence = sequences[first_path]
                second_sequence = sequences[second_path]
                first_added = added_lines.get(first_path, set())
                second_added = added_lines.get(second_path, set())
                if not (
                    _window_touches_added_lines(
                        first_sequence, first_index, clone_length, first_added
                    )
                    or _window_touches_added_lines(
                        second_sequence, second_index, clone_length, second_added
                    )
                ):
                    continue

                violations[key] = CloneViolation(
                    first_path=first_path,
                    first_start=first_sequence.physical_lines[first_index],
                    first_end=first_sequence.physical_lines[first_index + clone_length - 1],
                    second_path=second_path,
                    second_start=second_sequence.physical_lines[second_index],
                    second_end=second_sequence.physical_lines[second_index + clone_length - 1],
                    significant_lines=clone_length,
                )

    return sorted(
        violations.values(),
        key=lambda item: (
            -item.significant_lines,
            item.first_path,
            item.first_start,
            item.second_path,
            item.second_start,
        ),
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Reject newly added exact production-code clones of 25 or more significant lines."
    )
    parser.add_argument("--min-lines", type=int, default=DEFAULT_MIN_LINES)
    parser.add_argument("--base", default=os.environ.get("CODE_QUALITY_DIFF_BASE"))
    parser.add_argument("--head", default=os.environ.get("CODE_QUALITY_DIFF_HEAD"))
    parser.add_argument("--max-findings", type=int, default=20)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.min_lines < 2 or args.max_findings < 1:
        print("min-lines must be at least 2 and max-findings must be positive", file=sys.stderr)
        return 2

    root = Path(__file__).resolve().parent.parent
    base_revision, head_revision, comparison_warning = resolve_git_comparison(
        root, args.base, args.head
    )
    if comparison_warning:
        print(f"Warning: {comparison_warning}")

    added_lines = collect_added_lines(
        root,
        base_revision=base_revision,
        head_revision=head_revision,
    )
    added_lines = {
        path: line_numbers
        for path, line_numbers in added_lines.items()
        if line_numbers and is_production_source(path) and (root / path).is_file()
    }
    if not added_lines:
        print("New production code clone policy: no added production lines to inspect.")
        return 0

    sequences = {
        path: build_source_sequence(path, read_source_lines(root, path))
        for path in iter_repository_files(root)
        if is_production_source(path) and (root / path).is_file()
    }
    violations = find_clone_violations(
        sequences,
        added_lines,
        minimum_lines=args.min_lines,
    )

    comparison = (
        f"{base_revision}..{head_revision}" if base_revision else "HEAD..working-tree"
    )
    print(
        f"New production code clone policy: {len(added_lines)} changed files, "
        f"minimum {args.min_lines} significant lines, comparison {comparison}"
    )
    if not violations:
        print("Violations: none")
        return 0

    print("Violations:", file=sys.stderr)
    for violation in violations[: args.max_findings]:
        print(
            "  - "
            f"{violation.first_path}:{violation.first_start}-{violation.first_end} duplicates "
            f"{violation.second_path}:{violation.second_start}-{violation.second_end} "
            f"({violation.significant_lines} significant lines)",
            file=sys.stderr,
        )
    if len(violations) > args.max_findings:
        print(
            f"  - ... {len(violations) - args.max_findings} additional findings omitted",
            file=sys.stderr,
        )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
