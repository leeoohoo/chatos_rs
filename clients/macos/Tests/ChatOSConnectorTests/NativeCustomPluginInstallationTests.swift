import ChatOSCore
import Darwin
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeCustomPluginInstallationTests {
    @Test("custom MCP access profiles grant only their declared permission sets")
    func accessProfilePermissionsAreExplicit() {
        let restricted = NativeCustomMCPAccessPolicy.permissions(for: .restricted).map(\.id)
        let network = NativeCustomMCPAccessPolicy.permissions(for: .networkClient).map(\.id)
        let developer = NativeCustomMCPAccessPolicy.permissions(for: .developerAnalysis).map(\.id)

        #expect(restricted == ["process.spawn"])
        #expect(network == ["process.spawn", "network.client"])
        #expect(Set(developer) == Set([
            "process.spawn",
            "network.client",
            "system.developer-tools.read",
            "browser.managed.launch",
            "artifact.create",
            "workspace.read",
        ]))
        #expect(!restricted.contains("network.client"))
        #expect(!restricted.contains("system.developer-tools.read"))
        #expect(!restricted.contains("browser.managed.launch"))
    }

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
            arguments: ["mcp"],
            accessProfile: .restricted
        )
        await #expect(throws: NativeConnectorError.self) {
            _ = try await installer.prepareCustomMCP(draft)
        }
        #expect(!FileManager.default.fileExists(atPath: root.path))
    }

    @Test("npm operations use concurrent process groups and cancellation stays operation-scoped")
    func npmOperationsAreConcurrentAndCancellationIsScoped() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("NPMProcessIsolation-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let parentPIDURL = root.appendingPathComponent("parent.pid")
        let childPIDURL = root.appendingPathComponent("child.pid")
        let longRunning = Task {
            try await runNPMFixture(
                arguments: [
                    "-c",
                    "trap '' TERM; echo $$ > '\(parentPIDURL.path)'; sleep 30 & echo $! > '\(childPIDURL.path)'; wait",
                ],
                timeout: 30
            )
        }
        for _ in 0..<200
            where !FileManager.default.fileExists(atPath: parentPIDURL.path)
                || !FileManager.default.fileExists(atPath: childPIDURL.path) {
            try await Task.sleep(for: .milliseconds(10))
        }
        let parentPIDText = try String(contentsOf: parentPIDURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let childPIDText = try String(contentsOf: childPIDURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let parentPID = try #require(pid_t(parentPIDText))
        let childPID = try #require(pid_t(childPIDText))

        let clock = ContinuousClock()
        let startedAt = clock.now
        let survivor = Task {
            try await runNPMFixture(
                arguments: ["-c", "sleep 1; printf isolated"],
                timeout: 5
            )
        }
        let canceller = Task {
            try await Task.sleep(for: .milliseconds(200))
            longRunning.cancel()
        }
        let survivorOutput = try await survivor.value
        #expect(String(decoding: survivorOutput, as: UTF8.self) == "isolated")
        #expect(clock.now - startedAt < .seconds(2))
        _ = try await canceller.value
        do {
            _ = try await longRunning.value
            Issue.record("expected the owned npm process group to be cancelled")
        } catch is CancellationError {
            // Expected: cancellation of this operation must not affect the survivor above.
        } catch {
            Issue.record("expected CancellationError, got \(error)")
        }
        for _ in 0..<100 where installerProcessExists(parentPID) || installerProcessExists(childPID) {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(!installerProcessExists(parentPID))
        #expect(!installerProcessExists(childPID))
    }

    @Test("npm process output remains bounded after moving to blocking lanes")
    func npmProcessOutputRemainsBounded() async throws {
        do {
            _ = try await runNPMFixture(
                arguments: ["-c", "/usr/bin/yes x | /usr/bin/head -c 2200000"],
                timeout: 5,
                overflowMessage: "fixture output overflow"
            )
            Issue.record("expected oversized npm output to be rejected")
        } catch let error as NativeConnectorError {
            #expect(error.localizedDescription.contains("fixture output overflow"))
        }
    }

    @Test("npm process timeout terminates its complete process group")
    func npmProcessTimeoutTerminatesProcessGroup() async throws {
        let clock = ContinuousClock()
        let startedAt = clock.now
        do {
            _ = try await runNPMFixture(
                arguments: ["-c", "sleep 30"],
                timeout: 0.1,
                timeoutMessage: "fixture timed out"
            )
            Issue.record("expected the npm fixture to time out")
        } catch let error as NativeConnectorError {
            #expect(error.localizedDescription.contains("fixture timed out"))
        }
        #expect(clock.now - startedAt < .seconds(3))
    }

    @Test("offline REA fixture completes npm preflight, install, persistence and Main Chat discovery")
    func offlineREAInstallation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("OfflineREAInstallation-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(
            id: "owner-offline-rea",
            username: "offline-rea",
            displayName: "Offline REA",
            role: "user"
        )
        state.deviceID = "device-offline-rea"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let configuration = NativeConnectorConfiguration(
            gatewayBaseURL: URL(string: "https://connector.example.test")!,
            stateURL: stateURL
        )
        let service = NativeLocalConnectorService(
            configuration: configuration,
            ticketProvider: CustomPluginRejectingTicketProvider(),
            pluginInstallerFactory: { installationRoot in
                NativePluginInstaller(
                    rootURL: installationRoot,
                    npmInstall: Self.installOfflineREAFixture(in:)
                )
            }
        )
        let draft = LocalConnectorCustomMCPDraft(
            displayName: "REA",
            description: "Deterministic reverse-engineering fixture.",
            componentKey: "rea",
            packageName: "rea-agents",
            packageVersion: "6.5.0",
            binaryName: "rea",
            arguments: ["mcp"],
            accessProfile: .developerAnalysis
        )

        let inspection = try await service.inspectCustomMCP(draft)
        #expect(inspection.version == "6.5.0")
        #expect(inspection.components == [.init(
            componentKey: "rea",
            displayName: "REA",
            kind: .mcp,
            toolNames: ["binary_session", "runtime_snapshot", "web_session"]
        )])
        try await service.installCustomMCP(
            draft,
            expectedFingerprint: inspection.fingerprint
        )

        let restoredService = NativeLocalConnectorService(
            configuration: configuration,
            ticketProvider: CustomPluginRejectingTicketProvider()
        )
        let mainChatComponents = try await restoredService.installedAgentPlugins(
            ownerUserID: "owner-offline-rea"
        )
        #expect(mainChatComponents.count == 1)
        #expect(mainChatComponents.first?.componentKey == "rea")
        #expect(mainChatComponents.first?.displayName == "REA")

        let catalog = try await restoredService.fetchPlugins(refresh: true)
        let installedPluginID = try recordPluginID(in: stateURL)
        let visibleREA = try #require(catalog.first(where: { $0.pluginID == installedPluginID }))
        #expect(visibleREA.runtimeStatus == .ready)
        #expect(visibleREA.runtimeToolCount == 3)
        #expect(visibleREA.runtimeError == nil)

        let restored = try NativeConnectorStateStore(stateURL: stateURL).load()
        let record = try #require(restored.installedPluginRecords?[installedPluginID])
        #expect(record.source == .customMCP)
        #expect(record.version == "6.5.0")
        #expect(NativePluginInstallationIntegrity.status(record: record) == .verified)
        let manifest = try NativePluginManifestLoader.loadManifest(
            from: URL(fileURLWithPath: record.installationPath, isDirectory: true)
                .appendingPathComponent("chatos.plugin.json")
        )
        #expect(Set(manifest.permissions.map(\.permission)).isSuperset(of: [
            "process.spawn",
            "network.client",
            "system.developer-tools.read",
            "browser.managed.launch",
            "artifact.create",
            "workspace.read",
        ]))
    }

    private func runNPMFixture(
        arguments: [String],
        timeout: TimeInterval,
        overflowMessage: String = "fixture overflow",
        timeoutMessage: String = "fixture timeout"
    ) async throws -> Data {
        try await NativePluginInstaller.runNPMProcess(
            executableURL: URL(fileURLWithPath: "/bin/sh"),
            arguments: arguments,
            currentDirectoryURL: nil,
            timeout: timeout,
            stderrLimit: 256 * 1_024,
            overflowMessage: overflowMessage,
            timeoutMessage: timeoutMessage,
            failureLabel: "fixture"
        )
    }

    private func installerProcessExists(_ processID: pid_t) -> Bool {
        Darwin.kill(processID, 0) == 0 || errno == EPERM
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

        let catalog = try await service.fetchPlugins(refresh: true)
        let visible = try #require(catalog.first(where: { $0.displayName == "Local Fixture" }))
        #expect(visible.runtimeStatus == .ready)
        #expect(visible.runtimeToolCount == 1)
        #expect(visible.runtimeLastCheckedAt != nil)
        #expect(visible.runtimeError == nil)

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

        let catalog = try await service.fetchPlugins(refresh: true)
        let visible = try #require(catalog.first(where: { $0.displayName == "Portable Hello" }))
        #expect(visible.runtimeStatus == nil)
        #expect(visible.runtimeToolCount == nil)
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

    @Test("a weak root manifest cannot hide a valid Codex Hook manifest")
    func codexHookManifestFallbackAndPreflight() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("CodexHookImport-\(UUID().uuidString)", isDirectory: true)
        let source = root.appendingPathComponent("source", isDirectory: true)
        let manifestFolder = source.appendingPathComponent(".codex-plugin", isDirectory: true)
        let hookFolder = source.appendingPathComponent("hooks", isDirectory: true)
        try FileManager.default.createDirectory(at: manifestFolder, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: hookFolder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data(#"{"name":"shadow-only"}"#.utf8)
            .write(to: source.appendingPathComponent("plugin.json"))
        try Data("""
        {
          "name": "generic-hook-fixture",
          "version": "5.3.2",
          "hooks": "./hooks/codex-hooks.json",
          "interface": {"displayName": "Generic Hook Fixture"}
        }
        """.utf8).write(to: manifestFolder.appendingPathComponent("plugin.json"))
        let command = "node -e \"process.stdout.write('{}')\""
        let hookDocument: [String: Any] = ["hooks": [
            "SessionStart": [[
                "matcher": "startup|resume|clear|compact",
                "hooks": [[
                    "type": "command",
                    "command": command,
                    "timeout": 5,
                    "statusMessage": "Loading mode",
                ]],
            ]],
            "SubagentStart": [["hooks": [[
                "type": "command", "command": "node subagent.js",
            ]]]],
            "UserPromptSubmit": [["hooks": [[
                "type": "command", "command": "node tracker.js",
            ]]]],
        ]]
        try JSONSerialization.data(withJSONObject: hookDocument, options: [.sortedKeys])
            .write(to: hookFolder.appendingPathComponent("codex-hooks.json"))

        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-hook", username: "hook", displayName: "Hook", role: "user")
        state.deviceID = "device-hook"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: CustomPluginRejectingTicketProvider()
        )

        let inspection = try await service.inspectLocalPlugin(atPath: source.path)
        #expect(inspection.displayName == "Generic Hook Fixture")
        #expect(inspection.version == "5.3.2")
        #expect(inspection.sourceSummary.contains("Codex Plugin"))
        #expect(inspection.sourceSummary.contains(".codex-plugin/plugin.json"))
        let hook = try #require(inspection.components.first(where: { $0.kind == .hook }))
        #expect(hook.hookDialect == "codex")
        #expect(hook.hookEvents == ["SessionStart", "SubagentStart", "UserPromptSubmit"])
        #expect(hook.hookCommands.count == 3)
        #expect(hook.hookCommands.first == command)
        #expect(hook.hookRuntimes.count == 1)
        #expect(hook.hookRuntimes.first?.hasPrefix("node: /") == true)
        #expect(inspection.requestedPermissions == ["process.spawn", "workspace.read"])

        try await service.installLocalPlugin(
            atPath: source.path,
            expectedFingerprint: inspection.fingerprint
        )
        let catalog = try await service.fetchPlugins(refresh: true)
        let installed = try #require(catalog.first(where: { $0.displayName == "Generic Hook Fixture" }))
        #expect(installed.installationIssue == nil)
    }

    @Test("Codex Hook parser rejects unsupported async handlers and path escapes")
    func codexHookParserRejectsUnsafeInputs() throws {
        let async = Data(#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"node hook.js","async":true}]}]}}"#.utf8)
        #expect(throws: NativeConnectorError.self) {
            _ = try NativeCodexHookBundleParser.parse(async)
        }

        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("CodexHookEscape-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        #expect(throws: NativeConnectorError.self) {
            _ = try NativeCodexHookBundleParser.load(
                contribution: .init(
                    componentKey: "escape",
                    dialect: "codex",
                    path: "../outside.json"
                ),
                installationRoot: root
            )
        }
    }

    @Test("manifest discovery aggregates every invalid candidate diagnostic")
    func manifestDiscoveryAggregatesDiagnostics() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PluginManifestDiagnostics-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent(".codex-plugin", isDirectory: true),
            withIntermediateDirectories: true
        )
        defer { try? FileManager.default.removeItem(at: root) }
        try Data(#"{"schemaVersion":3,"name":false}"#.utf8)
            .write(to: root.appendingPathComponent("chatos.plugin.json"))
        try Data(#"{"name":"missing-version"}"#.utf8)
            .write(to: root.appendingPathComponent(".codex-plugin/plugin.json"))
        try Data("not-json".utf8).write(to: root.appendingPathComponent("plugin.json"))
        do {
            _ = try NativeAgentPluginCompatibilityImporter.normalizePackage(at: root)
            Issue.record("expected every invalid identity candidate to be rejected")
        } catch {
            #expect(error.localizedDescription.contains("chatos.plugin.json"))
            #expect(error.localizedDescription.contains(".codex-plugin/plugin.json"))
            #expect(error.localizedDescription.contains("plugin.json"))
            #expect(error.localizedDescription.contains("name 和 version"))
        }
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
        let restrictedDraft = LocalConnectorCustomMCPDraft(
            displayName: "REA",
            description: "Reverse engineer anything from your terminal or agent with one CLI and MCP server.",
            componentKey: "rea",
            packageName: "rea-agents",
            packageVersion: "6.5.0",
            binaryName: "rea",
            arguments: ["mcp"],
            accessProfile: .restricted
        )
        let restrictedInspection = try await service.inspectCustomMCP(restrictedDraft)
        try await service.installCustomMCP(
            restrictedDraft,
            expectedFingerprint: restrictedInspection.fingerprint
        )

        let draft = LocalConnectorCustomMCPDraft(
            displayName: "REA",
            description: "Reverse engineer anything from your terminal or agent with one CLI and MCP server.",
            componentKey: "rea",
            packageName: "rea-agents",
            packageVersion: "6.5.0",
            binaryName: "rea",
            arguments: ["mcp"],
            accessProfile: .developerAnalysis
        )

        let inspection = try await service.inspectCustomMCP(draft)
        let component = try #require(inspection.components.first)
        #expect(inspection.version == "6.5.0")
        #expect(component.componentKey == "rea")
        #expect(component.toolNames.count >= 200)
        #expect(component.toolNames.count <= NativeLocalConnectorService.maximumAgentPluginToolCount)
        #expect(component.toolNames.contains("binary_session"))

        try await service.installCustomMCP(
            draft,
            expectedFingerprint: inspection.fingerprint
        )
        let installed = try await service.installedAgentPlugins(ownerUserID: "owner-rea")
        #expect(installed.count == 1)
        #expect(installed.first?.componentKey == "rea")
        #expect(installed.first?.displayName == "REA")

        let catalog = try await service.fetchPlugins(refresh: true)
        let installedPluginID = try recordPluginID(in: stateURL)
        let visibleREA = try #require(catalog.first(where: { $0.pluginID == installedPluginID }))
        #expect(visibleREA.runtimeStatus == .ready)
        #expect((visibleREA.runtimeToolCount ?? 0) >= 200)
        #expect(visibleREA.runtimeLastCheckedAt != nil)
        #expect(visibleREA.runtimeError == nil)

        let restored = try NativeConnectorStateStore(stateURL: stateURL).load()
        let record = try #require(restored.installedPluginRecords?.values.first)
        #expect(record.source == .customMCP)
        #expect(record.version == "6.5.0")
        #expect(NativePluginInstallationIntegrity.status(record: record) == .verified)
        let manifest = try NativePluginManifestLoader.loadManifest(
            from: URL(fileURLWithPath: record.installationPath, isDirectory: true)
                .appendingPathComponent("chatos.plugin.json")
        )
        #expect(Set(manifest.permissions.map(\.permission)).contains(
            "system.developer-tools.read"
        ))
    }

    private static func installOfflineREAFixture(in packageRoot: URL) async throws {
        let request = try JSONSerialization.jsonObject(
            with: Data(contentsOf: packageRoot.appendingPathComponent("package.json"))
        ) as? [String: Any]
        let dependencies = request?["dependencies"] as? [String: String]
        guard dependencies == ["rea-agents": "6.5.0"] else {
            throw NativeConnectorError.pluginInstallation(
                "离线 npm fixture 收到了非固定版本的安装请求"
            )
        }
        let dependencyRoot = packageRoot
            .appendingPathComponent("node_modules/rea-agents", isDirectory: true)
        let executableRoot = dependencyRoot.appendingPathComponent("dist", isDirectory: true)
        try FileManager.default.createDirectory(
            at: executableRoot,
            withIntermediateDirectories: true
        )
        try Data("""
        {
          "name": "rea-agents",
          "version": "6.5.0",
          "description": "Offline REA integration fixture",
          "bin": {"rea": "dist/rea.js"}
        }
        """.utf8).write(
            to: dependencyRoot.appendingPathComponent("package.json"),
            options: .atomic
        )
        try Data("""
        const readline = require("node:readline");
        const lines = readline.createInterface({ input: process.stdin });
        lines.on("line", (line) => {
          const request = JSON.parse(line);
          if (request.id === undefined) return;
          let result = {};
          if (request.method === "initialize") {
            result = {
              protocolVersion: "2024-11-05",
              capabilities: { tools: {} },
              serverInfo: { name: "rea", version: "6.5.0" }
            };
          } else if (request.method === "tools/list") {
            result = { tools: [
              { name: "binary_session", description: "Inspect a binary", inputSchema: { type: "object", properties: {} } },
              { name: "runtime_snapshot", description: "Inspect a runtime", inputSchema: { type: "object", properties: {} } },
              { name: "web_session", description: "Inspect a browser session", inputSchema: { type: "object", properties: {} } }
            ] };
          }
          process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\\n");
        });
        """.utf8).write(
            to: executableRoot.appendingPathComponent("rea.js"),
            options: .atomic
        )
    }

    private func recordPluginID(in stateURL: URL) throws -> String {
        let restored = try NativeConnectorStateStore(stateURL: stateURL).load()
        return try #require(restored.installedPluginRecords?.values.first?.pluginID)
    }
}

private struct CustomPluginRejectingTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String {
        throw URLError(.userAuthenticationRequired)
    }
}
