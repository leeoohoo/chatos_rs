public struct ConversationRuntimeSettings: Sendable, Equatable {
    public var selectedModelID: String?
    public var selectedModelName: String?
    public var selectedThinkingLevel: String?
    public var remoteConnectionID: String?
    public var reasoningEnabled: Bool

    public init(
        selectedModelID: String? = nil,
        selectedModelName: String? = nil,
        selectedThinkingLevel: String? = nil,
        remoteConnectionID: String? = nil,
        reasoningEnabled: Bool = false
    ) {
        self.selectedModelID = selectedModelID
        self.selectedModelName = selectedModelName
        self.selectedThinkingLevel = selectedThinkingLevel
        self.remoteConnectionID = remoteConnectionID
        self.reasoningEnabled = reasoningEnabled
    }
}

public struct ConversationModelOption: Sendable, Equatable, Identifiable {
    public var id: String
    public var displayName: String
    public var modelName: String
    public var provider: String
    public var thinkingLevel: String?
    public var supportsReasoning: Bool
    public var thinkingLevels: [String]

    public init(
        id: String,
        displayName: String,
        modelName: String,
        provider: String = "gpt",
        thinkingLevel: String? = nil,
        supportsReasoning: Bool = false,
        thinkingLevels: [String] = []
    ) {
        self.id = id
        self.displayName = displayName
        self.modelName = modelName
        self.provider = provider
        self.thinkingLevel = thinkingLevel
        self.supportsReasoning = supportsReasoning
        self.thinkingLevels = thinkingLevels
    }
}

public protocol ConversationRuntimeSettingsServicing: Sendable {
    func fetchSettings(sessionID: String) async throws -> ConversationRuntimeSettings
    func fetchAvailableModels() async throws -> [ConversationModelOption]
    func updateModel(sessionID: String, modelID: String) async throws -> ConversationRuntimeSettings
    func updateRemoteConnection(
        sessionID: String,
        connectionID: String?
    ) async throws -> ConversationRuntimeSettings
    func updateReasoning(sessionID: String, enabled: Bool) async throws -> ConversationRuntimeSettings
    func updateReasoningLevel(
        sessionID: String,
        level: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings
}
