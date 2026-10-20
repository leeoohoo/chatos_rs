import Foundation
import Testing
@testable import ChatOSCore

@Suite("Media Studio image reference application boundary")
struct MediaStudioImageReferenceUseCaseTests {
    @Test("local import routes files and merges without duplicates")
    func importsLocalFiles() async throws {
        let first = Self.input(name: "first.png")
        let second = Self.input(name: "second.png")
        let importer = RecordingImageReferenceImporter(
            localResult: [first, second],
            generatedResult: []
        )
        let urls = [URL(fileURLWithPath: "/tmp/first.png"), URL(fileURLWithPath: "/tmp/second.png")]

        let merged = try await MediaStudioImageReferenceUseCase(importer: importer)
            .importLocalFiles(urls, current: [first])

        #expect(merged == [first, second])
        #expect(await importer.localRequests == [urls])
    }

    @Test("generated import requires exact assets from current account history")
    func authorizesGeneratedAssets() async throws {
        let asset = GeneratedMediaAsset(id: "asset", mimeType: "image/png", base64Data: "AA==")
        let other = GeneratedMediaAsset(id: "other", mimeType: "image/png", base64Data: "AA==")
        let imported = Self.input(name: "generated-asset.png")
        let importer = RecordingImageReferenceImporter(
            localResult: [],
            generatedResult: [imported]
        )
        let useCase = MediaStudioImageReferenceUseCase(importer: importer)
        let history = [MediaStudioImageHistoryItem(
            id: "history",
            prompt: "prompt",
            modelName: "image",
            createdAt: Date(timeIntervalSince1970: 0),
            images: [asset]
        )]

        let result = try await useCase.importGeneratedAssets(
            [asset],
            authorizedBy: history,
            current: []
        )
        #expect(result == [imported])

        await #expect(throws: MediaStudioImageReferenceError.unauthorizedGeneratedAsset) {
            _ = try await useCase.importGeneratedAssets(
                [other],
                authorizedBy: history,
                current: []
            )
        }
        #expect(await importer.generatedRequests == [[asset]])
    }

    @Test("capacity is rejected before platform import")
    func rejectsTooManyReferences() async {
        let importer = RecordingImageReferenceImporter(localResult: [], generatedResult: [])
        let current = (0..<8).map { Self.input(name: "\($0).png") }

        await #expect(throws: MediaStudioImageReferenceError.tooManyReferences) {
            _ = try await MediaStudioImageReferenceUseCase(importer: importer).importLocalFiles(
                [URL(fileURLWithPath: "/tmp/extra.png")],
                current: current
            )
        }
        #expect(await importer.localRequests.isEmpty)
    }

    @Test("lifecycle rejects replaced owner and account imports")
    func lifecycleRejectsStaleTokens() {
        var state = MediaStudioImageReferenceState()
        let session = UUID()
        let old = state.begin(ownerUserID: "alice", accountSessionID: session, source: .localFiles)
        let current = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            source: .generatedHistory
        )

        #expect(!state.accepts(old, ownerUserID: "alice", accountSessionID: session))
        #expect(!state.accepts(current, ownerUserID: "bob", accountSessionID: session))
        #expect(!state.accepts(current, ownerUserID: "alice", accountSessionID: UUID()))
        #expect(state.accepts(current, ownerUserID: "alice", accountSessionID: session))
        let didFinish = state.finish(current)
        #expect(didFinish)
        #expect(state.activeToken == nil)
    }

    private static func input(name: String) -> ImageGenerationInputImage {
        .init(name: name, mimeType: "image/png", base64Data: "AA==")
    }
}

private actor RecordingImageReferenceImporter: MediaStudioImageReferenceImporting {
    let localResult: [ImageGenerationInputImage]
    let generatedResult: [ImageGenerationInputImage]
    var localRequests: [[URL]] = []
    var generatedRequests: [[GeneratedMediaAsset]] = []

    init(
        localResult: [ImageGenerationInputImage],
        generatedResult: [ImageGenerationInputImage]
    ) {
        self.localResult = localResult
        self.generatedResult = generatedResult
    }

    func importLocalFiles(_ urls: [URL]) async throws -> [ImageGenerationInputImage] {
        localRequests.append(urls)
        return localResult
    }

    func importGeneratedAssets(
        _ assets: [GeneratedMediaAsset]
    ) async throws -> [ImageGenerationInputImage] {
        generatedRequests.append(assets)
        return generatedResult
    }
}
