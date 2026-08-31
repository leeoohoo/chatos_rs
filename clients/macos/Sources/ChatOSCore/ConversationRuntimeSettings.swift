public struct ConversationRuntimeSettings: Sendable, Equatable {
    public var selectedModelID: String?
    public var selectedModelName: String?
    public var selectedThinkingLevel: String?
    public var reasoningEnabled: Bool
    public var planModeEnabled: Bool

    public init(
        selectedModelID: String? = nil,
        selectedModelName: String? = nil,
        selectedThinkingLevel: String? = nil,
        reasoningEnabled: Bool = false,
        planModeEnabled: Bool = false
    ) {
        self.selectedModelID = selectedModelID
        self.selectedModelName = selectedModelName
        self.selectedThinkingLevel = selectedThinkingLevel
        self.reasoningEnabled = reasoningEnabled
        self.planModeEnabled = planModeEnabled
    }
}

public struct ConversationModelOption: Sendable, Equatable, Identifiable {
    public var id: String
    public var displayName: String
    public var modelName: String
    public var thinkingLevel: String?

    public init(
        id: String,
        displayName: String,
        modelName: String,
        thinkingLevel: String? = nil
    ) {
        self.id = id
        self.displayName = displayName
        self.modelName = modelName
        self.thinkingLevel = thinkingLevel
    }
}

public protocol ConversationRuntimeSettingsServicing: Sendable {
    func fetchSettings(sessionID: String) async throws -> ConversationRuntimeSettings
    func fetchAvailableModels() async throws -> [ConversationModelOption]
    func updateModel(sessionID: String, modelID: String) async throws -> ConversationRuntimeSettings
    func updatePlanMode(sessionID: String, enabled: Bool) async throws -> ConversationRuntimeSettings
    func updateReasoning(sessionID: String, enabled: Bool) async throws -> ConversationRuntimeSettings
}
