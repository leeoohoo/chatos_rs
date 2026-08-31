import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeConnectorStateStoreTests {
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
                installedAt: "2026-08-25T00:00:00Z"
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
        #expect(restored.pluginPreferences["plugin-a"] == false)
        #expect(restored.workspaces.first?.absoluteRoot == "/tmp/project")
    }
}
