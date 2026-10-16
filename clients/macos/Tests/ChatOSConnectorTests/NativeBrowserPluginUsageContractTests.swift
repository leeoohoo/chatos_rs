import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

enum NativeBrowserPluginUsageFixture {
    static func installedRecord(at installation: URL) throws -> NativeInstalledPluginRecord {
        let statePath = try XCTUnwrap(ProcessInfo.processInfo.environment["CHATOS_PLUGIN_TEST_STATE_PATH"],
                                     "Activated installed-plugin tests require the original installation record")
        let state = try NativeConnectorStateStore(stateURL: URL(fileURLWithPath: statePath)).load()
        return try XCTUnwrap(state.installedPluginRecords?.values.first {
            URL(fileURLWithPath: $0.installationPath).standardizedFileURL == installation.standardizedFileURL
        }, "No matching installation integrity record; do not fabricate trust for an installed package")
    }
    static let openArguments = NativeJSONValue.object([
        "session_name": .string("ChatOS isolated browser usage test"),
    ])
    static let navigationArguments = NativeJSONValue.object([
        "timeout_ms": .number(30_000),
        "url": .string("data:text/html,%3Ctitle%3EChatOS%20Browser%20Fixture%3C/title%3E%3Ch1%3EIsolated%20test%3C/h1%3E%3Cbutton%3ETest%20action%3C/button%3E"),
    ])
}

final class NativeBrowserPluginUsageContractTests: XCTestCase {
    func testRepositoryBrowserCatalogWithoutBrowserActions() async throws {
        guard let executable = ProcessInfo.processInfo.environment["CHATOS_BROWSER_CDP_CATALOG_EXECUTABLE"],
              let package = ProcessInfo.processInfo.environment["CHATOS_BROWSER_CDP_CATALOG_PACKAGE_ROOT"] else {
            if ProcessInfo.processInfo.environment["CHATOS_REQUIRE_NATIVE_HOST_TESTS"] == "1" {
                XCTFail("Required Browser catalog executable/package is missing from native CI")
                return
            }
            throw XCTSkip("Use native-platform verification to build and check the repository Browser catalog")
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let installation = root.appendingPathComponent("package")
        let bin = installation.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try FileManager.default.copyItem(atPath: executable, toPath: bin.appendingPathComponent("chatos-browser-cdp").path)
        var manifest = try JSONDecoder().decode(NativePluginManifest.self, from: Data(contentsOf:
            URL(fileURLWithPath: package).appendingPathComponent("chatos.plugin.json")))
        manifest.mcpServers[NativeBrowserPluginIdentity.componentKey]?.env["CHATOS_BROWSER_EXTENSION_ID"] = ""
        // This is a fresh CI fixture, not a replacement integrity record for an installed release.
        let launch = try NativePluginManifestLoader.prepare(
            record: try nativePluginTestRecord(installationURL: installation, version: manifest.version), manifest: manifest,
            componentKey: NativeBrowserPluginIdentity.componentKey, serverKey: nil,
            adapterSessionID: UUID().uuidString, ownerUserID: "fixture-user", deviceID: "fixture-device",
            workspaceRoot: nil, permissionSnapshot: Set(manifest.permissions.map(\.permission)),
            runtimeRootURL: root.appendingPathComponent("runtime")
        )
        try await verifyUsageSchemas(launch)
    }

    func testUsageRequestsAgainstActualInstalledCatalogWithoutBrowserActions() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_BROWSER_CDP_CATALOG_ROOT"] else {
            throw XCTSkip("Set CHATOS_BROWSER_CDP_CATALOG_ROOT to verify an actual Browser catalog")
        }
        let installation = URL(fileURLWithPath: path, isDirectory: true)
        var manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: installation.appendingPathComponent("chatos.plugin.json"))
        )
        // tools/list does not open a browser; also prevent test process bridge setup
        // from registering a native host in the user's existing Chrome.
        manifest.mcpServers[NativeBrowserPluginIdentity.componentKey]?.env["CHATOS_BROWSER_EXTENSION_ID"] = ""
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let launch = try NativePluginManifestLoader.prepare(
            record: try NativeBrowserPluginUsageFixture.installedRecord(at: installation),
            manifest: manifest, componentKey: NativeBrowserPluginIdentity.componentKey,
            serverKey: nil, adapterSessionID: UUID().uuidString, ownerUserID: "fixture-user",
            deviceID: "fixture-device", workspaceRoot: nil,
            permissionSnapshot: Set(manifest.permissions.map(\.permission)), runtimeRootURL: root
        )
        try await verifyUsageSchemas(launch)
    }

    private func verifyUsageSchemas(_ launch: NativePreparedPluginLaunch) async throws {
        let client = NativePluginStdioClient(launch: launch)
        do {
            try await client.start()
            let initialized = try await client.initialize()
            func schema(_ name: String) throws -> Data {
                let tool = try XCTUnwrap(initialized.tools.first { $0.jsonObject?["name"]?.jsonString == name })
                return try JSONEncoder().encode(XCTUnwrap(tool.jsonObject?["inputSchema"]))
            }
            let openSchema = try schema("browser_session_open")
            try AgentExternalToolSchemaValidator.validate(
                arguments: NativeBrowserPluginUsageFixture.openArguments.canonicalJSONString, schema: openSchema
            )
            for field in ["mode", "headless", "persistent_profile"] {
                XCTAssertThrowsError(try AgentExternalToolSchemaValidator.validate(
                    arguments: "{\"\(field)\":true}", schema: openSchema
                ), "Obsolete \(field) must not be sent by native smoke tests")
            }
            let navigationSchema = try schema("browser_navigate")
            try AgentExternalToolSchemaValidator.validate(
                arguments: NativeBrowserPluginUsageFixture.navigationArguments.canonicalJSONString, schema: navigationSchema
            )
            for invalid in ["{}", "{\"url\":12}", "{\"url\":\"https://example.test\",\"timeout_ms\":0}"] {
                XCTAssertThrowsError(try AgentExternalToolSchemaValidator.validate(arguments: invalid, schema: navigationSchema))
            }
            await client.terminate()
        } catch {
            await client.terminate()
            throw error
        }
    }
}
