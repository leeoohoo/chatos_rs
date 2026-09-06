import Foundation
import Testing
@testable import ChatOSConnector

struct NativeBrowserExtensionPairingRuntimeTests {
    @Test
    func pairingStatusAcceptsOnlyTheProductionExtensionIdentity() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("BrowserPairingStatus-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let pairing = root.appendingPathComponent("extension-pairing.json")
        try Data(#"{"protocol_version":"1.0","allowed_extension_origin":"chrome-extension://jooaepjckiofmpldinopgdgddcoaofil/"}"#.utf8)
            .write(to: pairing)

        #expect(NativeBrowserExtensionPairingStatus.isPaired(at: pairing))

        try Data(#"{"protocol_version":"1.0","allowed_extension_origin":"chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"}"#.utf8)
            .write(to: pairing)
        #expect(!NativeBrowserExtensionPairingStatus.isPaired(at: pairing))
    }

    @Test
    func pairingRuntimeKeepsInitializedBrowserMCPAliveUntilStopped() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("BrowserPairingRuntime-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}'
          elif [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"browser_session_status"}]}}'
          fi
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"chatos-browser-cdp","version":"1.0.0","mcpServers":{"browser-cdp":{"type":"stdio","bin":"fixture","args":[]}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            manifest: manifest,
            componentKey: "browser-cdp",
            server: manifest.mcpServers["browser-cdp"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: root,
            visualSessionURL: root.appendingPathComponent("visual"),
            artifactURL: root.appendingPathComponent("artifacts"),
            displayName: "Browser CDP"
        )
        let runtime = NativeBrowserExtensionPairingRuntime(lifetime: .seconds(30))

        try await runtime.start(launch: launch)
        #expect(await runtime.isRunning())

        await runtime.stop()
        #expect(!(await runtime.isRunning()))
    }
}
