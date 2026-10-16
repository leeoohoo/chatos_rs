@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

/// Opt-in fixture export: uses the production loader/bootstrap, never a handwritten eval Prompt.
final class NativeTaskOrchestrationEvalExportTests: XCTestCase {
    func testExportFrozenEvaluationCatalog() throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_ORCHESTRATION_EXPORT_PATH"] else {
            throw XCTSkip("set CHATOS_ORCHESTRATION_EXPORT_PATH to export the evaluation fixture")
        }
        guard path.hasPrefix("/"), !FileManager.default.fileExists(atPath: path) else {
            return XCTFail("export requires a new absolute output path")
        }
        let plugins: [NativeInstalledAgentPlugin] = [
            .init(pluginID: "eval-browser", pluginKey: NativeBrowserPluginIdentity.marketplaceKey,
                  componentKey: NativeBrowserPluginIdentity.componentKey, displayName: "Browser CDP", description: "Inspect public websites and verify browser UI."),
            .init(pluginID: "eval-computer", pluginKey: "open-computer-use@chatos-marketplace",
                  componentKey: "computer-use-mcp", displayName: "Computer Use", description: "Inspect and control native desktop apps."),
            .init(pluginID: "eval-document", pluginKey: NativeDocumentPluginIdentity.marketplaceKey,
                  componentKey: NativeDocumentPluginIdentity.componentKey, displayName: "Documents", description: "Create, edit, render and verify Word, PDF, spreadsheets and presentations."),
            .init(pluginID: "eval-design", pluginKey: "design@eval",
                  componentKey: "design-mcp", displayName: "Design", description: "Create editable design assets and export visual previews."),
            .init(pluginID: "eval-media", pluginKey: "media@eval",
                  componentKey: "media-mcp", displayName: "Media", description: "Edit video and audio, generate subtitles and export verified media."),
        ]
        let external = NativeLocalAgentExternalMCPConfig(
            resourceID: "eval-research", serverName: "Research",
            url: URL(string: "https://example.invalid/evaluation-only")!, headers: [:],
            tools: [.init(publicName: "external_mcp__eval_research", upstreamName: "research",
                          description: "Read public research records.", inputSchema: .object(["type": .string("object")]))]
        )
        let main = try NativeLocalConnectorService.mainChatTaskSkillSnapshot()
        let task = try NativeLocalConnectorService.taskExecutionSkillSnapshot(externalMCPConfigs: [external])
        let mainTools = NativeLocalAgentPlatformToolCatalog.capabilityTools(
            pluginChoices: plugins,
            builtinChoices: [
                .init(value: "CodeMaintainerRead", title: "Read bound project files"),
                .init(value: "CodeMaintainerWrite", title: "Write owned project paths"),
                .init(value: "TerminalController", title: "Run project commands and tests"),
            ],
            externalChoices: [.init(value: external.resourceID, title: "Read public research records")],
            taskModelChoices: [.init(value: "eval-model", title: "Evaluation model")]
        )
        let policy: LocalAgentJSONValue = .object([
            "allowed_model_config_ids": .array([.string("eval-model")]),
            "enabled_builtin_kinds": .array([]), "external_mcp_config_ids": .array([]),
            "plugin_keys": .array([]), "max_iterations": .number(32),
            "plugin_release_snapshots": .array(plugins.map {
                .object(["plugin_key": .string($0.pluginKey), "plugin_id": .string($0.id),
                         "release_id": .string("eval-release"), "version": .string("1.0.0"),
                         "artifact_sha256": .string(String(repeating: "0", count: 64))])
            }),
        ])
        let checksum = try NativeLocalConnectorService.combinedSkillChecksum(main: main, task: task)
        let taskTools = NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools(externalMCPConfigs: [external])
        let revision = try NativeLocalConnectorService.capabilityRevision(
            baseRevision: "eval-policy-v1", skillBundleChecksum: checksum,
            mainTools: mainTools, taskTools: taskTools, taskPolicy: policy
        )
        func snapshot(_ profile: String, instructions: String, items: [LocalAgentJSONValue] = [],
                      tools: [LocalAgentJSONValue] = []) -> LocalAgentCapabilityPolicySnapshot {
            .init(ownerUserID: "eval-user", profileKey: profile, capabilityPolicyRevision: revision,
                  instructions: instructions, prefixedInputItems: items, tools: tools)
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let policyJSON = try XCTUnwrap(String(data: encoder.encode(policy), encoding: .utf8))
        let snapshots = [
            snapshot("skill_bundle_internal", instructions: try NativeLocalConnectorService.encodedSkillBundle(main.bundle)),
            snapshot("task_skill_bundle_internal", instructions: try NativeLocalConnectorService.encodedSkillBundle(task.bundle)),
            snapshot("task_policy_internal", instructions: policyJSON),
            snapshot("task_execution", instructions: task.instructions, items: [task.prefixedInputItem], tools: taskTools),
            snapshot("main_chat", instructions: main.instructions, items: [main.prefixedInputItem], tools: mainTools),
        ]
        XCTAssertEqual(Set(mainTools.compactMap { tool -> String? in
            guard case let .object(object) = tool, case let .string(name)? = object["name"] else { return nil }
            return name
        }), Set(NativeLocalAgentPlatformToolCatalog.mainChatTaskToolNames))
        try encoder.encode(snapshots).write(to: URL(fileURLWithPath: path), options: .withoutOverwriting)
    }
}
