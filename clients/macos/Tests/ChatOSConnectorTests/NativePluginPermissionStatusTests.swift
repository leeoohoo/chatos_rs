@testable import ChatOSConnector
import Foundation
import Testing

extension NativePluginRuntimeTests {
    @Test("plugin capabilities are reported as available instead of ambiguous on-demand permissions")
    func pluginCapabilityStatusIsExplicit() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {
              "schemaVersion":3,
              "name":"chatos-browser-cdp",
              "version":"0.1.4",
              "mcpServers":{"browser-cdp":{"type":"stdio","bin":"browser-cdp"}},
              "permissions":[
                {"permission":"browser.page.read","required":true},
                {"permission":"browser.network.observe","required":false}
              ]
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "0.1.4",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-08-27T00:00:00Z",
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: root,
                maximumFiles: 20_000,
                maximumBytes: 512 * 1_024 * 1_024
            )
        )

        let permissions = await NativePluginPermissionInspector.permissions(
            record: record,
            manifest: manifest
        )

        #expect(permissions.allSatisfy { $0.status == "ready" })
        #expect(permissions.allSatisfy { $0.statusLabel == "已可用" })
        #expect(permissions.allSatisfy { !$0.canRequest })
        #expect(permissions[1].label == "查看浏览器网络")
        #expect(permissions[1].summary.contains("WebSocket"))
    }
}
