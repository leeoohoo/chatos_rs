#!/usr/bin/env python3

import json
import os
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "clients/shared/rust/chatos_client_storage/audit/legacy_access.json"
PROVIDER_OWNED_MACOS_PREFERENCES = (
    "clients/macos/Sources/ChatOSAgentRuntime/AgentTypes.swift",
    "clients/macos/Sources/ChatOSApp/Features/Pet/PetPreferencesStore.swift",
    "clients/macos/Sources/ChatOSApp/Features/GlobalUtilities/GlobalUtilityPreferencesStore.swift",
    "clients/macos/Sources/ChatOSApp/Features/GlobalUtilities/QuickSearch/QuickSearchViewModel.swift",
)


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
    def test_provider_owned_macos_preferences_do_not_regress_to_user_defaults(self) -> None:
        for relative_path in PROVIDER_OWNED_MACOS_PREFERENCES:
            source = (ROOT / relative_path).read_text(errors="replace")
            self.assertNotIn("UserDefaults", source, relative_path)

    def test_native_connector_secrets_do_not_regress_to_plaintext_files(self) -> None:
        source = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeConnectorStorage.swift"
        ).read_text(errors="replace")
        self.assertNotIn('appendingPathComponent("Secrets"', source)
        self.assertNotIn("posixPermissions", source)
        self.assertIn("MacOSKeychainBrokerClient", source)

    def test_native_remote_connections_do_not_restore_legacy_route_fallbacks(self) -> None:
        source = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeRemoteConnectionService.swift"
        ).read_text(errors="replace")
        self.assertNotIn("chatos-swift-native-client", source)
        self.assertNotIn("local-machine", source)
        self.assertNotIn("migrateLegacyRouteIfNeeded", source)
        self.assertNotIn("isLegacyRoute", source)
        self.assertIn("routeStore.requireCurrent()", source)

    def test_agent_runtime_preferences_use_client_settings_repository(self) -> None:
        source = (
            ROOT
            / "clients/macos/Sources/ChatOSConnector/NativeAgentRuntimeSettingsStore.swift"
        ).read_text(errors="replace")
        self.assertIn("NativeLocalClientSettingStore<AgentRuntimePreferences>", source)
        self.assertNotIn("UserDefaults", source)
        self.assertNotIn("FileManager", source)

    def test_macos_terminal_history_uses_selected_client_storage_provider(self) -> None:
        persistent_state = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeConnectorStorage.swift"
        ).read_text(errors="replace")
        terminal_store = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeTerminalHistoryStore.swift"
        ).read_text(errors="replace")
        connector = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService.swift"
        ).read_text(errors="replace")
        self.assertNotIn("commandHistory", persistent_state)
        self.assertNotIn("FileManager", terminal_store)
        self.assertNotIn("UserDefaults", terminal_store)
        self.assertIn("accountSession.client(accountID:", terminal_store)
        self.assertIn("terminalHistoryStore.append", connector)

    def test_macos_connector_runtime_preferences_use_client_settings_repository(self) -> None:
        persistent_state = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeConnectorStorage.swift"
        ).read_text(errors="replace")
        runtime_store = (
            ROOT
            / "clients/macos/Sources/ChatOSConnector/NativeConnectorRuntimePreferencesStore.swift"
        ).read_text(errors="replace")
        connector = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService.swift"
        ).read_text(errors="replace")
        for legacy_field in (
            "developerMode",
            "sandboxEnabled",
            "permissionProfileID",
            "approvalPolicy",
            "approvalReviewer",
            "networkAccess",
            "policyRevision",
        ):
            self.assertNotIn(legacy_field, persistent_state)
        self.assertIn(
            "NativeLocalClientSettingStore<NativeConnectorRuntimePreferences>",
            runtime_store,
        )
        self.assertNotIn("FileManager", runtime_store)
        self.assertNotIn("UserDefaults", runtime_store)
        self.assertIn("runtimePreferencesStore.updateDeveloperMode", connector)
        self.assertIn("runtimePreferencesStore.updateSandbox", connector)

    def test_macos_connector_approval_state_uses_selected_client_storage_provider(self) -> None:
        persistent_state = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeConnectorStorage.swift"
        ).read_text(errors="replace")
        approval_store = (
            ROOT
            / "clients/macos/Sources/ChatOSConnector/NativeConnectorApprovalStore.swift"
        ).read_text(errors="replace")
        connector = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService.swift"
        ).read_text(errors="replace")
        for legacy_field in (
            "approvalMode",
            "commandApprovalModelConfigID",
            "commandApprovalThinkingLevel",
            "approvalHistory",
        ):
            self.assertNotIn(legacy_field, persistent_state)
        self.assertIn(
            "NativeLocalClientSettingStore<NativeConnectorApprovalPreferences>",
            approval_store,
        )
        self.assertIn("client.appendApprovalHistory", approval_store)
        self.assertIn("client.approvalHistoryRecords", approval_store)
        self.assertNotIn("FileManager", approval_store)
        self.assertNotIn("UserDefaults", approval_store)
        self.assertIn("approvalStore.activate", connector)
        self.assertIn("approvalStore.append", connector)

    def test_macos_installed_plugins_use_plugin_state_repository(self) -> None:
        persistent_state = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativeConnectorStorage.swift"
        ).read_text(errors="replace")
        plugin_store = (
            ROOT / "clients/macos/Sources/ChatOSConnector/NativePluginStateStore.swift"
        ).read_text(errors="replace")
        connector_plugins = (
            ROOT
            / "clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+Plugins.swift"
        ).read_text(errors="replace")
        for legacy_field in (
            "installedPluginIDs",
            "installedPluginRecords",
            "pluginPreferences",
        ):
            self.assertNotIn(legacy_field, persistent_state)
            self.assertNotIn(legacy_field, connector_plugins)
        self.assertIn("client.installedPluginRecords", plugin_store)
        self.assertIn("client.putInstalledPlugin", plugin_store)
        self.assertIn("client.deleteInstalledPlugin", plugin_store)
        self.assertNotIn("UserDefaults", plugin_store)
        self.assertNotIn("state.json", connector_plugins)

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
