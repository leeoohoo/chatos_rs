# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class LocalAgentStartupCIPolicyTests(unittest.TestCase):
    def test_native_ci_builds_real_host_and_requires_integration_tests(self):
        script = (ROOT / "scripts/verify-repository.sh").read_text()
        mac = script.split('Darwin)', 1)[1].split('MINGW*', 1)[0]
        self.assertLess(mac.index('build -p chatos_local_agent_host'), mac.index('run_swiftpm test'))
        self.assertIn('test -p chatos_client_storage --lib', mac)
        self.assertIn('CHATOS_REQUIRE_NATIVE_HOST_TESTS=1', mac)
        self.assertIn('test -x "$CHATOS_LOCAL_AGENT_HOST_PATH"', mac)
        self.assertLess(mac.index('build --manifest-path plugins/browser/Cargo.toml -p browser-cdp-cli'), mac.index('run_swiftpm test'))
        self.assertIn('CHATOS_BROWSER_CDP_CATALOG_EXECUTABLE=', mac)
        self.assertIn('CHATOS_BROWSER_CDP_CATALOG_PACKAGE_ROOT=', mac)
        browser_tests = (ROOT / "clients/macos/Tests/ChatOSConnectorTests/NativeBrowserPluginUsageContractTests.swift").read_text()
        self.assertIn('Required Browser catalog executable/package is missing from native CI', browser_tests)
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn('Setup Rust for macOS Local Agent startup tests', workflow)

    def test_missing_required_host_cannot_silently_skip_native_tests(self):
        tests = (ROOT / "clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentHostLifecycleTests.swift").read_text()
        self.assertEqual(tests.count('CHATOS_REQUIRE_NATIVE_HOST_TESTS'), tests.count('throw XCTSkip('))
        self.assertIn('XCTFail("Required native Host integration executable is missing")', tests)
        self.assertIn('workspace.fetchWorkspaceRelations()', tests)

    def test_pet_generic_cancel_failure_is_visible_not_just_task_list_failure(self):
        view = (ROOT / "clients/macos/Sources/ChatOSApp/Features/Pet/PetMessageView+ActivitySections.swift").read_text()
        error_display = view.index('if let error = cancellationErrors[activity.id]')
        cancel_button = view.index('Text("取消任务")')
        self.assertLess(error_display, cancel_button)
        self.assertIn('Text(error)', view[error_display:cancel_button])
        tests = (ROOT / "clients/local_agent_host/layers/infrastructure/database/src/task_commands_tests.rs").read_text()
        self.assertIn('cancelling_review_task_abandons_workflow_but_preserves_uncertain_run', tests)

    def test_restore_and_online_refresh_share_historical_credential_loader(self):
        bootstrap = (ROOT / "clients/macos/Sources/ChatOSConnector/NativeLocalAgentBootstrap.swift").read_text()
        self.assertEqual(bootstrap.count('NativeLocalAgentCredentialEnvironment.restoredModels('), 2)
        self.assertIn('refreshedCredentialVariables.insert(variable).inserted', bootstrap)
        tests = (ROOT / "clients/macos/Tests/ChatOSConnectorTests/NativeLocalAgentCredentialEnvironmentTests.swift").read_text()
        self.assertIn('currentCatalogRetainsHistoricalKeysAndOverridesOnlyMatchingCredentials', tests)


if __name__ == "__main__":
    unittest.main()
