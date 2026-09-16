import ChatOSAgentRuntime
import ChatOSCore
import Foundation
@testable import ChatOSConnector
import XCTest

final class LocalAgentBuilderToolProviderTests: XCTestCase {
    func testBuilderProducesAgentWithoutPreselectingPlugins() async throws {
        let provider = try LocalAgentBuilderToolProvider(
            project: .init(
                projectName: "客户端",
                projectDescription: "macOS 客户端",
                projectTypeKey: "desktop_application",
                roomName: "项目群聊",
                roomGoal: "完成本地多 Agent 协作",
                members: [.init(name: "架构师", role: "架构", responsibility: "审查设计")]
            ),
            models: [
                .init(id: "model-1", name: "主模型", provider: "openai", modelName: "gpt-test"),
            ],
            professions: LocalAgentSkillCatalog.professions
        )

        let definitions = try await provider.definitions()
        XCTAssertEqual(
            Set(definitions.map(\.name)),
            ["project_inspect", "model_list", "profession_list", "agent_draft"]
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
        XCTAssertFalse(project.content.contains("project-1"))

        let accepted = try await provider.execute(
            .init(
                id: "valid",
                name: "agent_draft",
                arguments: Self.arguments()
            )
        )
        XCTAssertFalse(accepted.isError)
        let draft = await provider.currentDraft()
        XCTAssertEqual(draft?.name, "客户端工程师")
        XCTAssertEqual(draft?.modelConfigID, "model-1")
        XCTAssertEqual(draft?.professionKey, "desktop_engineer")
        let draftSchema = String(
            decoding: try XCTUnwrap(definitions.first(where: { $0.name == "agent_draft" })).schema,
            as: UTF8.self
        )
        XCTAssertFalse(draftSchema.contains("plugin"))
    }

    private static func arguments() throws -> String {
        let value: [String: Any] = [
            "name": "客户端工程师",
            "role": "客户端实现",
            "responsibility": "实现本地群聊界面",
            "rolePrompt": "只处理当前项目明确交给你的客户端任务。",
            "modelConfigID": "model-1",
            "professionKey": "desktop_engineer",
            "rationale": "项目缺少客户端实现成员",
        ]
        return String(
            decoding: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
            as: UTF8.self
        )
    }
}
