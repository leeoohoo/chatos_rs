import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeAgentPluginToolProviderTests: XCTestCase {
    func testRequirementSurveyUsesSharedProgressiveSkillTools() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("requirement-survey-skill-\(UUID().uuidString)")
        let databaseURL = root.appendingPathComponent("chat.db")
        defer { try? FileManager.default.removeItem(at: root) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: databaseURL)
        let tools = NativeMCPRequirementSurveyTools(
            store: store,
            ownerUserID: "alice",
            projectID: "project-1",
            creatorAgentID: "agent-1",
            sourceDeliveryID: "delivery-1",
            now: { 1 }
        )

        let result = try await tools.call(
            name: "skill_activate",
            arguments: [
                "skill_ref": .string(
                    LocalAgentProgressiveSkillCatalog.requirementSurveyResolveRef
                ),
            ]
        )
        let object = try XCTUnwrap(result.jsonObject)
        XCTAssertEqual(object["name"]?.jsonString, "requirement-survey-resolve")
        XCTAssertEqual(object["instructions_sha256"]?.jsonString?.count, 64)
        XCTAssertTrue(object["instructions"]?.jsonString?.contains(
            "requirement_survey_resolve"
        ) == true)
        XCTAssertEqual(
            object["resources"]?.jsonArray?.first?.jsonObject?["relative_path"]?.jsonString,
            "references/example.md"
        )
        XCTAssertEqual(
            object["resources"]?.jsonArray?.first?.jsonObject?["sha256"]?.jsonString?.count,
            64
        )

        let resource = try await tools.call(
            name: "skill_read_resource",
            arguments: [
                "skill_ref": .string(
                    LocalAgentProgressiveSkillCatalog.requirementSurveyResolveRef
                ),
                "relative_path": .string("references/example.md"),
            ]
        )
        XCTAssertTrue(resource.jsonObject?["content"]?.jsonString?.contains(
            "execution_steps"
        ) == true)
    }

    func testTodoAuthorizationCatalogUsesExecutorRuntimeToolDefinitions() {
        let cases: [(LocalAgentTodoBuiltinCapability, [NativeJSONValue])] = [
            (.projectRead, NativeMCPCodeReadTools.toolDefinitions),
            (.projectWrite, NativeMCPCodeWriteStore.toolDefinitions),
            (.terminal, NativeMCPTerminalStore.toolDefinitions),
            (.requirementSurveyRead, NativeMCPRequirementSurveyTools.readToolDefinitions),
            (.requirementSurveyWrite, NativeMCPRequirementSurveyTools.writeToolDefinitions),
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

    func testRequirementSurveyCreateSchemaSupportsRankingQuestions() throws {
        let create = try XCTUnwrap(
            NativeMCPRequirementSurveyTools.writeToolDefinitions.first {
                $0.jsonObject?["name"]?.jsonString == "requirement_survey_create"
            }
        )
        let kinds = create.jsonObject?["inputSchema"]?.jsonObject?["properties"]?
            .jsonObject?["questions"]?.jsonObject?["items"]?.jsonObject?["properties"]?
            .jsonObject?["kind"]?.jsonObject?["enum"]?.jsonArray?
            .compactMap(\.jsonString)

        XCTAssertEqual(kinds, ["single_choice", "multiple_choice", "ranking"])
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

    func testInstalledPluginRunsDirectlyWithoutRelayRequest() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-plugin-provider-\(UUID().uuidString)")
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let bin = installation.appendingPathComponent("bin", isDirectory: true)
        let project = root.appendingPathComponent("project", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
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
              printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"local_echo","description":"Echo locally","inputSchema":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false},"annotations":{"readOnlyHint":true}}]}}'
              ;;
            *'"method":"tools/call"'*)
              printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"local-plugin-ok project-1"}]}}'
              ;;
          esac
        done
        """#
        try Data(script.utf8).write(to: executable)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: executable.path
        )
        let manifest = #"""
        {
          "schemaVersion": 3,
          "name": "test-agent-plugin",
          "version": "1.0.0",
          "description": "test",
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
                installedAt: "2026-09-16T00:00:00Z"
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
        let providers = try await service.makeAgentPluginToolProviders(
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

        let outcome = try await providers[0].execute(.init(
            id: "call-1",
            name: "local_echo",
            arguments: #"{"value":"hello"}"#
        ))
        XCTAssertFalse(outcome.isError)
        XCTAssertTrue(outcome.content.contains("local-plugin-ok"))

        let broker = try await service.makeAgentCapabilityToolProvider(
            ownerUserID: "alice",
            runContext: runContext,
            projectContext: .init(
                projectID: "project-1",
                projectName: "Test",
                projectRoot: project.path
            ),
            executionPlan: .init(
                builtinCapabilities: [.projectRead, .projectWrite, .terminal],
                plugins: [.init(pluginID: "plugin-1", displayName: "test-agent-plugin")]
            )
        )
        let brokerDefinitions = try await broker.definitions()
        XCTAssertEqual(
            Set(brokerDefinitions.map(\.name)),
            ["capability_search", "capability_describe", "capability_invoke"]
        )

        let search = try await broker.execute(.init(
            id: "search-1",
            name: "capability_search",
            arguments: #"{"query":"test"}"#
        ))
        XCTAssertTrue(search.content.contains("plugin_1"))
        XCTAssertTrue(search.content.contains("test-agent-plugin"))
        XCTAssertFalse(search.content.contains("plugin-1"))

        let description = try await broker.execute(.init(
            id: "describe-1",
            name: "capability_describe",
            arguments: #"{"plugin_option":"plugin_1"}"#
        ))
        XCTAssertTrue(description.content.contains("tool_1"))
        XCTAssertTrue(description.content.contains("local_echo"))
        XCTAssertFalse(description.content.contains("project-1"))

        let invoked = try await broker.execute(.init(
            id: "invoke-1",
            name: "capability_invoke",
            arguments: #"{"plugin_option":"plugin_1","tool_option":"tool_1","arguments":{"value":"hello"}}"#
        ))
        XCTAssertFalse(invoked.isError)
        XCTAssertTrue(invoked.content.contains("local-plugin-ok"))
        XCTAssertFalse(invoked.content.contains("project-1"))
        XCTAssertTrue(invoked.content.contains("[internal-project]"))
    }
}

private struct AgentPluginTestTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
