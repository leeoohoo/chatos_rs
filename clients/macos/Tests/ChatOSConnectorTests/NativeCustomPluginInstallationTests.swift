import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeCustomPluginInstallationTests {
    @Test("custom npm MCP requires an exact package version before invoking npm")
    func exactVersionIsRequired() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("CustomMCPValidation-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let installer = NativePluginInstaller(rootURL: root)
        let draft = LocalConnectorCustomMCPDraft(
            displayName: "Unsafe floating version",
            componentKey: "unsafe-version",
            packageName: "rea-agents",
            packageVersion: "latest",
            binaryName: "rea",
            arguments: ["mcp"]
        )
        await #expect(throws: NativeConnectorError.self) {
            _ = try await installer.prepareCustomMCP(draft)
        }
        #expect(!FileManager.default.fileExists(atPath: root.path))
    }

    @Test("local Plugin import rejects symbolic links")
    func localPluginSymlinkIsRejected() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("LocalPluginValidation-\(UUID().uuidString)", isDirectory: true)
        let source = root.appendingPathComponent("source", isDirectory: true)
        let installs = root.appendingPathComponent("installs", isDirectory: true)
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture"}}}
        """.utf8).write(to: source.appendingPathComponent("chatos.plugin.json"))
        try FileManager.default.createSymbolicLink(
            at: source.appendingPathComponent("escape"),
            withDestinationURL: URL(fileURLWithPath: "/tmp")
        )
        let installer = NativePluginInstaller(rootURL: installs)
        #expect(throws: (any Error).self) {
            _ = try installer.prepareLocalPlugin(atPath: source.path)
        }
    }

    @Test("local Plugin requires matching preflight fingerprint and fails closed after drift")
    func localPluginFingerprintAndDriftChecks() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("LocalPluginE2E-\(UUID().uuidString)", isDirectory: true)
        let source = root.appendingPathComponent("source", isDirectory: true)
        let bin = source.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        {
          "schemaVersion": 3,
          "name": "local-fixture",
          "version": "1.0.0",
          "description": "Local fixture",
          "interface": {"displayName": "Local Fixture"},
          "mcpServers": {"local-fixture": {"type":"stdio","bin":"fixture","agent":{"displayName":"Local Fixture","description":"Fixture"}}},
          "permissions": [{"permission":"process.spawn","required":true,"components":["local-fixture"]}]
        }
        """.utf8).write(to: source.appendingPathComponent("chatos.plugin.json"))
        let executable = bin.appendingPathComponent("fixture")
        try Data("""
        #!/usr/bin/env node
        const readline = require("node:readline");
        const lines = readline.createInterface({ input: process.stdin });
        lines.on("line", (line) => {
          const request = JSON.parse(line);
          if (request.id === undefined) return;
          let result = {};
          if (request.method === "initialize") {
            result = { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "fixture", version: "1" } };
          } else if (request.method === "tools/list") {
            result = { tools: [{ name: "ping", description: "Ping", inputSchema: { type: "object", properties: {} } }] };
          }
          process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\\n");
        });
        """.utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)

        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-local", username: "local", displayName: "Local", role: "user")
        state.deviceID = "device-local"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )

        let inspection = try await service.inspectLocalPlugin(atPath: source.path)
        #expect(inspection.components.first?.toolNames == ["ping"])
        await #expect(throws: NativeConnectorError.self) {
            try await service.installLocalPlugin(
                atPath: source.path,
                expectedFingerprint: String(repeating: "0", count: 64)
            )
        }
        try await service.installLocalPlugin(
            atPath: source.path,
            expectedFingerprint: inspection.fingerprint
        )
        var installed = try await service.installedAgentPlugins(ownerUserID: "owner-local")
        #expect(installed.map(\.componentKey) == ["local-fixture"])

        let restored = try NativeConnectorStateStore(stateURL: stateURL).load()
        let record = try #require(restored.installedPluginRecords?.values.first)
        try Data("tampered".utf8).write(
            to: URL(fileURLWithPath: record.installationPath)
                .appendingPathComponent("chatos.plugin.json"),
            options: .atomic
        )
        installed = try await service.installedAgentPlugins(ownerUserID: "owner-local")
        #expect(installed.isEmpty)
    }

    @Test("portable Agent Plugin skill-only packages import and become selectable")
    func portableSkillOnlyPluginImports() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PortableAgentPlugin-\(UUID().uuidString)", isDirectory: true)
        let source = root.appendingPathComponent("source", isDirectory: true)
        let skill = source.appendingPathComponent("skills/hello", isDirectory: true)
        try FileManager.default.createDirectory(at: skill, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        {
          "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
          "name": "portable-hello",
          "version": "1.2.3",
          "description": "Portable skill fixture",
          "extensions": {"com.openai": {"interface": {"displayName": "Portable Hello"}}}
        }
        """.utf8).write(to: source.appendingPathComponent("plugin.json"))
        try Data("""
        ---
        name: hello
        description: Greet the user using the portable plugin workflow.
        ---
        Greet the user warmly.
        """.utf8).write(to: skill.appendingPathComponent("SKILL.md"))

        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-portable", username: "portable", displayName: "Portable", role: "user")
        state.deviceID = "device-portable"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )

        let inspection = try await service.inspectLocalPlugin(atPath: source.path)
        #expect(inspection.displayName == "Portable Hello")
        #expect(inspection.version == "1.2.3")
        #expect(inspection.sourceSummary.contains("Agent Plugin"))
        #expect(inspection.components == [.init(
            componentKey: "hello",
            displayName: "hello",
            kind: .skill,
            toolNames: []
        )])
        try await service.installLocalPlugin(
            atPath: source.path,
            expectedFingerprint: inspection.fingerprint
        )
        let installed = try await service.installedAgentPlugins(ownerUserID: "owner-portable")
        #expect(installed.count == 1)
        #expect(installed.first?.componentKey == NativeInstalledAgentPlugin.skillsOnlyComponentKey)
        #expect(installed.first?.displayName == "Portable Hello")
    }

    @Test("Codex compatibility plugins import their .mcp.json stdio servers")
    func codexPluginMCPImports() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("CodexPluginImport-\(UUID().uuidString)", isDirectory: true)
        let source = root.appendingPathComponent("source", isDirectory: true)
        let manifestFolder = source.appendingPathComponent(".codex-plugin", isDirectory: true)
        try FileManager.default.createDirectory(at: manifestFolder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        {"name":"codex-fixture","version":"2.0.0","description":"Codex fixture","displayName":"Codex Fixture"}
        """.utf8).write(to: manifestFolder.appendingPathComponent("plugin.json"))
        try Data("""
        {"mcpServers":{"fixture":{"command":"node","args":["${PLUGIN_ROOT}/server.js"]}}}
        """.utf8).write(to: source.appendingPathComponent(".mcp.json"))
        try Data("""
        const readline = require("node:readline");
        const lines = readline.createInterface({ input: process.stdin });
        lines.on("line", (line) => {
          const request = JSON.parse(line);
          if (request.id === undefined) return;
          let result = {};
          if (request.method === "initialize") {
            result = { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "codex-fixture", version: "2" } };
          } else if (request.method === "tools/list") {
            result = { tools: [{ name: "ping", description: "Ping", inputSchema: { type: "object", properties: {} } }] };
          }
          process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\\n");
        });
        """.utf8).write(to: source.appendingPathComponent("server.js"))

        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-codex", username: "codex", displayName: "Codex", role: "user")
        state.deviceID = "device-codex"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )

        let inspection = try await service.inspectLocalPlugin(atPath: source.path)
        #expect(inspection.sourceSummary.contains("Codex Plugin"))
        #expect(inspection.components.first?.kind == .mcp)
        #expect(inspection.components.first?.toolNames == ["ping"])
    }

    @Test("a standalone root Skill inside a monorepo subdirectory imports through the unified source flow")
    func standaloneSkillSubdirectoryImports() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("StandaloneSkillSource-\(UUID().uuidString)", isDirectory: true)
        let plugin = root.appendingPathComponent("repository/packages/reviewer", isDirectory: true)
        try FileManager.default.createDirectory(at: plugin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        ---
        name: reviewer
        description: Review a change carefully.
        ---
        Review the requested change and report concrete findings.
        """.utf8).write(to: plugin.appendingPathComponent("SKILL.md"))

        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-skill", username: "skill", displayName: "Skill", role: "user")
        state.deviceID = "device-skill"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )
        let source = LocalConnectorPluginSourceDraft(
            kind: .localFolder,
            location: root.appendingPathComponent("repository").path,
            subdirectory: "packages/reviewer"
        )

        let inspection = try await service.inspectPluginSource(source)
        #expect(inspection.components == [.init(
            componentKey: "reviewer",
            displayName: "reviewer",
            kind: .skill,
            toolNames: []
        )])
        try await service.installPluginSource(
            source,
            expectedFingerprint: inspection.fingerprint
        )
        let installed = try await service.installedAgentPlugins(ownerUserID: "owner-skill")
        #expect(installed.first?.componentKey == NativeInstalledAgentPlugin.skillsOnlyComponentKey)
    }

    @Test("VS Code style servers accept a structured command array")
    func vscodeMCPConfigurationImports() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("VSCodeMCPImport-\(UUID().uuidString)", isDirectory: true)
        let source = root.appendingPathComponent("source", isDirectory: true)
        let config = source.appendingPathComponent(".vscode", isDirectory: true)
        try FileManager.default.createDirectory(at: config, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        {"servers":{"fixture":{"type":"stdio","command":["node","${PLUGIN_ROOT}/server.js"]}}}
        """.utf8).write(to: config.appendingPathComponent("mcp.json"))
        try Data("""
        const readline = require("node:readline");
        const lines = readline.createInterface({ input: process.stdin });
        lines.on("line", (line) => {
          const request = JSON.parse(line);
          if (request.id === undefined) return;
          let result = {};
          if (request.method === "initialize") {
            result = { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "fixture", version: "1" } };
          } else if (request.method === "tools/list") {
            result = { tools: [{ name: "ping", inputSchema: { type: "object" } }] };
          }
          process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\\n");
        });
        """.utf8).write(to: source.appendingPathComponent("server.js"))

        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-vscode", username: "vscode", displayName: "VS Code", role: "user")
        state.deviceID = "device-vscode"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )

        let inspection = try await service.inspectLocalPlugin(atPath: source.path)
        #expect(inspection.sourceSummary.contains("Standalone MCP"))
        #expect(inspection.components.first?.toolNames == ["ping"])
    }

    @Test("Git plugin sources reject insecure or credential-bearing URLs before acquisition")
    func gitPluginSourceValidation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("GitPluginValidation-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: root.appendingPathComponent("state.json")
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )
        await #expect(throws: NativeConnectorError.self) {
            _ = try await service.inspectPluginSource(.init(
                kind: .gitRepository,
                location: "http://example.test/plugin.git"
            ))
        }
        await #expect(throws: NativeConnectorError.self) {
            _ = try await service.inspectPluginSource(.init(
                kind: .gitRepository,
                location: "https://token@example.test/plugin.git"
            ))
        }
    }

    @Test(
        "REA 6.5.0 completes inspect, fingerprinted install, persistence and Main Chat component discovery",
        .enabled(if: ProcessInfo.processInfo.environment["CHATOS_REA_INTEGRATION"] == "1")
    )
    func reaEndToEndInstallation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("REAInstallation-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-rea", username: "rea", displayName: "REA", role: "user")
        state.deviceID = "device-rea"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )
        let draft = LocalConnectorCustomMCPDraft(
            displayName: "Reverse Engineer Anything",
            description: "Pinned REA integration fixture",
            componentKey: "reverse-engineer-anything",
            packageName: "rea-agents",
            packageVersion: "6.5.0",
            binaryName: "rea",
            arguments: ["mcp"]
        )

        let inspection = try await service.inspectCustomMCP(draft)
        let component = try #require(inspection.components.first)
        #expect(inspection.version == "6.5.0")
        #expect(component.componentKey == "reverse-engineer-anything")
        #expect(component.toolNames.count >= 200)
        #expect(component.toolNames.contains("binary_session"))

        try await service.installCustomMCP(
            draft,
            expectedFingerprint: inspection.fingerprint
        )
        let installed = try await service.installedAgentPlugins(ownerUserID: "owner-rea")
        #expect(installed.count == 1)
        #expect(installed.first?.componentKey == "reverse-engineer-anything")
        #expect(installed.first?.displayName == "Reverse Engineer Anything")

        let restored = try NativeConnectorStateStore(stateURL: stateURL).load()
        let record = try #require(restored.installedPluginRecords?.values.first)
        #expect(record.source == .customMCP)
        #expect(record.version == "6.5.0")
        #expect(NativePluginInstallationIntegrity.status(record: record) == .verified)
    }
}

private struct CustomPluginRejectingTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String {
        throw URLError(.userAuthenticationRequired)
    }
}
