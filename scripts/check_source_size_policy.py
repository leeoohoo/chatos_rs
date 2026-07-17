#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import argparse
import csv
import os
import sys
from dataclasses import dataclass
from datetime import date
from pathlib import Path

from code_quality_common import (
    collect_added_lines,
    is_production_source,
    iter_repository_files,
    normalize_relative_path,
    read_source_lines,
    resolve_git_comparison,
)


DEFAULT_WARN_LINES = 500
DEFAULT_HARD_LINES = 800


@dataclass(frozen=True)
class AllowlistEntry:
    path: str
    max_lines: int
    expires_on: date
    reason: str


def parse_allowlist(path: Path) -> tuple[dict[str, AllowlistEntry], list[str]]:
    entries: dict[str, AllowlistEntry] = {}
    errors: list[str] = []
    if not path.is_file():
        return {}, [f"Source-size allowlist not found: {path}"]

    with path.open(encoding="utf-8", newline="") as handle:
        for line_number, row in enumerate(csv.reader(handle, delimiter="\t"), start=1):
            if not row or not row[0].strip() or row[0].lstrip().startswith("#"):
                continue
            if len(row) < 4:
                errors.append(
                    f"{path}:{line_number}: expected path, max_lines, expires_on, and reason"
                )
                continue
            relative_path = normalize_relative_path(row[0].strip())
            try:
                max_lines = int(row[1])
            except ValueError:
                errors.append(f"{path}:{line_number}: invalid max_lines {row[1]!r}")
                continue
            try:
                expires_on = date.fromisoformat(row[2].strip())
            except ValueError:
                errors.append(f"{path}:{line_number}: invalid expires_on {row[2]!r}")
                continue
            reason = "\t".join(row[3:]).strip()
            if max_lines < 1:
                errors.append(f"{path}:{line_number}: max_lines must be positive")
                continue
            if not reason:
                errors.append(f"{path}:{line_number}: reason must not be empty")
                continue
            if relative_path in entries:
                errors.append(f"{path}:{line_number}: duplicate path {relative_path}")
                continue
            entries[relative_path] = AllowlistEntry(
                path=relative_path,
                max_lines=max_lines,
                expires_on=expires_on,
                reason=reason,
            )
    return entries, errors


def evaluate_source_sizes(
    line_counts: dict[str, int],
    allowlist: dict[str, AllowlistEntry],
    *,
    warn_lines: int,
    hard_lines: int,
    today: date,
    warning_paths: set[str] | None = None,
) -> tuple[list[str], list[str]]:
    warnings: list[str] = []
    errors: list[str] = []

    for relative_path, line_count in sorted(
        line_counts.items(), key=lambda item: (-item[1], item[0])
    ):
        entry = allowlist.get(relative_path)
        if line_count > warn_lines and (
            warning_paths is None or relative_path in warning_paths
        ):
            warnings.append(
                f"{relative_path}: {line_count} lines (warning threshold {warn_lines})"
            )
        if line_count <= hard_lines:
            if entry:
                errors.append(
                    f"{relative_path}: stale allowlist entry; file is now {line_count} lines "
                    f"(hard limit {hard_lines})"
                )
            continue
        if not entry:
            errors.append(
                f"{relative_path}: {line_count} lines exceeds hard limit {hard_lines} "
                "without an allowlist entry"
            )
            continue
        if entry.expires_on < today:
            errors.append(
                f"{relative_path}: allowlist expired on {entry.expires_on.isoformat()}"
            )
        if line_count > entry.max_lines:
            errors.append(
                f"{relative_path}: {line_count} lines exceeds allowlist budget {entry.max_lines}"
            )

    for relative_path, entry in sorted(allowlist.items()):
        if relative_path not in line_counts:
            errors.append(f"{relative_path}: allowlist entry does not reference production source")
        elif entry.expires_on < today and line_counts[relative_path] <= hard_lines:
            # The stale-entry error above is more actionable than a second expiry error.
            continue

    return warnings, errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Warn above 500 lines and require expiring allowlist entries above 800 lines."
    )
    parser.add_argument("--warn-lines", type=int, default=DEFAULT_WARN_LINES)
    parser.add_argument("--hard-lines", type=int, default=DEFAULT_HARD_LINES)
    parser.add_argument(
        "--allowlist",
        type=Path,
        default=Path(__file__).with_name("source-size-allowlist.tsv"),
    )
    parser.add_argument(
        "--today",
        default=os.environ.get("SOURCE_SIZE_POLICY_TODAY"),
        help="ISO date override used by deterministic tests.",
    )
    parser.add_argument("--base", default=os.environ.get("CODE_QUALITY_DIFF_BASE"))
    parser.add_argument("--head", default=os.environ.get("CODE_QUALITY_DIFF_HEAD"))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.warn_lines < 1 or args.hard_lines <= args.warn_lines:
        print("hard-lines must be greater than warn-lines, and both must be positive", file=sys.stderr)
        return 2
    try:
        today = date.fromisoformat(args.today) if args.today else date.today()
    except ValueError:
        print(f"Invalid --today value: {args.today!r}", file=sys.stderr)
        return 2

    root = Path(__file__).resolve().parent.parent
    base_revision, head_revision, comparison_warning = resolve_git_comparison(
        root, args.base, args.head
    )
    if comparison_warning:
        print(f"Warning: {comparison_warning}")
    line_counts = {
        relative_path: len(read_source_lines(root, relative_path))
        for relative_path in iter_repository_files(root)
        if is_production_source(relative_path) and (root / relative_path).is_file()
    }
    added_lines = collect_added_lines(
        root,
        base_revision=base_revision,
        head_revision=head_revision,
    )
    new_file_paths = {
        relative_path
        for relative_path, line_numbers in added_lines.items()
        if relative_path in line_counts
        and line_numbers == set(range(1, line_counts[relative_path] + 1))
    }
    allowlist, parse_errors = parse_allowlist(args.allowlist)
    warnings, policy_errors = evaluate_source_sizes(
        line_counts,
        allowlist,
        warn_lines=args.warn_lines,
        hard_lines=args.hard_lines,
        today=today,
        warning_paths=new_file_paths,
    )

    print(
        f"Production source size policy: {len(line_counts)} files, "
        f"warn for new files > {args.warn_lines}, require allowlist > {args.hard_lines}"
    )
    if warnings:
        print("Warnings:")
        for warning in warnings:
            print(f"  - {warning}")
    else:
        print("Warnings: none")

    errors = [*parse_errors, *policy_errors]
    if errors:
        print("Violations:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("Violations: none")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
