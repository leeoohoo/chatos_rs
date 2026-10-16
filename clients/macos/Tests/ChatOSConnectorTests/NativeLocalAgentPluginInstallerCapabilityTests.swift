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

    private func toolNames(_ tools: [LocalAgentJSONValue]) -> Set<String> {
        Set(tools.compactMap(Self.toolName))
    }

    private static func toolName(_ value: LocalAgentJSONValue) -> String? {
        guard case let .object(tool) = value,
              case let .string(name)? = tool["name"] else { return nil }
        return name
    }
}
