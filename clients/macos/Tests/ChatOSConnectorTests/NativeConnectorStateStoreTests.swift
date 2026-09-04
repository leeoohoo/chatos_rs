import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeConnectorStateStoreTests {
    @Test
    func pluginUpdateStateUsesInstalledReleaseVersionAndArtifact() {
        let installed = NativeInstalledPluginRecord(
            pluginID: "plugin-a",
            releaseID: "release-1",
            version: "0.1.2",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: "/tmp/plugin-a/0.1.2",
            installedAt: "2026-09-04T00:00:00Z"
        )
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-1",
                version: "0.1.2",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            )
        ))
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-republished",
                version: "0.1.2",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            )
        ))
        #expect(NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-2",
                version: "0.1.3",
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            )
        ))
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-missing-version",
                version: nil,
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            )
        ))
    }

    @Test
    func stateRoundTripsWithoutLosingSecurityOrPluginSettings() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = NativeConnectorStateStore(stateURL: directory.appendingPathComponent("state.json"))
        var state = NativeConnectorPersistentState.empty
        state.deviceID = "device-1"
        state.deviceName = "Test Mac"
        state.developerMode = true
        state.sandboxEnabled = false
        state.permissionProfileID = ":read-only"
        state.networkAccess = "restricted"
        state.installedPluginIDs = ["plugin-a"]
        state.commandApprovalModelConfigID = "approval-model"
        state.commandApprovalThinkingLevel = "high"
        state.installedPluginRecords = [
            "plugin-a": .init(
                pluginID: "plugin-a",
                releaseID: "release-a",
                version: "1.2.3",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: "/tmp/plugin-a/1.2.3",
                installedAt: "2026-08-25T00:00:00Z",
                pluginKey: "plugin-a@official"
            ),
        ]
        state.pluginPreferences = ["plugin-a": false]
        state.workspaces = [
            .init(id: "workspace-1", alias: "Project", absoluteRoot: "/tmp/project", fingerprint: "abc")
        ]

        try store.save(state)
        let restored = try store.load()

        #expect(restored.deviceID == "device-1")
        #expect(restored.deviceName == "Test Mac")
        #expect(restored.developerMode)
        #expect(!restored.sandboxEnabled)
        #expect(restored.permissionProfileID == ":read-only")
        #expect(restored.networkAccess == "restricted")
        #expect(restored.installedPluginIDs == ["plugin-a"])
        #expect(restored.commandApprovalModelConfigID == "approval-model")
        #expect(restored.commandApprovalThinkingLevel == "high")
        #expect(restored.installedPluginRecords?["plugin-a"]?.version == "1.2.3")
        #expect(restored.installedPluginRecords?["plugin-a"]?.pluginKey == "plugin-a@official")
        #expect(restored.pluginPreferences["plugin-a"] == false)
        #expect(restored.workspaces.first?.absoluteRoot == "/tmp/project")
    }

    @Test
    func stalePluginIdentityMigratesToRepublishedCatalogEntryByArtifact() throws {
        var state = NativeConnectorPersistentState.empty
        state.installedPluginIDs = ["old-plugin-id"]
        state.pluginPreferences = ["old-plugin-id": false]
        state.installedPluginRecords = [
            "old-plugin-id": .init(
                pluginID: "old-plugin-id",
                releaseID: "old-release-id",
                version: "0.1.6",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: "/tmp/browser/0.1.6",
                installedAt: "2026-08-25T00:00:00Z"
            ),
        ]
        let source = GatewayPluginSourceDTO(
            catalog: GatewayPluginCatalogDTO(
                id: "current-plugin-id",
                displayName: "Browser CDP",
                name: "chatos-browser-cdp",
                description: nil,
                publisher: nil,
                interface: nil
            ),
            release: GatewayPluginReleaseDTO(
                id: "current-release-id",
                version: "0.1.6",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            ),
            preference: nil
        )

        let changed = NativeLocalConnectorService.reconcileInstalledPluginIdentities(
            state: &state,
            sources: [source]
        )

        #expect(changed)
        #expect(state.installedPluginIDs == ["current-plugin-id"])
        #expect(state.installedPluginRecords?["old-plugin-id"] == nil)
        #expect(state.installedPluginRecords?["current-plugin-id"]?.pluginID == "current-plugin-id")
        #expect(state.installedPluginRecords?["current-plugin-id"]?.releaseID == "current-release-id")
        #expect(state.pluginPreferences["old-plugin-id"] == nil)
        #expect(state.pluginPreferences["current-plugin-id"] == false)
    }

    @Test
    func stablePluginKeySelectsTheCorrectCatalogEntryWhenArtifactsCollide() throws {
        var state = NativeConnectorPersistentState.empty
        state.installedPluginIDs = ["old-computer-use-id"]
        state.installedPluginRecords = [
            "old-computer-use-id": .init(
                pluginID: "old-computer-use-id",
                releaseID: "old-release-id",
                version: "0.8.11",
                artifactSHA256: String(repeating: "c", count: 64),
                installationPath: "/tmp/computer-use/0.8.11",
                installedAt: "2026-08-25T00:00:00Z",
                pluginKey: "open-computer-use@chatos-marketplace"
            ),
        ]
        let sources = [
            GatewayPluginSourceDTO(
                catalog: GatewayPluginCatalogDTO(
                    id: "unrelated-plugin-id",
                    displayName: "Unrelated",
                    name: "unrelated",
                    description: nil,
                    publisher: nil,
                    interface: nil,
                    pluginKey: "unrelated@chatos-marketplace"
                ),
                release: GatewayPluginReleaseDTO(
                    id: "unrelated-release-id",
                    version: "0.8.11",
                    artifactSHA256: String(repeating: "c", count: 64),
                    npmPackage: nil
                ),
                preference: nil
            ),
            GatewayPluginSourceDTO(
                catalog: GatewayPluginCatalogDTO(
                    id: "current-computer-use-id",
                    displayName: "Computer Use",
                    name: "open-computer-use",
                    description: nil,
                    publisher: nil,
                    interface: nil,
                    pluginKey: "open-computer-use@chatos-marketplace"
                ),
                release: GatewayPluginReleaseDTO(
                    id: "current-release-id",
                    version: "0.8.11",
                    artifactSHA256: String(repeating: "c", count: 64),
                    npmPackage: nil
                ),
                preference: nil
            ),
        ]

        let changed = NativeLocalConnectorService.reconcileInstalledPluginIdentities(
            state: &state,
            sources: sources
        )

        #expect(changed)
        #expect(state.installedPluginIDs == ["current-computer-use-id"])
        #expect(state.installedPluginRecords?["current-computer-use-id"]?.releaseID == "current-release-id")
        #expect(
            state.installedPluginRecords?["current-computer-use-id"]?.pluginKey
                == "open-computer-use@chatos-marketplace"
        )
    }

    @Test
    func currentInstallRemovesLegacyDuplicateByInstalledPackageName() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let legacyInstall = directory.appendingPathComponent("0.8.11", isDirectory: true)
        try FileManager.default.createDirectory(at: legacyInstall, withIntermediateDirectories: true)
        let legacyManifest = """
        {
          "schemaVersion": 3,
          "name": "open-computer-use",
          "version": "0.8.11",
          "mcpServers": {},
          "skills": []
        }
        """
        try Data(legacyManifest.utf8).write(
            to: legacyInstall.appendingPathComponent("chatos.plugin.json")
        )

        var state = NativeConnectorPersistentState.empty
        state.installedPluginIDs = ["legacy-id", "current-id"]
        state.pluginPreferences = ["legacy-id": false, "current-id": true]
        state.installedPluginRecords = [
            "legacy-id": .init(
                pluginID: "legacy-id",
                releaseID: "legacy-release",
                version: "0.8.11",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: legacyInstall.path,
                installedAt: "2026-08-25T00:00:00Z"
            ),
            "current-id": .init(
                pluginID: "current-id",
                releaseID: "current-release",
                version: "0.8.12",
                artifactSHA256: String(repeating: "b", count: 64),
                installationPath: "/tmp/computer-use/0.8.12",
                installedAt: "2026-09-01T00:00:00Z",
                pluginKey: "open-computer-use@chatos-marketplace"
            ),
        ]
        let source = GatewayPluginSourceDTO(
            catalog: GatewayPluginCatalogDTO(
                id: "current-id",
                displayName: "Visual Computer Use",
                name: "open-computer-use",
                description: nil,
                publisher: nil,
                interface: nil,
                pluginKey: "open-computer-use@chatos-marketplace"
            ),
            release: GatewayPluginReleaseDTO(
                id: "current-release",
                version: "0.8.12",
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            ),
            preference: nil
        )

        let changed = NativeLocalConnectorService.reconcileInstalledPluginIdentities(
            state: &state,
            sources: [source]
        )

        #expect(changed)
        #expect(state.installedPluginIDs == ["current-id"])
        #expect(state.installedPluginRecords?["legacy-id"] == nil)
        #expect(state.installedPluginRecords?["current-id"]?.version == "0.8.12")
        #expect(state.pluginPreferences["legacy-id"] == nil)
        #expect(state.pluginPreferences["current-id"] == true)
    }
}
