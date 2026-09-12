#!/usr/bin/env python3

import json
import os
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "clients/shared/rust/chatos_client_storage/audit/legacy_access.json"


def load_audit() -> dict:
    return json.loads(AUDIT.read_text())


def direct_database_driver_files() -> set[str]:
    files: set[Path] = set()
    macos_sources = ROOT / "clients/macos/Sources"
    for path in macos_sources.rglob("*.swift"):
        if "import SQLite3" in path.read_text(errors="replace"):
            files.add(path)

    windows_sources = ROOT / "clients/windows/src"
    for path in windows_sources.rglob("*.cs"):
        if path.name == "LocalStateDatabase.cs" or path.name.startswith("Sqlite"):
            files.add(path)

    return {path.relative_to(ROOT).as_posix() for path in files}


class ClientStorageBoundaryTests(unittest.TestCase):
    def test_every_direct_database_driver_is_in_the_migration_inventory(self) -> None:
        audit = load_audit()
        inventoried = {
            entry["path"]
            for entry in audit["entries"]
            if entry["access_kind"] == "direct_database_driver"
        }
        self.assertSetEqual(direct_database_driver_files(), inventoried)

    def test_inventory_entries_are_unique_real_and_actionable(self) -> None:
        audit = load_audit()
        entries = audit["entries"]
        paths = [entry["path"] for entry in entries]
        self.assertEqual(len(paths), len(set(paths)))
        for entry in entries:
            self.assertTrue((ROOT / entry["path"]).is_file(), entry["path"])
            self.assertIn(entry["platform"], {"macos", "windows"})
            self.assertTrue(entry["target"].strip(), entry["path"])
            self.assertEqual(entry["status"], "pending", entry["path"])

    def test_exemptions_are_only_os_ui_preferences_or_secure_stores(self) -> None:
        audit = load_audit()
        entry_paths = {entry["path"] for entry in audit["entries"]}
        exemptions = (
            audit["os_ui_preference_exemptions"]
            + audit["secure_store_exemptions"]
        )
        self.assertEqual(len(exemptions), len(set(exemptions)))
        self.assertTrue(entry_paths.isdisjoint(exemptions))
        for path in exemptions:
            self.assertTrue((ROOT / path).is_file(), path)

    def test_strict_gate_rejects_remaining_legacy_access(self) -> None:
        if os.environ.get("CHATOS_CLIENT_STORAGE_STRICT") != "1":
            self.skipTest("strict gate is enabled only for final Work Package A acceptance")
        pending = [
            entry["path"]
            for entry in load_audit()["entries"]
            if entry["status"] == "pending"
        ]
        self.assertEqual(pending, [], "legacy client storage access remains")


if __name__ == "__main__":
    unittest.main()
