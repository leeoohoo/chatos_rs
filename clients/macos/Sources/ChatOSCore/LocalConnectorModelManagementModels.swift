import Foundation

public struct LocalConnectorModelProvider: Codable, Identifiable, Sendable, Equatable {
    public var id: String
    public var name: String
    public var provider: String
    public var promptVendor: String
    public var baseURL: String
    public var hasAPIKey: Bool
    public var enabled: Bool
    public var supportsImages: Bool
    public var supportsReasoning: Bool
    public var supportsResponses: Bool
    public var lastSyncStatus: String?
    public var lastSyncError: String?
    public var importedModelCount: Int

    public init(
        id: String,
        name: String,
        provider: String,
        promptVendor: String,
        baseURL: String,
        hasAPIKey: Bool,
        enabled: Bool,
        supportsImages: Bool,
        supportsReasoning: Bool,
        supportsResponses: Bool,
        lastSyncStatus: String?,
        lastSyncError: String?,
        importedModelCount: Int
    ) {
        self.id = id
        self.name = name
        self.provider = provider
        self.promptVendor = promptVendor
        self.baseURL = baseURL
        self.hasAPIKey = hasAPIKey
        self.enabled = enabled
        self.supportsImages = supportsImages
        self.supportsReasoning = supportsReasoning
        self.supportsResponses = supportsResponses
        self.lastSyncStatus = lastSyncStatus
        self.lastSyncError = lastSyncError
        self.importedModelCount = importedModelCount
    }
}

public struct LocalConnectorModelProviderDraft: Codable, Sendable, Equatable {
    public var name: String
    public var provider: String
    public var promptVendor: String
    public var baseURL: String
    public var apiKey: String
    public var clearAPIKey: Bool
    public var enabled: Bool
    public var supportsImages: Bool
    public var supportsReasoning: Bool
    public var supportsResponses: Bool

    public init(
        name: String,
        provider: String,
        promptVendor: String,
        baseURL: String,
        apiKey: String,
        clearAPIKey: Bool = false,
        enabled: Bool,
        supportsImages: Bool,
        supportsReasoning: Bool,
        supportsResponses: Bool
    ) {
        self.name = name
        self.provider = provider
        self.promptVendor = promptVendor
        self.baseURL = baseURL
        self.apiKey = apiKey
        self.clearAPIKey = clearAPIKey
        self.enabled = enabled
        self.supportsImages = supportsImages
        self.supportsReasoning = supportsReasoning
        self.supportsResponses = supportsResponses
    }
}

public struct LocalConnectorModelConfigUpdate: Codable, Sendable, Equatable {
    public var taskUsageScenario: String?
    public var taskThinkingLevel: String?
    public var temperature: Double?
    public var maxOutputTokens: Int?
    public var enabled: Bool

    public init(
        taskUsageScenario: String?,
        taskThinkingLevel: String?,
        temperature: Double?,
        maxOutputTokens: Int?,
        enabled: Bool
    ) {
        self.taskUsageScenario = taskUsageScenario
        self.taskThinkingLevel = taskThinkingLevel
        self.temperature = temperature
        self.maxOutputTokens = maxOutputTokens
        self.enabled = enabled
    }
}
