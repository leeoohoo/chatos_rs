import ChatOSAgentRuntime
import ChatOSCore
import Foundation
@testable import ChatOSConnector
import XCTest

final class LocalAgentBuilderToolProviderTests: XCTestCase {
    func testBuilderCanOnlyProduceDraftFromFrozenAllowlists() async throws {
        let provider = try LocalAgentBuilderToolProvider(
            project: .init(
                projectID: "project-1",
                projectName: "客户端",
                projectDescription: "macOS 客户端",
                roomName: "项目群聊",
                roomGoal: "完成本地多 Agent 协作",
                members: [.init(name: "架构师", role: "架构", responsibility: "审查设计")]
            ),
            models: [
                .init(id: "model-1", name: "主模型", provider: "openai", modelName: "gpt-test"),
            ],
            plugins: [
                .init(id: "plugin.git", name: "Git", description: "本地 Git 工具"),
            ]
        )

        let definitions = try await provider.definitions()
        XCTAssertEqual(
            Set(definitions.map(\.name)),
            ["project_inspect", "model_list", "plugin_list_installed", "agent_draft"]
        )
        XCTAssertEqual(
            definitions.first(where: { $0.name == "agent_draft" })?.effect,
            .terminal
        )
        let project = try await provider.execute(
            .init(id: "project", name: "project_inspect", arguments: "{}")
        )
        XCTAssertTrue(project.content.contains("客户端"))
        XCTAssertTrue(project.content.contains("架构师"))

        let rejected = try await provider.execute(
            .init(
                id: "invalid",
                name: "agent_draft",
                arguments: Self.arguments(pluginIDs: ["plugin.not-installed"])
            )
        )
        XCTAssertTrue(rejected.isError)
        let rejectedDraft = await provider.currentDraft()
        XCTAssertNil(rejectedDraft)

        let accepted = try await provider.execute(
            .init(
                id: "valid",
                name: "agent_draft",
                arguments: Self.arguments(pluginIDs: ["plugin.git"])
            )
        )
        XCTAssertFalse(accepted.isError)
        let draft = await provider.currentDraft()
        XCTAssertEqual(draft?.name, "客户端工程师")
        XCTAssertEqual(draft?.modelConfigID, "model-1")
        XCTAssertEqual(draft?.pluginIDs, ["plugin.git"])
    }

    private static func arguments(pluginIDs: [String]) throws -> String {
        let value: [String: Any] = [
            "name": "客户端工程师",
            "role": "客户端实现",
            "responsibility": "实现本地群聊界面",
            "rolePrompt": "只处理当前项目明确交给你的客户端任务。",
            "modelConfigID": "model-1",
            "pluginIDs": pluginIDs,
            "rationale": "项目缺少客户端实现成员",
        ]
        return String(
            decoding: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
            as: UTF8.self
        )
    }
}
