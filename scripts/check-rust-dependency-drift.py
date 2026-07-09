#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import sys
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
MONITORED = ("axum", "tower-http", "mongodb")
SKIP_DIRS = {
    ".git",
    ".local",
    ".cache",
    "node_modules",
    "target",
    "target-debug",
    "target-release",
}

BASELINE: dict[str, dict[str, str]] = {
    "chatos/backend/Cargo.toml": {
        "axum": "0.8",
        "tower-http": "0.6",
        "mongodb": "2.8",
    },
    "local_connector_client/core/Cargo.toml": {
        "axum": "0.8",
        "tower-http": "0.6",
    },
    "local_connector_service/backend/Cargo.toml": {
        "axum": "0.8",
        "tower-http": "0.6",
        "mongodb": "2.8",
    },
    "memory_engine/backend/Cargo.toml": {
        "axum": "0.7",
        "tower-http": "0.5",
        "mongodb": "3",
    },
    "official_website_service/backend/Cargo.toml": {
        "axum": "0.7",
        "tower-http": "0.5",
    },
    "project_management_service/backend/Cargo.toml": {
        "axum": "0.7",
        "tower-http": "0.5",
        "mongodb": "2.8",
    },
    "sandbox_manager_service/backend/Cargo.toml": {
        "axum": "0.7",
        "tower-http": "0.5",
        "mongodb": "2.8",
    },
    "sandbox_manager_service/sandbox_mcp_server/Cargo.toml": {
        "axum": "0.7",
    },
    "task_runner_service/backend/Cargo.toml": {
        "axum": "0.7",
        "tower-http": "0.5",
        "mongodb": "2.8",
    },
    "user_service/backend/Cargo.toml": {
        "axum": "0.7",
        "tower-http": "0.5",
        "mongodb": "2.8",
    },
}

SECTION_RE = re.compile(r"^\s*\[([^\]]+)]\s*$")
DEP_RE = re.compile(r"^\s*([A-Za-z0-9_-]+)\s*=\s*(.+?)\s*$")
STRING_VERSION_RE = re.compile(r'^"([^"]+)"')
TABLE_VERSION_RE = re.compile(r'\bversion\s*=\s*"([^"]+)"')
WORKSPACE_RE = re.compile(r"\bworkspace\s*=\s*true\b")


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def should_skip(path: Path) -> bool:
    return any(part in SKIP_DIRS for part in path.parts)


def is_dependency_section(section: str | None) -> bool:
    if section is None:
        return False
    return section in {"dependencies", "dev-dependencies", "build-dependencies"} or section.endswith(
        ".dependencies"
    )


def dependency_version(raw_value: str) -> str | None:
    value = raw_value.split("#", 1)[0].strip()
    match = STRING_VERSION_RE.search(value)
    if match:
        return match.group(1)
    match = TABLE_VERSION_RE.search(value)
    if match:
        return match.group(1)
    if WORKSPACE_RE.search(value):
        return "workspace"
    return None


def monitored_dependencies(path: Path) -> dict[str, str]:
    found: dict[str, str] = {}
    section: str | None = None
    for line in path.read_text(encoding="utf-8").splitlines():
        section_match = SECTION_RE.match(line)
        if section_match:
            section = section_match.group(1)
            continue
        if not is_dependency_section(section):
            continue
        dep_match = DEP_RE.match(line)
        if not dep_match:
            continue
        name, raw_value = dep_match.groups()
        if name not in MONITORED:
            continue
        version = dependency_version(raw_value)
        if version is not None:
            found[name] = version
    return found


def cargo_manifests() -> list[Path]:
    return sorted(path for path in ROOT.rglob("Cargo.toml") if not should_skip(path))


def main() -> int:
    failures: list[str] = []
    actual_by_path: dict[str, dict[str, str]] = {}

    for manifest in cargo_manifests():
        actual = monitored_dependencies(manifest)
        if actual:
            actual_by_path[rel(manifest)] = actual

    for manifest, expected in BASELINE.items():
        actual = actual_by_path.get(manifest)
        if actual is None:
            failures.append(f"{manifest}: baseline entry is missing from repository")
            continue
        if actual != expected:
            failures.append(f"{manifest}: expected {expected}, found {actual}")

    for manifest, actual in actual_by_path.items():
        if manifest not in BASELINE:
            failures.append(f"{manifest}: monitored dependency set is not in baseline: {actual}")

    print("Rust dependency baseline:")
    for manifest in sorted(actual_by_path):
        versions = ", ".join(f"{name}={version}" for name, version in sorted(actual_by_path[manifest].items()))
        print(f"  {manifest}: {versions}")

    if failures:
        print("\nDependency drift detected:")
        for failure in failures:
            print(f"  {failure}")
        print("\nUpdate RUST_WORKSPACE_DEPENDENCY_BASELINE.zh-CN.md and this script when drift is intentional.")
        return 1

    print("\nNo Rust dependency drift detected.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
