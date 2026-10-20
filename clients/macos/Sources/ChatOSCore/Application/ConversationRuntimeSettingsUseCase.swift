public struct ConversationRuntimeSettingsSnapshot: Sendable, Equatable {
    public let settings: ConversationRuntimeSettings
    public let availableModels: [ConversationModelOption]

    public init(
        settings: ConversationRuntimeSettings,
        availableModels: [ConversationModelOption]
    ) {
        self.settings = settings
        self.availableModels = availableModels
    }
}

public struct ConversationRuntimeSettingsUseCase: Sendable {
    private let service: any ConversationRuntimeSettingsServicing

    public init(service: any ConversationRuntimeSettingsServicing) {
        self.service = service
    }

    public func load(sessionID: String) async throws -> ConversationRuntimeSettingsSnapshot {
        async let settings = service.fetchSettings(sessionID: sessionID)
        async let availableModels = service.fetchAvailableModels()
        return try await ConversationRuntimeSettingsSnapshot(
            settings: settings,
            availableModels: availableModels
        )
    }

    public func selectModel(
        sessionID: String,
        modelID: String
    ) async throws -> ConversationRuntimeSettings {
        try await service.updateModel(sessionID: sessionID, modelID: modelID)
    }

    public func selectReasoningLevel(
        sessionID: String,
        level: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        try await service.updateReasoningLevel(
            sessionID: sessionID,
            level: level,
            enabled: enabled
        )
    }

    public func selectRemoteConnection(
        sessionID: String,
        connectionID: String?
    ) async throws -> ConversationRuntimeSettings {
        try await service.updateRemoteConnection(
            sessionID: sessionID,
            connectionID: connectionID
        )
    }
}
