import Foundation
import Testing
@testable import ChatOSCore

@Suite("Media Studio model catalog application boundary")
struct MediaStudioModelCatalogUseCaseTests {
    @Test("catalog separates chat, image generation, and supported video operations")
    func loadsAndPartitionsCatalog() async throws {
        let models = [
            Self.model(id: "chat", modelName: "gpt-6.1-sol"),
            Self.model(id: "image", modelName: "gpt-image-1"),
            Self.model(id: "image2", modelName: "gpt-image-2"),
            Self.model(id: "h3", modelName: "  minimax-h3  "),
            Self.model(id: "h3-max", modelName: "MiniMax-H3-Max"),
        ]
        let provider = RecordingModelCatalogProvider(models: models)

        let catalog = try await MediaStudioModelCatalogUseCase(provider: provider).load()

        #expect(await provider.fetchCount == 1)
        #expect(catalog.allModels.map(\.id) == ["chat", "image", "image2", "h3", "h3-max"])
        #expect(catalog.textModels.map(\.id) == ["chat"])
        #expect(catalog.imageModels.map(\.id) == ["image", "image2"])
        #expect(catalog.videoModels.map(\.id) == ["h3"])
    }

    @Test("lifecycle state rejects replaced owner and account requests")
    func lifecycleRejectsStaleTokens() {
        var state = MediaStudioModelCatalogState()
        let aliceSession = UUID()
        let old = state.begin(ownerUserID: "alice", accountSessionID: aliceSession)
        let current = state.begin(ownerUserID: "alice", accountSessionID: aliceSession)

        #expect(!state.accepts(old, ownerUserID: "alice", accountSessionID: aliceSession))
        #expect(!state.accepts(current, ownerUserID: "bob", accountSessionID: aliceSession))
        #expect(!state.accepts(current, ownerUserID: "alice", accountSessionID: UUID()))
        #expect(state.accepts(current, ownerUserID: "alice", accountSessionID: aliceSession))
        let didFinish = state.finish(current)
        #expect(didFinish)
        #expect(state.activeToken == nil)
    }

    private static func model(id: String, modelName: String) -> MediaGenerationModel {
        .init(
            id: id,
            name: id,
            provider: "test",
            modelName: modelName,
            enabled: true,
            taskEnabled: false,
            hasAPIKey: true
        )
    }
}

private actor RecordingModelCatalogProvider: MediaGenerationModelCatalogProviding {
    let models: [MediaGenerationModel]
    var fetchCount = 0

    init(models: [MediaGenerationModel]) {
        self.models = models
    }

    func fetchModels() async throws -> [MediaGenerationModel] {
        fetchCount += 1
        return models
    }
}
