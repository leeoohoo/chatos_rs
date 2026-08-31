import Foundation

public enum LocalConnectorApprovalMode: String, Codable, CaseIterable, Sendable {
    case requestApproval = "request_approval"
    case autoApproval = "auto_approval"
    case fullControl = "full_control"
}

public struct LocalConnectorPendingApproval: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var requestID: String
    public var command: String
    public var cwd: String
    public var source: String
    public var risk: String
    public var reason: String?
    public var createdAt: String
    public var availableDecisions: [String]

    public init(
        id: String,
        requestID: String,
        command: String,
        cwd: String,
        source: String,
        risk: String,
        reason: String?,
        createdAt: String,
        availableDecisions: [String]
    ) {
        self.id = id
        self.requestID = requestID
        self.command = command
        self.cwd = cwd
        self.source = source
        self.risk = risk
        self.reason = reason
        self.createdAt = createdAt
        self.availableDecisions = availableDecisions
    }
}

public struct LocalConnectorApprovalHistoryEntry: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var command: String
    public var cwd: String
    public var source: String
    public var mode: LocalConnectorApprovalMode
    public var decision: String
    public var risk: String
    public var reason: String?
    public var createdAt: String

    public init(
        id: String,
        command: String,
        cwd: String,
        source: String,
        mode: LocalConnectorApprovalMode,
        decision: String,
        risk: String,
        reason: String?,
        createdAt: String
    ) {
        self.id = id
        self.command = command
        self.cwd = cwd
        self.source = source
        self.mode = mode
        self.decision = decision
        self.risk = risk
        self.reason = reason
        self.createdAt = createdAt
    }
}

public struct LocalConnectorApprovalSettings: Codable, Sendable, Equatable {
    public var defaultMode: LocalConnectorApprovalMode
    public var history: [LocalConnectorApprovalHistoryEntry]

    public init(defaultMode: LocalConnectorApprovalMode, history: [LocalConnectorApprovalHistoryEntry]) {
        self.defaultMode = defaultMode
        self.history = history
    }
}

public struct LocalConnectorModelConfig: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var sourceProviderID: String?
    public var name: String
    public var provider: String
    public var promptVendor: String?
    public var baseURL: String
    public var modelName: String
    public var taskUsageScenario: String?
    public var taskThinkingLevel: String?
    public var temperature: Double?
    public var maxOutputTokens: Int?
    public var enabled: Bool
    public var taskEnabled: Bool
    public var hasAPIKey: Bool
    public var supportsImages: Bool
    public var supportsReasoning: Bool
    public var supportsResponses: Bool

    public init(
        id: String,
        sourceProviderID: String? = nil,
        name: String,
        provider: String,
        promptVendor: String? = nil,
        baseURL: String = "",
        modelName: String,
        taskUsageScenario: String? = nil,
        taskThinkingLevel: String? = nil,
        temperature: Double? = nil,
        maxOutputTokens: Int? = nil,
        enabled: Bool,
        taskEnabled: Bool = true,
        hasAPIKey: Bool,
        supportsImages: Bool,
        supportsReasoning: Bool,
        supportsResponses: Bool = false
    ) {
        self.id = id
        self.sourceProviderID = sourceProviderID
        self.name = name
        self.provider = provider
        self.promptVendor = promptVendor
        self.baseURL = baseURL
        self.modelName = modelName
        self.taskUsageScenario = taskUsageScenario
        self.taskThinkingLevel = taskThinkingLevel
        self.temperature = temperature
        self.maxOutputTokens = maxOutputTokens
        self.enabled = enabled
        self.taskEnabled = taskEnabled
        self.hasAPIKey = hasAPIKey
        self.supportsImages = supportsImages
        self.supportsReasoning = supportsReasoning
        self.supportsResponses = supportsResponses
    }
}

public struct LocalConnectorModelSettings: Codable, Sendable, Equatable {
    public var modelRequestMaxRetries: Int?
    public var memorySummaryModelConfigID: String?
    public var memorySummaryThinkingLevel: String?
    public var projectManagementAgentModelConfigID: String?
    public var projectManagementAgentThinkingLevel: String?
    public var commandApprovalModelConfigID: String?
    public var commandApprovalThinkingLevel: String?

    public init(
        modelRequestMaxRetries: Int?,
        memorySummaryModelConfigID: String? = nil,
        memorySummaryThinkingLevel: String? = nil,
        projectManagementAgentModelConfigID: String? = nil,
        projectManagementAgentThinkingLevel: String? = nil,
        commandApprovalModelConfigID: String?,
        commandApprovalThinkingLevel: String?
    ) {
        self.modelRequestMaxRetries = modelRequestMaxRetries
        self.memorySummaryModelConfigID = memorySummaryModelConfigID
        self.memorySummaryThinkingLevel = memorySummaryThinkingLevel
        self.projectManagementAgentModelConfigID = projectManagementAgentModelConfigID
        self.projectManagementAgentThinkingLevel = projectManagementAgentThinkingLevel
        self.commandApprovalModelConfigID = commandApprovalModelConfigID
        self.commandApprovalThinkingLevel = commandApprovalThinkingLevel
    }
}

public struct LocalConnectorModelCatalog: Codable, Sendable, Equatable {
    public var items: [LocalConnectorModelConfig]
    public var settings: LocalConnectorModelSettings

    public init(items: [LocalConnectorModelConfig], settings: LocalConnectorModelSettings) {
        self.items = items
        self.settings = settings
    }
}
