import Foundation
import Testing
@testable import ChatOSCore

@Suite("Media Studio model catalog application boundary")
struct MediaStudioModelCatalogUseCaseTests {
    @Test("catalog keeps the image list and applies the existing video admission rule")
    func loadsAndPartitionsCatalog() async throws {
        let models = [
            Self.model(id: "image", modelName: "gpt-image-1"),
            Self.model(id: "h3", modelName: "  minimax-h3  "),
            Self.model(id: "h3-max", modelName: "MiniMax-H3-Max"),
        ]
        let provider = RecordingModelCatalogProvider(models: models)

        let catalog = try await MediaStudioModelCatalogUseCase(provider: provider).load()

        #expect(await provider.fetchCount == 1)
        #expect(catalog.imageModels.map(\.id) == ["image", "h3", "h3-max"])
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
