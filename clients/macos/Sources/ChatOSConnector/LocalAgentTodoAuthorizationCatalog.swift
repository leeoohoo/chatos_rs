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
        let definitions: [NativeJSONValue]
        let displayName: String
        let detail: String
        switch capability {
        case .projectRead:
            definitions = NativeMCPCodeReadTools.toolDefinitions
            displayName = "项目读取"
            detail = "仅在当前任务绑定的项目目录内读取、列目录和搜索。"
        case .projectWrite:
            definitions = NativeMCPCodeWriteStore.toolDefinitions
            displayName = "项目写入"
            detail = "通过暂存与提交会话修改当前项目；提交仍受运行时审批约束。"
        case .terminal:
            definitions = NativeMCPTerminalStore.toolDefinitions
            displayName = "项目终端"
            detail = "在当前任务绑定的项目目录内执行和管理命令进程。"
        case .requirementSurveyRead:
            definitions = NativeMCPRequirementSurveyTools.readToolDefinitions
            displayName = "需求调研读取"
            detail = "读取当前任务绑定项目的调研、Human 答案、方案、执行计划和项目任务状态。"
        case .requirementSurveyWrite:
            definitions = NativeMCPRequirementSurveyTools.writeToolDefinitions
            displayName = "需求调研创建与方案"
            detail = "创建调研单或写入正式方案；选择后程序会同时授权需求调研读取。"
        }
        return .init(
            capability: capability,
            displayName: displayName,
            detail: detail,
            toolNames: definitions.compactMap { $0.jsonObject?["name"]?.jsonString }
        )
    }
}
