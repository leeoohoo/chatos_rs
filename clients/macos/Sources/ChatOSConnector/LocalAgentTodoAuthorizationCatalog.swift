import ChatOSCore
import Foundation

/// Human-facing projection of the exact built-in tools guarded by a Todo capability.
/// Tool names come from the same runtime definitions used to build the executor registry.
public struct LocalAgentTodoBuiltinAuthorizationDescriptor: Sendable, Equatable, Identifiable {
    public var id: String { capability.rawValue }
    public let capability: LocalAgentTodoBuiltinCapability
    public let displayName: String
    public let detail: String
    public let toolNames: [String]

    public init(
        capability: LocalAgentTodoBuiltinCapability,
        displayName: String,
        detail: String,
        toolNames: [String]
    ) {
        self.capability = capability
        self.displayName = displayName
        self.detail = detail
        self.toolNames = toolNames
    }
}

public enum LocalAgentTodoAuthorizationCatalog {
    public static func descriptor(
        for capability: LocalAgentTodoBuiltinCapability
    ) -> LocalAgentTodoBuiltinAuthorizationDescriptor {
        let toolNames: [String]
        let displayName: String
        let detail: String
        switch capability {
        case .projectRead:
            toolNames = NativeMCPCodeReadTools.toolDefinitions.compactMap {
                $0.jsonObject?["name"]?.jsonString
            }
            displayName = "项目读取"
            detail = "仅在当前任务绑定的项目目录内读取、列目录和搜索。"
        case .projectWrite:
            toolNames = NativeMCPCodeWriteStore.toolDefinitions.compactMap {
                $0.jsonObject?["name"]?.jsonString
            }
            displayName = "项目写入"
            detail = "通过暂存与提交会话修改当前项目；提交仍受运行时审批约束。"
        case .terminal:
            toolNames = NativeMCPTerminalStore.toolDefinitions.compactMap {
                $0.jsonObject?["name"]?.jsonString
            }
            displayName = "项目终端"
            detail = "在当前任务绑定的项目目录内执行和管理命令进程。"
        case .requirementSurveyRead:
            toolNames = []
            displayName = "需求调研读取"
            detail = "读取当前团队 Todo 绑定的调研答案与方案上下文。"
        case .requirementSurveyWrite:
            toolNames = ["requirement_survey_create", "requirement_plan_save"]
            displayName = "需求调研"
            detail = "在正式需求调研中心创建问卷，并在提交后保存版本化方案。"
        }
        return .init(
            capability: capability,
            displayName: displayName,
            detail: detail,
            toolNames: toolNames
        )
    }
}
