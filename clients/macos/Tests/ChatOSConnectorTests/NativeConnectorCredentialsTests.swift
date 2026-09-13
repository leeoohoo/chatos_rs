import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeConnectorCredentialsTests {
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
    func webDesignStudioUpdateFrom0100To0110IsAvailable() {
        let installed = NativeInstalledPluginRecord(
            pluginID: "14993149-32e7-40a8-b166-dc7639edebfc",
            releaseID: "9a8dbb08-bb45-434e-a59e-40550867e7fb",
            version: "0.10.0",
            artifactSHA256: "e22f2a4e5b14d3e434fa4b01541d74dbcef31de0fc2586083962549467c0e20a",
            installationPath: "/tmp/chatos-web-design-studio/0.10.0",
            installedAt: "2026-09-07T00:00:00Z"
        )

        #expect(NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "c8a8a26c-4605-4477-acb0-9a84a04b3098",
                version: "0.11.0",
                artifactSHA256: "75de6a325efb6bc1c3034ba9e207461d7dcfb6945498fa3c08d27e82d4a673c3",
                npmPackage: nil
            )
        ))
    }

    @Test
    func dynamicGatewayRequestsBypassCachedMarketplaceResponses() {
        let request = NativeConnectorGateway.dynamicRequest(
            url: URL(string: "https://connector.jgoool.com/api/plugin-management/plugins/install-sources")!
        )

        #expect(request.cachePolicy == .reloadIgnoringLocalAndRemoteCacheData)
        #expect(request.value(forHTTPHeaderField: "Cache-Control") == "no-cache, no-store")
        #expect(request.value(forHTTPHeaderField: "Pragma") == "no-cache")
    }

    @Test
    func pairingStateUsesTheSelectedClientStorageProvider() async throws {
        let transport = ProjectRunPreferencesTransport()
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        let accountSession = ProjectRunPreferencesAccountSession(client: client)
        let store = NativeConnectorPairingStateStore(accountSession: accountSession)
        _ = try await store.activate(ownerUserID: "user-1")
        var state = NativeConnectorPairingState.empty
        state.user = .init(id: "user-1", username: "tester", displayName: nil, role: "user")
        state.deviceID = "device-1"
        state.deviceName = "Test Mac"
        state.gatewayConnectionEnabled = false
        state.workspaces = [
            .init(id: "workspace-1", alias: "Project", absoluteRoot: "/tmp/project", fingerprint: "abc")
        ]

        _ = try await store.save(ownerUserID: "user-1", value: state)
        let restarted = NativeConnectorPairingStateStore(accountSession: accountSession)
        let restored = try await restarted.activate(ownerUserID: "user-1")

        #expect(restored.deviceID == "device-1")
        #expect(restored.deviceName == "Test Mac")
        #expect(restored.gatewayConnectionEnabled == false)
        #expect(restored.workspaces.first?.absoluteRoot == "/tmp/project")

        let requests = await transport.requests()
        #expect(requests.contains { request in
            guard let object = try? JSONSerialization.jsonObject(with: request) as? [String: Any],
                  let command = object["command"] as? [String: Any]
            else { return false }
            return command["type"] as? String == "put_client_setting"
        })
    }

    @Test
    func legacyStateJSONIsNeverRead() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data(#"{"deviceID":"legacy-device","gatewayConnectionEnabled":true}"#.utf8)
            .write(to: root.appendingPathComponent("state.json"))

        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                supportRootURL: root
            ),
            ticketProvider: LegacyStateTicketProvider(),
            accountSession: UnavailableLocalAgentAccountSession(),
            agentRuntimeSettings: AgentRuntimePreferencesTestProvider()
        )

        let state = await connector.pairingState
        #expect(state == .empty)
    }
}

private struct LegacyStateTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
