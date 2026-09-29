#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

"""Reject Local Agent Host dependencies on server runtimes being replaced."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
LOCAL_AGENT_ROOT = REPOSITORY_ROOT / "clients" / "local_agent_host"
FORBIDDEN_ROOTS = {"chatos", "mcp_management_service", "task_runner_service"}
LOCAL_AGENT_PACKAGES = {
    "chatos_agent_profiles",
    "chatos_client_storage",
    "chatos_local_agent_host",
    "chatos_local_agent_ports",
    "chatos_local_agent_protocol",
    "chatos_local_agent_runtime",
}


def relative_manifest(package: dict[str, object]) -> Path | None:
    manifest = Path(str(package["manifest_path"])).resolve()
    try:
        return manifest.relative_to(REPOSITORY_ROOT)
    except ValueError:
        return None


def main() -> int:
    completed = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(completed.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    host_ids = [
        package_id
        for package_id, package in packages.items()
        if package["name"] == "chatos_local_agent_host"
    ]
    if len(host_ids) != 1:
        print("expected exactly one chatos_local_agent_host package", file=sys.stderr)
        return 1

    reachable: set[str] = set()
    pending = host_ids[:]
    while pending:
        package_id = pending.pop()
        if package_id in reachable:
            continue
        reachable.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes[package_id]["deps"])

    violations: list[str] = []
    for package_id in sorted(reachable):
        package = packages[package_id]
        relative = relative_manifest(package)
        if relative is not None and relative.parts and relative.parts[0] in FORBIDDEN_ROOTS:
            violations.append(f"forbidden server dependency: {package['name']} ({relative})")

    local_root = LOCAL_AGENT_ROOT.resolve()
    for package in packages.values():
        if package["name"] not in LOCAL_AGENT_PACKAGES:
            continue
        manifest = Path(str(package["manifest_path"])).resolve()
        if not manifest.is_relative_to(local_root):
            violations.append(
                f"Local Agent package escaped clients/local_agent_host: "
                f"{package['name']} ({manifest.relative_to(REPOSITORY_ROOT)})"
            )

    runtime_ids = [
        package_id
        for package_id, package in packages.items()
        if package["name"] == "chatos_local_agent_runtime"
    ]
    if len(runtime_ids) != 1:
        violations.append("expected exactly one chatos_local_agent_runtime package")
    else:
        runtime_dependencies = {
            packages[dependency["pkg"]]["name"]
            for dependency in nodes[runtime_ids[0]]["deps"]
            if any(
                dependency_kind["kind"] in (None, "normal")
                for dependency_kind in dependency["dep_kinds"]
            )
        }
        if "chatos_client_storage" in runtime_dependencies:
            violations.append("application runtime directly depends on SQLite infrastructure")

    if violations:
        print("Local Agent Host dependency boundary violations:", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        return 1

    print(
        "Local Agent Host boundary: self-contained product packages, no chatos/, "
        "mcp_management_service/, or task_runner_service/ dependencies"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
