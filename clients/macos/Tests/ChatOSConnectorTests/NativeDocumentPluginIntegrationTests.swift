@testable import ChatOSConnector
import Foundation
import Darwin
import XCTest

/// Opt-in installed Release test, using the same Seatbelt launcher as the App.
final class NativeDocumentPluginIntegrationTests: XCTestCase {
    func testInstalledDocumentCreationAndRenderingInsideNativeSandbox() async throws {
        let environment = ProcessInfo.processInfo.environment
        let record: NativeInstalledPluginRecord
        if let packagePath = environment["CHATOS_DOCUMENT_TEST_PACKAGE_ROOT"] {
            // Test-only unpacked build, never registered as an installed Release.
            let installation = URL(fileURLWithPath: packagePath, isDirectory: true)
            let manifest = try JSONDecoder().decode(NativePluginManifest.self, from:
                Data(contentsOf: installation.appendingPathComponent("chatos.plugin.json")))
            var fixture = try nativePluginTestRecord(installationURL: installation, version: manifest.version)
            fixture.pluginKey = NativeDocumentPluginIdentity.marketplaceKey
            record = fixture
        } else if let statePath = environment["CHATOS_PLUGIN_TEST_STATE_PATH"] {
            let state = try NativeConnectorStateStore(stateURL: URL(fileURLWithPath: statePath)).load()
            record = try XCTUnwrap(state.installedPluginRecords?.values.first {
                $0.pluginKey == NativeDocumentPluginIdentity.marketplaceKey
            })
        } else {
            throw XCTSkip("Pass original Native state or a test-only unpacked Document build")
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let installation = URL(fileURLWithPath: record.installationPath)
        let manifest = try JSONDecoder().decode(NativePluginManifest.self, from:
            Data(contentsOf: installation.appendingPathComponent("chatos.plugin.json")))
        let launch = try NativePluginManifestLoader.prepare(
            record: record, manifest: manifest, componentKey: NativeDocumentPluginIdentity.componentKey,
            serverKey: nil, adapterSessionID: UUID().uuidString,
            ownerUserID: "fixture-user", deviceID: "fixture-device", workspaceRoot: nil,
            permissionSnapshot: Set(manifest.permissions.map(\.permission)), runtimeRootURL: root
        )
        let client = NativePluginStdioClient(launch: launch)
        do {
            try await client.start()
            _ = try await client.initialize()
            let created = try await client.callTool(name: "office_create", arguments: .object([
                "format": .string("docx"), "outputName": .string("插件验收报告.docx"),
                "locale": .string("zh-CN"), "operations": .array([
                    .object(["type": .string("word_add_heading"), "level": .number(1), "text": .string("插件验收报告")]),
                    .object(["type": .string("word_add_paragraph"), "text": .string("测试数据，不用于真实业务")]),
                    .object(["type": .string("word_add_table"), "rows": .array([
                        .array([.string("项目"), .string("结果")]),
                        .array([.string("文档渲染"), .string("待验证")])
                    ])])
                ])
            ]), timeout: .seconds(60))
            XCTAssertNotEqual(created.jsonObject?["isError"]?.jsonBool, true)
            let rendered = try await client.callTool(name: "document_render", arguments: .object([
                "inputPath": .string("插件验收报告.docx"), "outputPrefix": .string("preview"),
                "pages": .array([.number(1)])
            ]), timeout: .seconds(150))
            if rendered.jsonObject?["isError"]?.jsonBool == true {
                // The old MCP wrapper drops stderr. Run the same trusted binary
                // under the same profile to expose an actionable fixture-only cause.
                try engineDiagnostic(launch: launch)
            }
            XCTAssertNotEqual(rendered.jsonObject?["isError"]?.jsonBool, true,
                              rendered.canonicalJSONString)
            XCTAssertTrue(rendered.jsonObject?["content"]?.jsonArray?.contains {
                $0.jsonObject?["type"]?.jsonString == "image"
            } == true)
        } catch {
            await client.terminate()
            throw error
        }
        await client.terminate()
    }

    private func engineDiagnostic(launch: NativePreparedPluginLaunch) throws {
        struct Vendor: Decodable {
            struct Asset: Decodable { let platform: String; let arch: String; let target: String }
            let assets: [Asset]
        }
        let vendor = try JSONDecoder().decode(Vendor.self, from: Data(contentsOf:
            launch.installationURL.appendingPathComponent("vendor/officecli-v1.0.144.json")))
        #if arch(arm64)
        let architecture = "arm64"
        #else
        let architecture = "x64"
        #endif
        let asset = try XCTUnwrap(vendor.assets.first { $0.platform == "darwin" && $0.arch == architecture })
        var processLaunch = NativePluginProcessLaunch(stdio: launch)
        processLaunch.executableURL = launch.installationURL.appendingPathComponent("vendor/" + asset.target)
        processLaunch.arguments = ["view", launch.artifactURL.appendingPathComponent("插件验收报告.docx").path,
                                  "screenshot", "--page", "1", "--out", launch.artifactURL.appendingPathComponent("diagnostic.png").path,
                                  "--screenshot-width", "1600", "--screenshot-height", "1200", "--render", "html", "--json"]
        for key in ["OFFICECLI_SKIP_UPDATE", "OFFICECLI_NO_AUTO_INSTALL", "OFFICECLI_NO_AUTO_RESIDENT"] {
            processLaunch.environment[key] = "1"
        }
        // Diagnostic must use the same immutable browser adapter as the MCP.
        processLaunch.arguments = ["-c", "PATH=\"$CHATOS_PLUGIN_ROOT/bin/browser:$PATH\" exec \"$@\"", "diagnostic", processLaunch.executableURL.path] + processLaunch.arguments
        processLaunch.executableURL = URL(fileURLWithPath: "/bin/sh")
        let prepared = try NativePluginProcessLauncher.prepare(processLaunch)
        let process = Process()
        process.executableURL = prepared.executableURL
        // Native posix_spawn includes argv[0]; Foundation adds it itself.
        process.arguments = Array(prepared.arguments.dropFirst())
        process.environment = prepared.environment
        process.currentDirectoryURL = launch.artifactURL
        let diagnosticURL = launch.artifactURL.appendingPathComponent("engine-diagnostic.txt")
        FileManager.default.createFile(atPath: diagnosticURL.path, contents: nil)
        let diagnosticHandle = try FileHandle(forWritingTo: diagnosticURL)
        defer { try? diagnosticHandle.close() }
        process.standardError = diagnosticHandle
        process.standardOutput = diagnosticHandle
        try process.run()
        let deadline = Date().addingTimeInterval(30)
        while process.isRunning && Date() < deadline { Thread.sleep(forTimeInterval: 0.05) }
        if process.isRunning {
            process.terminate()
            let grace = Date().addingTimeInterval(1)
            while process.isRunning && Date() < grace { Thread.sleep(forTimeInterval: 0.05) }
            if process.isRunning { _ = kill(process.processIdentifier, SIGKILL) }
        }
        process.waitUntilExit()
        let diagnostic = try Data(contentsOf: diagnosticURL)
        XCTFail("Sandboxed engine exit \(process.terminationStatus): \(String(decoding: diagnostic, as: UTF8.self).prefix(4000))")
    }
}
