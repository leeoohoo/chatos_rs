import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeAgentPluginToolProviderTests: XCTestCase {
    func testTaskBuiltinDiscoveryRestoresFrozenSkillsWithoutBrokerExecution() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("task-builtin-skills-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let stateURL = root.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "alice", username: "alice", displayName: nil, role: "user")
        state.deviceID = "device-1"
        try JSONEncoder().encode(state).write(to: stateURL)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!, stateURL: stateURL
            ),
            ticketProvider: AgentPluginTestTicketProvider()
        )
        let external = NativeLocalAgentExternalMCPConfig(
            resourceID: "mcp-1", serverName: "external", url: URL(string: "https://mcp.example.test/rpc")!,
            headers: [:], tools: [.init(publicName: "external_mcp__external__search", upstreamName: "search",
                description: "Search", inputSchema: .object(["type": .string("object")]))]
        )
        let original = try NativeLocalConnectorService.taskExecutionSkillSnapshot(externalMCPConfigs: [external]).bundle
        // Simulate a Run created with a different app bundle, including its resource manifest.
        let frozen = LocalAgentSkillBundleSnapshot(
            bundleRevision: "frozen-revision",
            documents: original.documents.map { document in
                guard document.name == "chatos-project-read" else { return document }
                return .init(
                    skillRef: document.skillRef, name: document.name, role: document.role,
                    description: "Frozen read guidance", instructions: "Frozen instructions for this Run",
                    instructionsSHA256: String(repeating: "a", count: 64),
                    resources: [.init(
                        relativePath: "references/frozen.md", content: "A😀e\u{301}中",
                        sha256: String(repeating: "b", count: 64)
                    )]
                )
            },
            toolSkillBindings: original.toolSkillBindings
        )
        func provider(
            capabilities: Set<LocalAgentTodoBuiltinCapability>, refs: Set<String>, externalIDs: Set<String> = []
        ) async throws -> any AgentToolProvider {
            try await service.makeTaskExecutionCapabilityToolProvider(
                ownerUserID: "alice", runID: "frozen-run", conversationID: "contact-1",
                projectContext: .device, executionRootURL: root,
                workspaceScopeID: "contact:contact-1", builtinCapabilities: capabilities,
                pluginIDs: [], productSkillBundle: frozen, selectedExternalMCPIDs: externalIDs, preactivatedSkillRefs: refs
            )
        }
        let refs: Set<String> = [
            "product-skill:chatos-project-files", "product-skill:chatos-project-read",
            "product-skill:chatos-terminal", "product-skill:unknown",
        ]
        let broker = try await provider(capabilities: [.projectRead], refs: refs)
        let activation = try await broker.execute(.init(
            id: "restored-activation", name: "agent_skill_activate",
            arguments: #"{"skill_ref":"product-skill:chatos-project-read"}"#
        ))
        XCTAssertFalse(activation.isError)
        XCTAssertTrue(activation.content.contains("Frozen instructions for this Run"))
        XCTAssertTrue(activation.content.contains("references/frozen.md"))
        let page = try await broker.execute(.init(
            id: "restored-page", name: "agent_skill_read_resource",
            arguments: #"{"skill_ref":"product-skill:chatos-project-read","relative_path":"references/frozen.md","offset":1,"max_chars":3}"#
        ))
        XCTAssertFalse(page.isError)
        let pageObject = try JSONDecoder().decode(NativeJSONValue.self, from: Data(page.content.utf8)).jsonObject
        XCTAssertEqual(pageObject?["content"]?.jsonString, "😀e\u{301}")
        XCTAssertEqual(pageObject?["next_offset"], .number(4))
        let unrelated = try await broker.execute(.init(
            id: "unrelated-restored", name: "agent_skill_activate",
            arguments: #"{"skill_ref":"product-skill:chatos-terminal"}"#
        ))
        XCTAssertTrue(unrelated.isError, "Restored refs must not add unselected capability Skills")
        let unavailable = try await broker.execute(.init(
            id: "unselected-activation", name: "agent_skill_activate",
            arguments: #"{"skill_ref":"product-skill:chatos-terminal"}"#
        ))
        XCTAssertTrue(unavailable.isError)

        let externalOnly = try await provider(capabilities: [], refs: [], externalIDs: ["mcp-1"])
        let externalActivation = try await externalOnly.execute(.init(
            id: "activate-external", name: "agent_skill_activate",
            arguments: #"{"skill_ref":"product-skill:chatos-external-mcp"}"#
        ))
        XCTAssertFalse(externalActivation.isError)
        XCTAssertTrue(externalActivation.content.contains("frozen"))
        let notSelected = try await broker.execute(.init(
            id: "reject-unselected-external", name: "agent_skill_activate",
            arguments: #"{"skill_ref":"product-skill:chatos-external-mcp"}"#
        ))
        XCTAssertTrue(notSelected.isError)

        let allBuiltins = try await provider(
            capabilities: [.projectRead, .projectWrite, .terminal],
            refs: Set(frozen.documents.map(\.skillRef))
        )
        let description = try await allBuiltins.execute(.init(
            id: "describe-builtins", name: "capability_describe",
            arguments: #"{"plugin_option":"builtin_1"}"#
        ))
        XCTAssertFalse(description.isError)
        let descriptor = try JSONDecoder().decode(NativeJSONValue.self, from: Data(description.content.utf8))
        XCTAssertEqual(descriptor.jsonObject?["binding_schema_version"], .number(1))
        XCTAssertEqual(descriptor.jsonObject?["provider_id"], .string(ProductToolProviderID.capabilityBroker))
        let nullableIndex = try await allBuiltins.execute(.init(
            id: "describe-null-unset", name: "capability_describe",
            arguments: #"{"plugin_option":"builtin_1","tool_option":null,"query":null,"offset":null,"limit":null,"schema_offset":null}"#
        ))
        XCTAssertFalse(nullableIndex.isError)
        XCTAssertEqual(try JSONDecoder().decode(NativeJSONValue.self, from: Data(nullableIndex.content.utf8))
            .jsonObject?["view"], .string("catalog"))
        let tools = try await readCapabilityTestDefinitions(allBuiltins, pluginOption: "builtin_1")
        for name in ["read_file", "commit_edit_session", "execute_command", "process_write", "process_kill"] {
            let tool = try XCTUnwrap(tools.first { $0.jsonObject?["name"]?.jsonString == name }?.jsonObject)
            XCTAssertEqual(tool["invocation_mode"]?.jsonString, "direct_task_tool")
            let token = try XCTUnwrap(tool["tool_option"]?.jsonString)
            let outcome = try await allBuiltins.execute(.init(
                id: "reject-\(name)", name: "capability_invoke",
                arguments: """
                {"plugin_option":"builtin_1","tool_option":"\(token)","arguments":{}}
                """
            ))
            XCTAssertTrue(outcome.isError)
            XCTAssertTrue(outcome.content.contains("原生工具"))
        }
        let skillOnly = try await provider(capabilities: [], refs: [])
        let discovery = try await skillOnly.execute(.init(
            id: "run-bound-guidance", name: "agent_skill_activate",
            arguments: #"{"skill_ref":"product-skill:chatos-capability-discovery"}"#
        ))
        XCTAssertFalse(discovery.isError)
        XCTAssertTrue(discovery.content.contains("references/search-activation-and-recovery.md"))
        let unactivated = try await provider(capabilities: [.projectRead], refs: [])
        let denied = try await unactivated.execute(.init(
            id: "not-restored", name: "agent_skill_read_resource",
            arguments: #"{"skill_ref":"product-skill:chatos-project-read","relative_path":"references/frozen.md"}"#
        ))
        XCTAssertTrue(denied.isError)
        let host = FrozenTaskSkillHostStub(bundle: frozen, refs: refs)
        let executor = NativeLocalAgentPluginToolExecutor(
            connector: service, host: host, idleExpiration: .milliseconds(20)
        )
        let context = NativeLocalAgentTaskExecutionContext(
            conversationID: "contact-1", projectID: nil, applicationContext: .device,
            resolvedPath: nil, executionRootURL: root, workspaceScopeID: "contact:contact-1",
            toolAuthorization: .init(
                requiresExecution: false, enabledBuiltinKinds: ["CodeMaintainerRead"],
                pluginKeys: [], isLegacyUnrestricted: false
            )
        )
        for _ in 0..<2 {
            let result = try await executor.execute(
                ownerUserID: "alice",
                invocation: platformInvocation(
                    runID: "frozen-run", name: "agent_skill_activate",
                    arguments: .object(["skill_ref": .string("product-skill:chatos-project-read")])
                ),
                context: context,
                arguments: ["skill_ref": .string("product-skill:chatos-project-read")]
            )
            XCTAssertEqual(result.jsonObject?["is_error"]?.jsonBool, false)
            XCTAssertTrue(result.canonicalJSONString.contains("references/frozen.md"))
            try await Task.sleep(for: .milliseconds(80))
        }
        let queries = await host.queryTypes()
        XCTAssertEqual(queries.filter { $0 == "get_run_skill_bundle" }.count, 2)
        XCTAssertEqual(queries.filter { $0 == "get_run_skill_activations" }.count, 2)
        await executor.reset()
    }

    func testTodoAuthorizationCatalogUsesExecutorRuntimeToolDefinitions() {
        let cases: [(LocalAgentTodoBuiltinCapability, [NativeJSONValue])] = [
            (.projectRead, NativeMCPCodeReadTools.toolDefinitions),
            (.projectWrite, NativeMCPCodeWriteStore.toolDefinitions),
            (.terminal, NativeMCPTerminalStore.toolDefinitions),
        ]

        for (capability, definitions) in cases {
            let descriptor = LocalAgentTodoAuthorizationCatalog.descriptor(for: capability)
            XCTAssertEqual(descriptor.capability, capability)
            XCTAssertEqual(
                descriptor.toolNames,
                definitions.compactMap { $0.jsonObject?["name"]?.jsonString }
            )
            XCTAssertFalse(descriptor.displayName.isEmpty)
            XCTAssertFalse(descriptor.detail.isEmpty)
            XCTAssertFalse(descriptor.toolNames.isEmpty)
        }
    }

    func testTodoProjectWriteCommitUsesExecutionPlanAuthorizationWithoutSecondApproval() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("todo-project-write-\(UUID().uuidString)", isDirectory: true)
        let project = root.appendingPathComponent("project", isDirectory: true)
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "alice", username: "alice", displayName: nil, role: "user")
        state.deviceID = "device-1"
        state.workspaces = [.init(
            id: "workspace-1",
            alias: "test",
            absoluteRoot: root.path,
            fingerprint: "test"
        )]
        try JSONEncoder().encode(state).write(to: stateURL)

        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: AgentPluginTestTicketProvider()
        )
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: "room-1",
            agentID: "agent-1",
            deliveryID: "delivery-1",
            triggerMessageID: "message-1",
            rootMessageID: "message-1",
            runID: "run-1",
            hopCount: 0,
            lane: .executor
        )
        let resolvedProject = try await service.resolveProjectPath(project.path)
        let provider = NativeAgentBuiltinToolProvider(
            service: service,
            runContext: context,
            resolvedProject: resolvedProject,
            allowedCapabilities: [.projectWrite]
        )

        let opened = try await provider.execute(.init(
            id: "open-1",
            name: "open_edit_session",
            arguments: #"{"purpose":"test"}"#
        ))
        XCTAssertFalse(opened.isError)
        let openedValue = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(opened.content.utf8)
        )
        let sessionID = try XCTUnwrap(
            openedValue.jsonObject?["result"]?.jsonObject?["session_id"]?.jsonString
        )
        let staged = try await provider.execute(.init(
            id: "stage-1",
            name: "stage_edit_batch",
            arguments: NativeJSONValue.object([
                "session_id": .string(sessionID),
                "operations": .array([.object([
                    "kind": .string("write"),
                    "path": .string("docs/delivered.md"),
                    "content": .string("delivered\n"),
                    "expected_sha256": .null,
                ])]),
            ]).canonicalJSONString
        ))
        XCTAssertFalse(staged.isError)

        let committed = try await provider.execute(.init(
            id: "commit-1",
            name: "commit_edit_session",
            arguments: NativeJSONValue.object([
                "session_id": .string(sessionID),
            ]).canonicalJSONString
        ))

        XCTAssertFalse(committed.isError)
        XCTAssertTrue(committed.content.contains("docs/delivered.md"))
        XCTAssertEqual(
            try String(
                contentsOf: project.appendingPathComponent("docs/delivered.md"),
                encoding: .utf8
            ),
            "delivered\n"
        )
        let pendingApprovals = try await service.fetchPendingApprovals()
        XCTAssertTrue(pendingApprovals.isEmpty)
    }

    func testInstalledPluginCatalogExcludesUntrustedInstallations() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-plugin-integrity-\(UUID().uuidString)")
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        try FileManager.default.createDirectory(at: installation, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let manifestURL = installation.appendingPathComponent("chatos.plugin.json")
        try Data(#"{"schemaVersion":3,"name":"test-agent-plugin","version":"1.0.0","description":"test","mcpServers":{"test":{"type":"stdio","bin":"test","args":[]}},"permissions":[]}"#.utf8)
            .write(to: manifestURL)

        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "alice", username: "alice", displayName: nil, role: "user")
        state.deviceID = "device-1"
        state.installedPluginIDs = ["plugin-1"]
        state.installedPluginRecords = [
            "plugin-1": .init(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: installation.path,
                installedAt: "2026-10-07T00:00:00Z"
            ),
        ]
        try JSONEncoder().encode(state).write(to: stateURL)

        var service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: AgentPluginTestTicketProvider()
        )
        var installed = try await service.installedAgentPlugins(ownerUserID: "alice")
        XCTAssertTrue(installed.isEmpty, "Legacy installs without trusted hashes must be hidden")

        state.installedPluginRecords?["plugin-1"]?.packageFileSHA256 =
            try NativePluginInstallationIntegrity.snapshot(
                installationURL: installation,
                maximumFiles: 100,
                maximumBytes: 1_024 * 1_024
            )
        try Data("\n".utf8).write(to: manifestURL, options: .atomic)
        try JSONEncoder().encode(state).write(to: stateURL, options: .atomic)
        service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: AgentPluginTestTicketProvider()
        )
        installed = try await service.installedAgentPlugins(ownerUserID: "alice")
        XCTAssertTrue(installed.isEmpty, "Modified installs must be hidden from task planning")
    }

    func testInstalledPluginRunsDirectlyWithoutRelayRequest() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-plugin-provider-\(UUID().uuidString)")
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let bin = installation.appendingPathComponent("bin", isDirectory: true)
        let skill = installation.appendingPathComponent(
            "skills/test-agent-plugin",
            isDirectory: true
        )
        let references = skill.appendingPathComponent("references", isDirectory: true)
        let project = root.appendingPathComponent("project", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: references, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let executable = bin.appendingPathComponent("test-mcp")
        let script = #"""
        #!/bin/sh
        while IFS= read -r line; do
          case "$line" in
            *'"method":"initialize"'*)
              printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"instructions":"test"}}'
              ;;
            *'"method":"tools/list"'*)
              printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"local_echo","description":"Echo locally","inputSchema":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false},"annotations":{"readOnlyHint":true},"_meta":{"chatos/skillGate":{"allOf":["test-agent-plugin"],"selectByArgument":{"pointer":"/value","map":{"hello":"test-agent-plugin","fail":"test-agent-plugin","hello again":"test-agent-plugin","restored":"test-agent-plugin","task":"test-agent-plugin","slow":"test-agent-plugin"}}}}}]}}'
              ;;
            *'"method":"tools/call"'*)
              request_id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
              case "$line" in
                *'"value":"fail"'*)
                  printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":${request_id},\"error\":{\"code\":-32602,\"message\":\"requirements.users is invalid at /Users/alice/private.json; api_key=secret-value\"}}"
                  ;;
                *'"value":"slow"'*)
                  sleep 0.2
                  printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":${request_id},\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"local-plugin-ok project-1\"}]}}"
                  ;;
                *)
                  printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":${request_id},\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"local-plugin-ok project-1\"}]}}"
                  ;;
              esac
              ;;
          esac
        done
        """#
        try Data(script.utf8).write(to: executable)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: executable.path
        )
        let skillMarkdown = #"""
        ---
        name: test-agent-plugin
        description: Use the test Agent Plugin echo tool.
        metadata:
          chatos.role: router
        ---

        # Test Agent Plugin

        Activate this fixed Release Skill before calling `local_echo`.

        Legacy host guidance: call `skill_skill_activate` and pass activation evidence.

        Read [references/examples.md](references/examples.md) only when an exact echo example is needed.
        """#
        try Data(skillMarkdown.utf8).write(to: skill.appendingPathComponent("SKILL.md"))
        try Data("Echo a bounded value; never include internal IDs.".utf8).write(
            to: references.appendingPathComponent("examples.md")
        )
        let manifest = #"""
        {
          "schemaVersion": 3,
          "name": "test-agent-plugin",
          "version": "1.0.0",
          "description": "test",
          "skills": [{"path":"skills/test-agent-plugin"}],
          "permissions": [
            {"permission":"process.spawn","required":true,"components":["test-mcp"]}
          ],
          "mcpServers": {
            "test-mcp": {"type":"stdio","bin":"test-mcp","args":[],"env":{}}
          }
        }
        """#
        try Data(manifest.utf8).write(
            to: installation.appendingPathComponent("chatos.plugin.json")
        )
        let packageFileSHA256 = try NativePluginInstallationIntegrity.snapshot(
            installationURL: installation,
            maximumFiles: 20_000,
            maximumBytes: 512 * 1_024 * 1_024
        )

        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "alice", username: "alice", displayName: nil, role: "user")
        state.deviceID = "device-1"
        state.workspaces = [.init(
            id: "workspace-1",
            alias: "test",
            absoluteRoot: root.path,
            fingerprint: "test"
        )]
        state.installedPluginIDs = ["plugin-1"]
        state.installedPluginRecords = [
            "plugin-1": .init(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: installation.path,
                installedAt: "2026-09-16T00:00:00Z",
                packageFileSHA256: packageFileSHA256
            ),
        ]
        try JSONEncoder().encode(state).write(to: stateURL)

        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: AgentPluginTestTicketProvider()
        )
        let runContext = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: "room-1",
            agentID: "agent-1",
            deliveryID: "delivery-1",
            triggerMessageID: "message-1",
            rootMessageID: "message-1",
            runID: "run-1",
            hopCount: 0,
            lane: .executor
        )
        let installed = try await service.installedAgentPlugins(ownerUserID: "alice")
        XCTAssertEqual(installed.map(\.id), ["plugin-1"])
        XCTAssertEqual(installed.first?.displayName, "test-agent-plugin")
        let providers = try await service.makeAgentPluginSkillToolProviders(
            ownerUserID: "alice",
            runContext: runContext,
            pluginIDs: ["plugin-1"],
            projectContext: .init(
                projectID: "project-1",
                projectName: "Test",
                projectRoot: project.path
            )
        )
        XCTAssertEqual(providers.count, 1)
        let definitions = try await providers[0].definitions()
        XCTAssertEqual(definitions.map(\.name), ["local_echo"])
        XCTAssertEqual(definitions.first?.effect, .readOnly)
        XCTAssertNotNil(definitions.first?.pluginSkillGate)


        let blockedDirect = try await providers[0].execute(.init(
            id: "call-1",
            name: "local_echo",
            arguments: #"{"value":"hello"}"#
        ))
        XCTAssertTrue(blockedDirect.isError)
        XCTAssertTrue(blockedDirect.content.contains("test-agent-plugin"))
        _ = try await providers[0].pluginSkillSession.activate(named: "test-agent-plugin")
        let outcome = try await providers[0].execute(.init(
            id: "call-2",
            name: "local_echo",
            arguments: #"{"value":"hello"}"#
        ))
        XCTAssertFalse(outcome.isError)
        XCTAssertTrue(outcome.content.contains("local-plugin-ok"))
        let rejected = try await providers[0].execute(.init(
            id: "call-rejected",
            name: "local_echo",
            arguments: #"{"value":"fail"}"#
        ))
        XCTAssertTrue(rejected.isError)
        XCTAssertFalse(rejected.madeProgress)
        XCTAssertTrue(rejected.content.contains("requirements.users is invalid"))
        XCTAssertFalse(rejected.content.contains("/Users/alice"))
        XCTAssertFalse(rejected.content.contains("secret-value"))
        let recovered = try await providers[0].execute(.init(
            id: "call-recovered",
            name: "local_echo",
            arguments: #"{"value":"hello again"}"#
        ))
        XCTAssertFalse(recovered.isError)
        XCTAssertTrue(recovered.content.contains("local-plugin-ok"))

        let broker = try await service.makeAgentCapabilityToolProvider(
            ownerUserID: "alice",
            runContext: runContext,
            projectContext: .init(
                projectID: "project-1",
                projectName: "Test",
                projectRoot: project.path
            ),
            executionPlan: .init(
                builtinCapabilities: [
                    .projectRead, .projectWrite, .terminal,
                ],
                plugins: [.init(pluginID: "plugin-1", displayName: "test-agent-plugin")]
            )
        )
        let brokerDefinitions = try await broker.definitions()
        XCTAssertEqual(
            Set(brokerDefinitions.map(\.name)),
            [
                "capability_search", "capability_describe", "agent_skill_activate",
                "agent_skill_read_resource", "capability_invoke",
            ]
        )
        let brokerCoverage = ToolSkillCoverageCatalog.product.audit(
            brokerDefinitions.map {
                .init(
                    providerID: $0.providerID,
                    toolName: $0.name,
                    skillBindingID: $0.skillBindingID
                )
            }
        )
        XCTAssertEqual(brokerCoverage.coveredTools, 5)
        XCTAssertTrue(brokerCoverage.isComplete)
        for removedAlias in ["capability_skill_activate", "capability_skill_read_resource"] {
            let rejected = try await broker.execute(.init(
                id: "removed-\(removedAlias)",
                name: removedAlias,
                arguments: #"{"plugin_option":"plugin_1","skill_name":"test-agent-plugin"}"#
            ))
            XCTAssertTrue(rejected.isError)
            XCTAssertTrue(rejected.content.contains("能力发现工具不可用"))
        }

        let search = try await broker.execute(.init(
            id: "search-1",
            name: "capability_search",
            arguments: #"{"query":"test"}"#
        ))
        XCTAssertTrue(search.content.contains("plugin_1"))
        XCTAssertTrue(search.content.contains("test-agent-plugin"))
        XCTAssertFalse(search.content.contains("plugin-1"))

        let guessedSkillActivation = try await broker.execute(.init(
            id: "activate-undisclosed-plugin-skill",
            name: "agent_skill_activate",
            arguments: #"{"skill_ref":"plugin-skill:plugin_1:test-agent-plugin"}"#
        ))
        XCTAssertTrue(
            guessedSkillActivation.isError,
            "A Plugin Skill ref must not be activatable until capability_describe disclosed it in this Run"
        )

        let description = try await broker.execute(.init(
            id: "describe-1",
            name: "capability_describe",
            arguments: #"{"plugin_option":"plugin_1","tool_option":"tool_1"}"#
        ))
        XCTAssertTrue(description.content.contains("tool_1"))
        XCTAssertTrue(description.content.contains("local_echo"))
        XCTAssertTrue(description.content.contains("test-agent-plugin"))
        XCTAssertFalse(description.content.contains("project-1"))

        let pluginDescription = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(description.content.utf8)
        ).jsonObject
        let pluginTools = try XCTUnwrap(pluginDescription?["tools"]?.jsonArray)
        let echoTool = try XCTUnwrap(pluginTools.first?.jsonObject)
        XCTAssertEqual(pluginDescription?["provider_id"], .string("plugin:plugin-1"))
        XCTAssertEqual(pluginDescription?["binding_schema_version"], .number(1))
        let gate = try XCTUnwrap(echoTool["skill_gate"]?.jsonObject)
        XCTAssertEqual(gate["all_of"], .array([.string("plugin-skill:plugin_1:test-agent-plugin")]))
        XCTAssertEqual(gate["select_by_argument"]?.jsonObject?["pointer"], .string("/value"))
        XCTAssertEqual(gate["select_by_argument"]?.jsonObject?["map"]?.jsonObject?["hello"],
            .string("plugin-skill:plugin_1:test-agent-plugin"))
        let echoToolOption = try XCTUnwrap(echoTool["tool_option"]?.jsonString)
        XCTAssertNil(echoTool["required_skills"])
        XCTAssertEqual(
            echoTool["required_skill_refs"]?.jsonArray?.compactMap(\.jsonString),
            ["plugin-skill:plugin_1:test-agent-plugin"]
        )
        XCTAssertEqual(
            pluginDescription?["skills"]?.jsonArray?.first?.jsonObject?["skill_ref"]?.jsonString,
            "plugin-skill:plugin_1:test-agent-plugin"
        )

        let gatedPlugin = try await broker.execute(.init(
            id: "invoke-1",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"plugin_1","tool_option":"\(echoToolOption)","arguments":{"value":"hello"}}
            """
        ))
        XCTAssertTrue(gatedPlugin.isError)
        XCTAssertTrue(gatedPlugin.content.contains("test-agent-plugin"))

        let pluginActivation = try await broker.execute(.init(
            id: "activate-plugin-skill",
            name: "agent_skill_activate",
            arguments: #"{"skill_ref":"plugin-skill:plugin_1:test-agent-plugin"}"#
        ))
        XCTAssertFalse(pluginActivation.isError)
        XCTAssertTrue(pluginActivation.content.contains("Test Agent Plugin"))
        XCTAssertTrue(pluginActivation.content.contains("references/examples.md"))
        let pluginReference = try await broker.execute(.init(
            id: "read-plugin-reference",
            name: "agent_skill_read_resource",
            arguments: #"{"skill_ref":"plugin-skill:plugin_1:test-agent-plugin","relative_path":"references/examples.md","max_chars":12000}"#
        ))
        XCTAssertFalse(pluginReference.isError)
        XCTAssertTrue(pluginReference.content.contains("bounded value"))

        let invoked = try await broker.execute(.init(
            id: "invoke-2",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"plugin_1","tool_option":"\(echoToolOption)","arguments":{"value":"hello"}}
            """
        ))
        XCTAssertFalse(invoked.isError)
        XCTAssertTrue(invoked.content.contains("local-plugin-ok"))
        XCTAssertFalse(invoked.content.contains("project-1"))
        XCTAssertTrue(invoked.content.contains("[internal-project]"))
        let rejectedInvocation = try await broker.execute(.init(
            id: "invoke-rejected",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"plugin_1","tool_option":"\(echoToolOption)","arguments":{"value":"fail"}}
            """
        ))
        XCTAssertTrue(rejectedInvocation.isError)
        XCTAssertFalse(rejectedInvocation.madeProgress)
        XCTAssertTrue(rejectedInvocation.content.contains("requirements.users is invalid"))
        XCTAssertFalse(rejectedInvocation.content.contains("/Users/alice"))
        XCTAssertFalse(rejectedInvocation.content.contains("secret-value"))
        let recoveredInvocation = try await broker.execute(.init(
            id: "invoke-recovered",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"plugin_1","tool_option":"\(echoToolOption)","arguments":{"value":"hello again"}}
            """
        ))
        XCTAssertFalse(recoveredInvocation.isError)
        XCTAssertTrue(recoveredInvocation.content.contains("local-plugin-ok"))

        let restoredContext = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: "room-1",
            agentID: "local-task-execution",
            deliveryID: "run-restored",
            triggerMessageID: "run-restored",
            rootMessageID: "run-restored",
            runID: "run-restored",
            hopCount: 0,
            lane: .executor
        )
        let restoredBroker = NativeAgentCapabilityToolProvider(
            service: service,
            ownerUserID: "alice",
            runContext: restoredContext,
            projectContext: .init(
                projectID: "project-1",
                projectName: "Test",
                projectRoot: project.path
            ),
            resolvedProject: try await service.resolveProjectPath(project.path),
            builtinCapabilities: [],
            installedPlugins: installed,
            fallbackToSelectedOptions: true,
            preactivatedSkillRefs: ["plugin-skill:plugin_1:test-agent-plugin"]
        )
        let restoredDescription = try await restoredBroker.execute(.init(
            id: "describe-restored",
            name: "capability_describe",
            arguments: #"{"plugin_option":"plugin_1","tool_option":"tool_1"}"#
        ))
        let restoredDescriptionObject = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(restoredDescription.content.utf8)
        ).jsonObject
        let restoredToolOption = try XCTUnwrap(
            restoredDescriptionObject?["tools"]?.jsonArray?.first?.jsonObject?["tool_option"]?.jsonString
        )
        let restoredInvocation = try await restoredBroker.execute(.init(
            id: "invoke-restored",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"plugin_1","tool_option":"\(restoredToolOption)","arguments":{"value":"restored"}}
            """
        ))
        XCTAssertFalse(restoredInvocation.isError)
        XCTAssertTrue(restoredInvocation.content.contains("local-plugin-ok"))

        let builtinDescription = try await broker.execute(.init(
            id: "describe-builtin",
            name: "capability_describe",
            arguments: #"{"plugin_option":"builtin_1"}"#
        ))
        let builtinObject = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(builtinDescription.content.utf8)
        ).jsonObject
        XCTAssertEqual(builtinObject?["view"], .string("catalog"))
        let builtinTools = try await readCapabilityTestDefinitions(broker, pluginOption: "builtin_1")
        let processList = try XCTUnwrap(builtinTools.first {
            $0.jsonObject?["name"]?.jsonString == "process_list"
        }?.jsonObject)
        let processListOption = try XCTUnwrap(processList["tool_option"]?.jsonString)
        XCTAssertNil(processList["required_skills"])
        XCTAssertEqual(
            processList["required_skill_refs"]?.jsonArray?.compactMap(\.jsonString),
            [
                "product-skill:chatos-terminal",
                "product-skill:chatos-terminal-process-observation",
            ]
        )
        let listDirectory = try XCTUnwrap(builtinTools.first {
            $0.jsonObject?["name"]?.jsonString == "list_dir"
        }?.jsonObject)
        XCTAssertNil(listDirectory["required_skills"])
        XCTAssertNil(builtinTools.first {
            $0.jsonObject?["name"]?.jsonString == "skill_activate"
        })
        XCTAssertNil(builtinTools.first {
            $0.jsonObject?["name"]?.jsonString == "requirement_survey_get"
        })

        let gated = try await broker.execute(.init(
            id: "invoke-gated",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"builtin_1","tool_option":"\(processListOption)","arguments":{}}
            """
        ))
        XCTAssertTrue(gated.isError)
        XCTAssertTrue(gated.content.contains("chatos-terminal"))

        for skillName in ["chatos-terminal", "chatos-terminal-process-observation"] {
            let activated = try await broker.execute(.init(
                id: "activate-\(skillName)",
                name: "agent_skill_activate",
                arguments: #"{"skill_ref":"product-skill:\#(skillName)"}"#
            ))
            XCTAssertFalse(activated.isError)
            XCTAssertTrue(activated.content.contains(skillName))
        }

        let reference = try await broker.execute(.init(
            id: "read-terminal-reference",
            name: "agent_skill_read_resource",
            arguments: #"{"skill_ref":"product-skill:chatos-terminal-process-observation","relative_path":"references/scenarios.md","max_chars":2000}"#
        ))
        XCTAssertFalse(reference.isError)
        XCTAssertTrue(reference.content.contains("Background build"))

        let observed = try await broker.execute(.init(
            id: "invoke-observed",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"builtin_1","tool_option":"\(processListOption)","arguments":{}}
            """
        ))
        XCTAssertFalse(observed.isError)
        XCTAssertTrue(observed.content.contains("process_count"))

        do {
            _ = try await service.makeTaskExecutionCapabilityToolProvider(
                ownerUserID: "alice",
                runID: "task-run-stale-release",
                conversationID: "conversation-1",
                projectContext: .init(
                    projectID: "project-1",
                    projectName: "Test",
                    projectRoot: project.path
                ),
                executionRootURL: project,
                workspaceScopeID: "workspace-1",
                pluginIDs: ["plugin-1"],
                pluginReleaseSnapshots: [
                    "plugin-1": .init(
                        pluginKey: "plugin-1",
                        pluginID: "plugin-1",
                        releaseID: "stale-release",
                        version: "1.0.0",
                        artifactSHA256: String(repeating: "a", count: 64)
                    ),
                ]
            )
            XCTFail("stale Plugin Release snapshot must be rejected")
        } catch let error as NativePluginRuntimeError {
            guard case .invalidRequest = error else {
                return XCTFail("unexpected stale Release error: \(error)")
            }
        }

        let taskBroker = try await service.makeTaskExecutionCapabilityToolProvider(
            ownerUserID: "alice",
            runID: "task-run-1",
            conversationID: "conversation-1",
            projectContext: .init(
                projectID: "project-1",
                projectName: "Test",
                projectRoot: project.path
            ),
            executionRootURL: project,
            workspaceScopeID: "workspace-1",
            builtinCapabilities: [.projectRead],
            pluginIDs: ["plugin-1"],
            pluginReleaseSnapshots: [
                "plugin-1": .init(
                    pluginKey: "plugin-1",
                    pluginID: "plugin-1",
                    releaseID: "release-1",
                    version: "1.0.0",
                    artifactSHA256: String(repeating: "a", count: 64)
                ),
            ]
        )
        let taskSearch = try await taskBroker.execute(.init(
            id: "task-search-1",
            name: "capability_search",
            arguments: #"{"query":"test"}"#
        ))
        XCTAssertTrue(taskSearch.content.contains("plugin_1"))
        XCTAssertFalse(taskSearch.content.contains("builtin_1"))
        let taskBuiltinSearch = try await taskBroker.execute(.init(
            id: "task-search-builtin",
            name: "capability_search",
            arguments: #"{"query":"文件"}"#
        ))
        XCTAssertFalse(taskBuiltinSearch.isError)
        XCTAssertTrue(taskBuiltinSearch.content.contains("builtin_1"))
        let taskBuiltinDescription = try await taskBroker.execute(.init(
            id: "task-describe-builtin",
            name: "capability_describe",
            arguments: #"{"plugin_option":"builtin_1"}"#
        ))
        let taskBuiltinDescriptionObject = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(taskBuiltinDescription.content.utf8)
        ).jsonObject
        let taskBuiltinSkillRefs = Set(
            taskBuiltinDescriptionObject?["tools"]?.jsonArray?.flatMap { tool in
                tool.jsonObject?["required_skill_refs"]?.jsonArray?.compactMap(\.jsonString) ?? []
            } ?? []
        )
        XCTAssertTrue(taskBuiltinSkillRefs.contains("product-skill:chatos-project-files"))
        XCTAssertTrue(taskBuiltinSkillRefs.contains("product-skill:chatos-project-read"))
        for skillRef in taskBuiltinSkillRefs.sorted() {
            let activation = try await taskBroker.execute(.init(
                id: "task-activate-\(skillRef)",
                name: "agent_skill_activate",
                arguments: """
                {"skill_ref":"\(skillRef)"}
                """
            ))
            XCTAssertFalse(activation.isError)
        }
        let translatedTaskSearch = try await taskBroker.execute(.init(
            id: "task-search-translated",
            name: "capability_search",
            arguments: #"{"query":"图表 架构图 导出"}"#
        ))
        XCTAssertFalse(translatedTaskSearch.isError)
        XCTAssertTrue(translatedTaskSearch.content.contains("plugin_1"))
        XCTAssertTrue(translatedTaskSearch.content.contains("test-agent-plugin"))
        XCTAssertFalse(translatedTaskSearch.content.contains("plugin-1"))
        let taskDescription = try await taskBroker.execute(.init(
            id: "task-describe-1",
            name: "capability_describe",
            arguments: #"{"plugin_option":"plugin_1","tool_option":"tool_1"}"#
        ))
        let taskDescriptionObject = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(taskDescription.content.utf8)
        ).jsonObject
        let taskTool = try XCTUnwrap(
            taskDescriptionObject?["tools"]?.jsonArray?.first?.jsonObject
        )
        let taskToolOption = try XCTUnwrap(taskTool["tool_option"]?.jsonString)
        let taskActivation = try await taskBroker.execute(.init(
            id: "task-activate-1",
            name: "agent_skill_activate",
            arguments: #"{"skill_ref":"plugin-skill:plugin_1:test-agent-plugin"}"#
        ))
        XCTAssertFalse(taskActivation.isError)
        let activationObject = try JSONDecoder().decode(
            NativeJSONValue.self, from: Data(taskActivation.content.utf8)
        ).jsonObject
        XCTAssertEqual(activationObject?["instructions"]?.jsonString, skillMarkdown.trimmingCharacters(in: .whitespacesAndNewlines))
        XCTAssertNil(activationObject?["host_skill_protocol"])
        let taskInvocation = try await taskBroker.execute(.init(
            id: "task-invoke-1",
            name: "capability_invoke",
            arguments: """
            {"plugin_option":"plugin_1","tool_option":"\(taskToolOption)","arguments":{"value":"task"}}
            """
        ))
        XCTAssertFalse(taskInvocation.isError)
        XCTAssertTrue(taskInvocation.content.contains("local-plugin-ok"))

        let contactRoot = root.appendingPathComponent("contact-workspace", isDirectory: true)
        try FileManager.default.createDirectory(
            at: contactRoot,
            withIntermediateDirectories: true
        )
        let contactBroker = try await service.makeTaskExecutionCapabilityToolProvider(
            ownerUserID: "alice",
            runID: "contact-task-run-1",
            conversationID: "contact-conversation-1",
            projectContext: .device,
            executionRootURL: contactRoot,
            workspaceScopeID: "contact:contact-conversation-1",
            pluginIDs: ["plugin-1"]
        )
        let contactSearch = try await contactBroker.execute(.init(
            id: "contact-task-search-1",
            name: "capability_search",
            arguments: #"{"query":"test"}"#
        ))
        XCTAssertFalse(contactSearch.isError)
        XCTAssertTrue(contactSearch.content.contains("plugin_1"))
        XCTAssertFalse(contactSearch.content.contains("builtin_1"))

        let platformExecutor = NativeLocalAgentPluginToolExecutor(
            connector: service,
            idleExpiration: .milliseconds(50)
        )
        let platformContext = NativeLocalAgentTaskExecutionContext(
            conversationID: "conversation-2",
            projectID: "project-1",
            applicationContext: .init(
                projectID: "project-1",
                projectName: "Test",
                projectRoot: project.path
            ),
            resolvedPath: try await service.resolveProjectPath(project.path),
            executionRootURL: project,
            workspaceScopeID: "workspace-1",
            toolAuthorization: .init(
                requiresExecution: true,
                enabledBuiltinKinds: [],
                pluginKeys: ["plugin-1"],
                isLegacyUnrestricted: true
            )
        )
        let platformSearch = try await platformExecutor.execute(
            ownerUserID: "alice",
            invocation: platformInvocation(
                runID: "task-run-2",
                name: "capability_search",
                arguments: .object(["query": .string("test")])
            ),
            context: platformContext,
            arguments: ["query": .string("test")]
        )
        XCTAssertEqual(platformSearch.jsonObject?["is_error"]?.jsonBool, false)
        XCTAssertTrue(
            platformSearch.jsonObject?["content"]?.jsonObject?["matches"]?
                .jsonArray?.first?.jsonObject?["plugin_option"]?.jsonString == "plugin_1"
        )
        let platformDescribe = try await platformExecutor.execute(
            ownerUserID: "alice",
            invocation: platformInvocation(
                runID: "task-run-2",
                name: "capability_describe",
                arguments: .object(["plugin_option": .string("plugin_1"), "tool_option": .string("tool_1")])
            ),
            context: platformContext,
            arguments: ["plugin_option": .string("plugin_1"), "tool_option": .string("tool_1")]
        )
        XCTAssertEqual(platformDescribe.jsonObject?["is_error"]?.jsonBool, false)
        let platformActivation = try await platformExecutor.execute(
            ownerUserID: "alice",
            invocation: platformInvocation(
                runID: "task-run-2",
                name: "agent_skill_activate",
                arguments: .object([
                    "skill_ref": .string("plugin-skill:plugin_1:test-agent-plugin"),
                ])
            ),
            context: platformContext,
            arguments: [
                "skill_ref": .string("plugin-skill:plugin_1:test-agent-plugin"),
            ]
        )
        XCTAssertEqual(platformActivation.jsonObject?["is_error"]?.jsonBool, false)
        let slowInvocation = try await platformExecutor.execute(
            ownerUserID: "alice",
            invocation: platformInvocation(
                runID: "task-run-2",
                name: "capability_invoke",
                arguments: .object([
                    "plugin_option": .string("plugin_1"),
                    "tool_option": .string("tool_1"),
                    "arguments": .object(["value": .string("slow")]),
                ])
            ),
            context: platformContext,
            arguments: [
                "plugin_option": .string("plugin_1"),
                "tool_option": .string("tool_1"),
                "arguments": .object(["value": .string("slow")]),
            ]
        )
        XCTAssertEqual(slowInvocation.jsonObject?["is_error"]?.jsonBool, false)
        XCTAssertTrue(
            slowInvocation.jsonObject?["content"]?.canonicalJSONString
                .contains("local-plugin-ok") == true,
            "An active plugin call must outlive the idle-session timeout"
        )
        await platformExecutor.reset()

        let contactContext = NativeLocalAgentTaskExecutionContext(
            conversationID: "contact-conversation-1",
            projectID: nil,
            applicationContext: .device,
            resolvedPath: nil,
            executionRootURL: contactRoot,
            workspaceScopeID: "contact:contact-conversation-1",
            toolAuthorization: .init(
                requiresExecution: true,
                enabledBuiltinKinds: [],
                pluginKeys: ["plugin-1"],
                isLegacyUnrestricted: false
            )
        )
        XCTAssertThrowsError(try contactContext.requireProject()) { error in
            XCTAssertEqual(
                error as? NativeLocalAgentPlatformToolError,
                .projectUnavailable
            )
        }
    }

    private func platformInvocation(
        runID: String,
        name: String,
        arguments: LocalAgentJSONValue
    ) -> LocalAgentToolInvocationRecord {
        .init(
            invocationID: "invocation-\(name)",
            runID: runID,
            batchID: "batch-1",
            callID: "call-\(name)",
            toolName: name,
            executionRoute: "platform",
            arguments: arguments,
            sideEffecting: name == "capability_invoke",
            requiresApproval: false,
            approvalStatus: "not_required",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "running",
            result: nil,
            error: nil,
            version: 1,
            claimToken: "claim-1",
            claimUntilUnixMs: nil,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }
}

