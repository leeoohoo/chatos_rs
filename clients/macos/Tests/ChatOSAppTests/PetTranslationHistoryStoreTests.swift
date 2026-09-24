import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Pet translation history")
struct PetTranslationHistoryStoreTests {
    @Test("persists newest-first records and enforces retention")
    func persistsAndLimitsRecords() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PetTranslationHistoryTests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let fileURL = root.appendingPathComponent("history.json")
        let store = PetTranslationHistoryStore(fileURL: fileURL, limit: 2)

        _ = try await store.append(record(index: 1))
        _ = try await store.append(record(index: 2))
        let latest = try await store.append(record(index: 3))

        #expect(latest.map(\.sourceText) == ["source 3", "source 2"])
        let reloaded = try await PetTranslationHistoryStore(
            fileURL: fileURL,
            limit: 2
        ).records()
        #expect(reloaded == latest)
    }

    @Test("clear removes every stored record")
    func clearsRecords() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PetTranslationHistoryTests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = PetTranslationHistoryStore(
            fileURL: root.appendingPathComponent("history.json")
        )

        try await store.append(record(index: 1))
        try await store.clear()

        #expect(try await store.records().isEmpty)
    }

    @Test("successful translation clears the submitted composer content")
    @MainActor
    func successfulTranslationClearsComposer() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PetTranslationHistoryTests-\(UUID().uuidString)", isDirectory: true)
        let defaultsName = "PetTranslationHistoryTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: defaultsName)!
        defer {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: defaultsName)
        }
        let model = LocalConnectorModelConfig(
            id: "vision-model",
            name: "Vision Model",
            provider: "openai",
            modelName: "vision-model",
            enabled: true,
            hasAPIKey: true,
            supportsImages: true,
            supportsReasoning: false
        )
        let viewModel = PetTranslationViewModel(
            agent: PetTranslationAgent(services: SuccessfulTranslationServices()),
            historyStore: PetTranslationHistoryStore(
                fileURL: root.appendingPathComponent("history.json")
            ),
            defaults: defaults,
            modelProvider: { [model] }
        )

        viewModel.loadModels()
        try await waitUntil { viewModel.selectedModel != nil }
        viewModel.draft = "please translate"
        viewModel.addPastedImage(
            data: Data([0x89, 0x50, 0x4E, 0x47]),
            mimeType: "image/png",
            suggestedName: "screenshot.png"
        )
        viewModel.translate()

        #expect(viewModel.isTranslating)
        #expect(viewModel.draft.isEmpty)
        #expect(viewModel.attachments.isEmpty)
        try await waitUntil {
            !viewModel.isTranslating
                && viewModel.historyRecords.first?.attachments.first?.name == "screenshot.png"
        }

        #expect(viewModel.displayedResult == "translated")
        #expect(viewModel.historyRecords.first?.sourceText == "please translate")
        #expect(viewModel.historyRecords.first?.attachments.first?.name == "screenshot.png")
    }

    @Test("one-click screenshot translation loads a model and submits automatically")
    @MainActor
    func screenshotTranslationSubmitsWhenModelBecomesReady() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("PetTranslationHistoryTests-\(UUID().uuidString)", isDirectory: true)
        let defaultsName = "PetTranslationHistoryTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: defaultsName)!
        defer {
            try? FileManager.default.removeItem(at: root)
            defaults.removePersistentDomain(forName: defaultsName)
        }
        let model = LocalConnectorModelConfig(
            id: "vision-model",
            name: "Vision Model",
            provider: "openai",
            modelName: "vision-model",
            enabled: true,
            hasAPIKey: true,
            supportsImages: true,
            supportsReasoning: false
        )
        let viewModel = PetTranslationViewModel(
            agent: PetTranslationAgent(services: SuccessfulTranslationServices()),
            historyStore: PetTranslationHistoryStore(
                fileURL: root.appendingPathComponent("history.json")
            ),
            defaults: defaults,
            modelProvider: { [model] }
        )
        viewModel.addPastedImage(
            data: Data([0x89, 0x50, 0x4E, 0x47]),
            mimeType: "image/png",
            suggestedName: "direct-screenshot.png"
        )

        viewModel.translateWhenReady()
        try await waitUntil {
            !viewModel.isTranslating
                && viewModel.historyRecords.first?.attachments.first?.name
                    == "direct-screenshot.png"
        }

        #expect(viewModel.attachments.isEmpty)
        #expect(viewModel.displayedResult == "translated")
        #expect(viewModel.historyRecords.first?.attachments.first?.name == "direct-screenshot.png")
    }

    private func record(index: Int) -> PetTranslationHistoryRecord {
        PetTranslationHistoryRecord(
            id: UUID(),
            createdAt: Date(timeIntervalSince1970: TimeInterval(index)),
            topic: "Topic \(index)",
            sourceText: "source \(index)",
            attachments: [
                .init(name: "screenshot.png", mimeType: "image/png", isImage: true),
            ],
            targetRawValue: "automatic",
            outputStyleRawValue: "bilingual",
            modelID: "model-\(index)",
            modelName: "Model \(index)",
            translatedMarkdown: "translation \(index)"
        )
    }

    @MainActor
    private func waitUntil(
        timeout: Duration = .seconds(10),
        _ condition: @escaping @MainActor () -> Bool
    ) async throws {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        while !condition(), clock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(condition())
    }
}

private struct SuccessfulTranslationServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        SuccessfulTranslationModel()
    }

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy,
        thinkingLevel: String?
    ) async throws -> any AgentModelClient {
        SuccessfulTranslationModel()
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SuccessfulTranslationMemory()
    }
}

private struct SuccessfulTranslationModel: AgentModelClient {
    let usesServerSideCompaction = false

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        response
    }

    func stream(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval,
        onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void
    ) async throws -> AgentMessage {
        await onEvent(.textDelta(response.content))
        return response
    }

    private var response: AgentMessage {
        .init(
            role: .assistant,
            content: "<!-- CHATOS_TRANSLATION_TITLE: Test -->\ntranslated"
        )
    }
}

private struct SuccessfulTranslationMemory: AgentMemoryServicing {
    func ensureThread() async throws {}
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {}
    func compose() async throws -> AgentMemoryContext {
        .init(blocks: [], recentRecords: [])
    }
}
