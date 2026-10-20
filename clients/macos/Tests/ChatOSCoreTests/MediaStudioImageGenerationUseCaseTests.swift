import Foundation
import Testing
@testable import ChatOSCore

@Suite("Media Studio image generation application boundary")
struct MediaStudioImageGenerationUseCaseTests {
    @Test("generated media is persisted through the history port")
    func persistsGeneratedMedia() async throws {
        let generator = RecordingImageGenerator()
        let history = RecordingImageHistory()
        let request = Self.request(prompt: "  a quiet lake  ")
        let outcome = try await MediaStudioImageGenerationUseCase(
            generator: generator,
            history: history
        ).generate(request: request, owner: "alice")

        #expect(await generator.requests == [request])
        #expect(await history.owners == ["alice"])
        #expect(await history.prompts == [request.prompt])
        guard case let .saved(item) = outcome else {
            Issue.record("Expected a saved history item")
            return
        }
        #expect(item.id == "history-1")
        #expect(item.images.map(\.id) == ["asset-1"])
    }

    @Test("persistence failure preserves the generated result")
    func preservesGeneratedResultWhenPersistenceFails() async throws {
        let result = Self.result()
        let outcome = try await MediaStudioImageGenerationUseCase(
            generator: FixedImageGenerator(result: result),
            history: FailingImageHistory()
        ).generate(request: Self.request(prompt: "poster"), owner: "alice")

        guard case let .generatedWithoutPersistence(generated, message) = outcome else {
            Issue.record("Expected generated-without-persistence outcome")
            return
        }
        #expect(generated == result)
        #expect(message == "disk full")
    }

    @Test("lifecycle state only accepts the current owner session request")
    func lifecycleRejectsStaleTokens() {
        var state = MediaStudioImageGenerationState()
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

    private static func request(prompt: String) -> ImageGenerationRequest {
        .init(
            modelConfigID: "image-model",
            prompt: prompt,
            size: "1024x1024",
            count: 1,
            referenceImages: []
        )
    }

    fileprivate static func result() -> ImageGenerationResult {
        .init(
            id: "generation-1",
            modelConfigID: "image-model",
            modelName: "gpt-image-1",
            createdAt: "2026-10-21T00:00:00Z",
            images: [.init(id: "asset-1", mimeType: "image/png", base64Data: "AA==")]
        )
    }
}

private actor RecordingImageGenerator: MediaImageGenerating {
    var requests: [ImageGenerationRequest] = []

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        requests.append(request)
        return MediaStudioImageGenerationUseCaseTests.result()
    }
}

private struct FixedImageGenerator: MediaImageGenerating {
    let result: ImageGenerationResult

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        result
    }
}

private actor RecordingImageHistory: MediaStudioImageHistoryPersisting {
    var owners: [String] = []
    var prompts: [String] = []

    func saveImage(
        _ result: ImageGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioImageHistoryItem {
        owners.append(owner)
        prompts.append(prompt)
        return .init(
            id: "history-1",
            prompt: prompt,
            modelName: result.modelName,
            createdAt: Date(timeIntervalSince1970: 0),
            images: result.images
        )
    }
}

private struct FailingImageHistory: MediaStudioImageHistoryPersisting {
    func saveImage(
        _ result: ImageGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioImageHistoryItem {
        throw PersistenceError()
    }

    private struct PersistenceError: LocalizedError {
        var errorDescription: String? { "disk full" }
    }
}
