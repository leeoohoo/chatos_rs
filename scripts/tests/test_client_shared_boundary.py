#!/usr/bin/env python3

from pathlib import Path
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
SHARED_RUST = ROOT / "clients/shared/rust"

# This inventory must only shrink. It makes the remaining architectural debt
# explicit and prevents a client crate from silently acquiring another legacy
# top-level crate dependency while the old implementation is being removed.
EXPECTED_LEGACY_EDGES = {
    ("chatos_local_agent_runtime", "memory_engine_sdk"),
    ("chatos_local_agent_host", "chatos_mcp_runtime"),
    ("chatos_local_agent_host", "chatos_plugin_management_sdk"),
    ("chatos_local_agent_host", "memory_engine_sdk"),
}


def legacy_dependency_edges() -> set[tuple[str, str]]:
    edges: set[tuple[str, str]] = set()
    for manifest in SHARED_RUST.glob("*/Cargo.toml"):
        data = tomllib.loads(manifest.read_text())
        package = data["package"]["name"]
        for dependency, value in data.get("dependencies", {}).items():
            if not isinstance(value, dict):
                continue
            path = value.get("path")
            if path and (manifest.parent / path).resolve().is_relative_to(ROOT / "crates"):
                edges.add((package, dependency))
    return edges


class ClientSharedBoundaryTests(unittest.TestCase):
    def test_legacy_top_level_crate_edges_only_shrink(self) -> None:
        self.assertSetEqual(legacy_dependency_edges(), EXPECTED_LEGACY_EDGES)

    def test_model_gateway_has_no_server_runtime_dependency(self) -> None:
        manifest = tomllib.loads(
            (SHARED_RUST / "chatos_local_agent_runtime/Cargo.toml").read_text()
        )
        self.assertNotIn("chatos_service_runtime", manifest["dependencies"])
        self.assertIn("chatos_client_http", manifest["dependencies"])


if __name__ == "__main__":
    unittest.main()
