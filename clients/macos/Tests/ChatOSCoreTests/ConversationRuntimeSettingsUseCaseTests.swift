import Testing
@testable import ChatOSCore

@Suite("ConversationRuntimeSettingsUseCase")
struct ConversationRuntimeSettingsUseCaseTests {
    @Test
    func loadCombinesSettingsAndModelCatalog() async throws {
        let service = RuntimeSettingsServiceSpy()
        let useCase = ConversationRuntimeSettingsUseCase(service: service)

        let snapshot = try await useCase.load(sessionID: "session-1")
        let calls = await service.calls

        #expect(snapshot.settings.selectedModelID == "model-1")
        #expect(snapshot.availableModels.map(\.id) == ["model-1"])
        #expect(calls.count == 2)
        #expect(calls.contains(.fetchSettings("session-1")))
        #expect(calls.contains(.fetchModels))
    }

    @Test
    func mutationsPreserveTypedInputs() async throws {
        let service = RuntimeSettingsServiceSpy()
        let useCase = ConversationRuntimeSettingsUseCase(service: service)

        _ = try await useCase.selectModel(
            sessionID: "session-1",
            modelID: "model-2"
        )
        _ = try await useCase.selectReasoningLevel(
            sessionID: "session-1",
            level: "high",
            enabled: true
        )
        _ = try await useCase.selectRemoteConnection(
            sessionID: "session-1",
            connectionID: "remote-1"
        )

        #expect(await service.calls == [
            .updateModel(sessionID: "session-1", modelID: "model-2"),
            .updateReasoningLevel(sessionID: "session-1", level: "high", enabled: true),
            .updateRemoteConnection(sessionID: "session-1", connectionID: "remote-1"),
        ])
    }
}

private actor RuntimeSettingsServiceSpy: ConversationRuntimeSettingsServicing {
    enum Call: Sendable, Equatable {
        case fetchSettings(String)
        case fetchModels
        case updateModel(sessionID: String, modelID: String)
        case updateReasoningLevel(sessionID: String, level: String, enabled: Bool)
        case updateRemoteConnection(sessionID: String, connectionID: String?)
    }

    private(set) var calls: [Call] = []

    func fetchSettings(sessionID: String) async throws -> ConversationRuntimeSettings {
        calls.append(.fetchSettings(sessionID))
        return settings()
    }

    func fetchAvailableModels() async throws -> [ConversationModelOption] {
        calls.append(.fetchModels)
        return [ConversationModelOption(
            id: "model-1",
            displayName: "Model 1",
            modelName: "gpt-6.1"
        )]
    }

    func updateModel(
        sessionID: String,
        modelID: String
    ) async throws -> ConversationRuntimeSettings {
        calls.append(.updateModel(sessionID: sessionID, modelID: modelID))
        return settings()
    }

    func updateRemoteConnection(
        sessionID: String,
        connectionID: String?
    ) async throws -> ConversationRuntimeSettings {
        calls.append(.updateRemoteConnection(
            sessionID: sessionID,
            connectionID: connectionID
        ))
        return settings()
    }

    func updateReasoning(
        sessionID: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        settings()
    }

    func updateReasoningLevel(
        sessionID: String,
        level: String,
        enabled: Bool
    ) async throws -> ConversationRuntimeSettings {
        calls.append(.updateReasoningLevel(
            sessionID: sessionID,
            level: level,
            enabled: enabled
        ))
        return settings()
    }

    private func settings() -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(selectedModelID: "model-1")
    }
}
