@testable import ChatOSConnector
import ChatOSCore
import XCTest

final class NativeLocalAgentPluginInstallerCapabilityTests: XCTestCase {
    func testInstallerCapabilityContainsOnlyDedicatedInstallerTools() throws {
        let capability = try NativeLocalConnectorService.pluginInstallerCapability(
            ownerUserID: "user-1",
            baseRevision: "base-1"
        )
        XCTAssertEqual(capability.ownerUserID, "user-1")
        XCTAssertEqual(capability.profileKey, "main_chat")
        XCTAssertEqual(toolNames(capability.tools),
                       NativeLocalAgentPlatformToolCatalog.pluginInstallerToolNames)
        XCTAssertTrue(capability.instructions?.contains("项目聊天和其他联系人没有这些安装工具") == true)
        XCTAssertTrue(capability.instructions?.contains("不得再问“安装还是只查看”") == true)
        XCTAssertTrue(capability.instructions?.contains("REA 或 reverse-engineer-anything 明确对应 npm 包 rea-agents") == true)
        XCTAssertTrue(capability.instructions?.contains("不得再让用户在 rea-agents 与面向其他宿主的 REA 适配包之间选择") == true)
    }

    func testOrdinaryMainChatCatalogNeverPublishesInstallerTools() {
        let ordinary = Set(
            NativeLocalAgentPlatformToolCatalog.capabilityTools.compactMap(Self.toolName)
        )
        XCTAssertTrue(
            ordinary.isDisjoint(
                with: NativeLocalAgentPlatformToolCatalog.pluginInstallerToolNames
            )
        )
    }

    func testProjectTaskCatalogNeverPublishesInstallerTools() {
        let projectTaskTools = Set(
            NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools.compactMap(
                Self.toolName
            )
        )
        XCTAssertTrue(
            projectTaskTools.isDisjoint(
                with: NativeLocalAgentPlatformToolCatalog.pluginInstallerToolNames
            )
        )
    }

    func testOnlyDedicatedInstallerContactReceivesInstallerCapability() {
        XCTAssertTrue(
            NativeLocalAgentConversationService.usesPluginInstallerCapability(
                for: .init(
                    kind: .contact,
                    resourceID: WorkspaceSystemContact.pluginInstallerID
                )
            )
        )
        XCTAssertFalse(
            NativeLocalAgentConversationService.usesPluginInstallerCapability(
                for: .init(kind: .project, resourceID: "project-1")
            )
        )
        XCTAssertFalse(
            NativeLocalAgentConversationService.usesPluginInstallerCapability(
                for: .init(kind: .contact, resourceID: WorkspaceSystemContact.mainID)
            )
        )
        XCTAssertFalse(
            NativeLocalAgentConversationService.usesPluginInstallerCapability(for: nil)
        )
    }

    func testInstallerRevisionChangesWithBaseCapabilityRevision() throws {
        let first = try NativeLocalConnectorService.pluginInstallerCapability(
            ownerUserID: "user-1",
            baseRevision: "base-1"
        )
        let second = try NativeLocalConnectorService.pluginInstallerCapability(
            ownerUserID: "user-1",
            baseRevision: "base-2"
        )
        XCTAssertNotEqual(first.capabilityPolicyRevision, second.capabilityPolicyRevision)
    }

    func testInstallIntentRequiresAnExplicitNonNegatedRequest() {
        for message in [
            "帮我安装一下 REA",
            "直接添加这个 MCP",
            "请配置并使用这个插件",
            "Please install rea-agents for me.",
            "Set up this MCP plugin.",
        ] {
            XCTAssertTrue(
                NativeLocalAgentPluginInstallerToolExecutor.hasExplicitInstallIntent(message),
                message
            )
        }

        for message in [
            "比较一下 REA 和另一个插件",
            "怎么安装 REA？",
            "是否支持安装 Codex 插件？",
            "先不要安装，只查看工具列表",
            "How do I install this MCP?",
            "Do not install this package.",
        ] {
            XCTAssertFalse(
                NativeLocalAgentPluginInstallerToolExecutor.hasExplicitInstallIntent(message),
                message
            )
        }
    }

    private func toolNames(_ tools: [LocalAgentJSONValue]) -> Set<String> {
        Set(tools.compactMap(Self.toolName))
    }

    private static func toolName(_ value: LocalAgentJSONValue) -> String? {
        guard case let .object(tool) = value,
              case let .string(name)? = tool["name"] else { return nil }
        return name
    }
}