private actor FrozenTaskSkillHostStub: LocalAgentHostClientServicing {
    let bundle: LocalAgentSkillBundleSnapshot
    let refs: Set<String>
    private var types: [String] = []

    init(bundle: LocalAgentSkillBundleSnapshot, refs: Set<String>) {
        self.bundle = bundle
        self.refs = refs
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        let object = try JSONDecoder().decode(NativeJSONValue.self, from: command).jsonObject
        guard object?["owner_user_id"]?.jsonString == "alice",
              object?["run_id"]?.jsonString == "frozen-run",
              let type = object?["type"]?.jsonString else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        types.append(type)
        let result: NativeJSONValue
        switch type {
        case "get_run_skill_bundle":
            result = .object([
                "type": .string("run_skill_bundle"), "run_id": .string("frozen-run"),
                "bundle": try JSONDecoder().decode(NativeJSONValue.self, from: JSONEncoder().encode(bundle)),
            ])
        case "get_run_skill_activations":
            result = .object([
                "type": .string("run_skill_activations"), "run_id": .string("frozen-run"),
                "skill_refs": .array(refs.sorted().map(NativeJSONValue.string)),
            ])
        default: throw NativeLocalAgentHostError.invalidResponse
        }
        return try JSONEncoder().encode(result)
    }

    func queryTypes() -> [String] { types }
}

private struct AgentPluginTestTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
