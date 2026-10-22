import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MediaStudioViewModelTests: XCTestCase {
    func testAccountBindingDefersCatalogHistoryAndStoryActivation() async throws {
        let service = DelayedMediaStudioModelService()
        let viewModel = MediaStudioViewModel(service: service)
        viewModel.bindAccount(userID: "lazy-media-test")
        await Task.yield()

        let initialFetches = await service.numberOfFetches()
        XCTAssertEqual(initialFetches, 0)
        XCTAssertFalse(viewModel.isLoadingModels)
        XCTAssertFalse(viewModel.isLoadingHistory)
        XCTAssertNil(viewModel.stories.owner)
        XCTAssertFalse(viewModel.stories.isLoading)

        viewModel.loadIfNeeded()
        XCTAssertTrue(viewModel.isLoadingHistory)
        try await waitUntil { await service.hasDelayedFetch() }
        XCTAssertNil(viewModel.stories.owner)
        await service.resumeDelayedFetch()
        try await waitUntil { !viewModel.isLoadingModels && !viewModel.isLoadingHistory }
        viewModel.loadIfNeeded()
        await Task.yield()
        let loadedFetches = await service.numberOfFetches()
        XCTAssertEqual(loadedFetches, 1)
    }

    func testLoadingBeforeAuthenticationDoesNotConsumeFirstActivation() async throws {
        let service = DelayedMediaStudioModelService()
        let viewModel = MediaStudioViewModel(service: service)
        viewModel.loadIfNeeded()
        viewModel.reloadModels()
        viewModel.bindAccount(userID: "lazy-media-test")
        viewModel.loadIfNeeded()

        try await waitUntil { await service.hasDelayedFetch() }
        await service.resumeDelayedFetch()
        try await waitUntil { !viewModel.isLoadingModels }
        XCTAssertEqual(viewModel.models.map(\.id), ["old-model"])
    }

    func testStoryCatalogActivatesOnlyForStoryOrHistorySection() {
        for section in [MediaStudioViewModel.Section.story, .history] {
            let viewModel = MediaStudioViewModel(service: MediaStudioFailureService())
            viewModel.bindAccount(userID: "lazy-story-test")
            viewModel.section = .video
            viewModel.loadSelectedSectionIfNeeded()
            XCTAssertNil(viewModel.stories.owner)

            viewModel.section = section
            viewModel.loadSelectedSectionIfNeeded()
            XCTAssertEqual(viewModel.stories.owner, "lazy-story-test")
            XCTAssertTrue(viewModel.stories.isLoading)
            viewModel.resetForSignedOut()
            XCTAssertNil(viewModel.stories.owner)
        }
    }

    func testBindingAnotherAccountDiscardsLoadedStateWithoutStartingItsLoads() async throws {
        let service = DelayedMediaStudioModelService()
        let viewModel = MediaStudioViewModel(service: service)
        viewModel.bindAccount(userID: "alice")
        viewModel.loadIfNeeded()
        try await waitUntil { await service.hasDelayedFetch() }
        viewModel.bindAccount(userID: "bob")
        await service.resumeDelayedFetch()
        await Task.yield()

        XCTAssertTrue(viewModel.models.isEmpty)
        XCTAssertFalse(viewModel.isLoadingModels)
        XCTAssertFalse(viewModel.isLoadingHistory)
        XCTAssertNil(viewModel.stories.owner)
        let beforeActivation = await service.numberOfFetches()
        XCTAssertEqual(beforeActivation, 1)
        viewModel.loadIfNeeded()
        try await waitUntil { viewModel.models.map(\.id) == ["new-model"] }
    }

    func testOnlyMiniMaxH3IsExposedForVideoGeneration() async throws {
        let viewModel = MediaStudioViewModel(service: MediaStudioFailureService())
        viewModel.activate(userID: "media-studio-test")
        viewModel.loadIfNeeded()
        for _ in 0..<100 where viewModel.isLoadingModels {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(viewModel.videoModels.map(\.id), ["h3"])
        XCTAssertEqual(viewModel.selectedVideoModelID, "h3")
        XCTAssertEqual(viewModel.videoSize, "768P")
        XCTAssertEqual(viewModel.videoSeconds, 4)
    }

    func testRejectedCreationClearsQueuedProgressAndAllowsRetry() async throws {
        let viewModel = MediaStudioViewModel(service: MediaStudioFailureService())
        viewModel.activate(userID: "media-studio-test")
        viewModel.loadIfNeeded()
        for _ in 0..<100 where viewModel.isLoadingModels {
            try await Task.sleep(for: .milliseconds(10))
        }
        viewModel.videoPrompt = "A running dog"
        viewModel.generateVideo()
        for _ in 0..<100 where viewModel.isGeneratingVideo {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(viewModel.videoProgress?.status, "failed")
        XCTAssertNil(viewModel.videoProgress?.percent)
        XCTAssertNotNil(viewModel.errorMessage)
        XCTAssertTrue(viewModel.canGenerateVideo)
    }

    func testLateModelRefreshCannotOverwriteNewerCatalog() async throws {
        let service = DelayedMediaStudioModelService()
        let viewModel = MediaStudioViewModel(service: service)
        viewModel.activate(userID: "media-studio-test")

        viewModel.loadIfNeeded()
        try await waitUntil { await service.hasDelayedFetch() }
        viewModel.reloadModels()
        try await waitUntil { viewModel.models.map(\.id) == ["new-model"] }
        await service.resumeDelayedFetch()
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertEqual(viewModel.models.map(\.id), ["new-model"])
        XCTAssertEqual(viewModel.selectedModelID, "new-model")
        XCTAssertFalse(viewModel.isLoadingModels)
    }

    func testPreviousAccountModelCatalogCannotOverwriteCurrentAccount() async throws {
        let service = DelayedMediaStudioModelService()
        let viewModel = MediaStudioViewModel(service: service)
        viewModel.activate(userID: "alice")

        viewModel.loadIfNeeded()
        try await waitUntil { await service.hasDelayedFetch() }
        viewModel.activate(userID: "bob")
        viewModel.loadIfNeeded()
        try await waitUntil { viewModel.models.map(\.id) == ["new-model"] }
        await service.resumeDelayedFetch()
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertEqual(viewModel.models.map(\.id), ["new-model"])
        XCTAssertEqual(viewModel.selectedModelID, "new-model")
        XCTAssertFalse(viewModel.isLoadingModels)
    }

    func testDelayedImageGenerationDoesNotRetainViewModel() async throws {
        let service = DelayedMediaStudioImageService()
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("MediaStudioViewModelTests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        var viewModel: MediaStudioViewModel? = MediaStudioViewModel(
            service: service,
            historyStore: MediaStudioHistoryStore(root: root)
        )
        weak let weakViewModel = viewModel
        viewModel?.activate(userID: "media-studio-test")
        viewModel?.loadIfNeeded()
        try await waitUntil {
            viewModel?.selectedModelID == "image-model"
                && viewModel?.isLoadingHistory == false
        }
        viewModel?.prompt = "A quiet lake"

        viewModel?.generate()
        try await waitUntil { await service.hasDelayedImageRequest() }
        viewModel = nil

        XCTAssertNil(weakViewModel)

        await service.resumeImageRequest()
        try await Task.sleep(for: .milliseconds(20))
        XCTAssertNil(weakViewModel)
    }

    private func waitUntil(
        _ condition: @escaping @MainActor () async -> Bool
    ) async throws {
        for _ in 0..<100 {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for media studio state")
    }
}

private actor DelayedMediaStudioImageService: MediaGenerationServicing {
    private var imageContinuation: CheckedContinuation<Void, Never>?

    func fetchModels() async throws -> [MediaGenerationModel] {
        [
            .init(
                id: "image-model",
                name: "Image Model",
                provider: "openai",
                modelName: "gpt-image-1",
                enabled: true,
                taskEnabled: false,
                hasAPIKey: true
            ),
        ]
    }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        await withCheckedContinuation { continuation in
            imageContinuation = continuation
        }
        return .init(
            id: "image-result",
            modelConfigID: request.modelConfigID,
            modelName: "gpt-image-1",
            createdAt: "2026-10-20T00:00:00Z",
            images: [
                .init(
                    id: "asset-1",
                    mimeType: "image/png",
                    base64Data: Data([0x89, 0x50, 0x4E, 0x47]).base64EncodedString()
                ),
            ]
        )
    }

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        throw URLError(.badServerResponse)
    }

    func hasDelayedImageRequest() -> Bool { imageContinuation != nil }

    func resumeImageRequest() {
        imageContinuation?.resume()
        imageContinuation = nil
    }
}

private actor DelayedMediaStudioModelService: MediaGenerationServicing {
    private var fetchCount = 0
    private var delayedFetchContinuation: CheckedContinuation<Void, Never>?

    func fetchModels() async throws -> [MediaGenerationModel] {
        fetchCount += 1
        if fetchCount == 1 {
            await withCheckedContinuation { continuation in
                delayedFetchContinuation = continuation
            }
            return [Self.model(id: "old-model")]
        }
        return [Self.model(id: "new-model")]
    }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw URLError(.badServerResponse)
    }

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        throw URLError(.badServerResponse)
    }

    func hasDelayedFetch() -> Bool { delayedFetchContinuation != nil }

    func numberOfFetches() -> Int { fetchCount }

    func resumeDelayedFetch() {
        delayedFetchContinuation?.resume()
        delayedFetchContinuation = nil
    }

    private static func model(id: String) -> MediaGenerationModel {
        .init(
            id: id,
            name: id,
            provider: "gpt",
            modelName: id,
            enabled: true,
            taskEnabled: false,
            hasAPIKey: true
        )
    }
}

private struct MediaStudioFailureService: MediaGenerationServicing {
    func fetchModels() async throws -> [MediaGenerationModel] {
        [("h3", "MiniMax-H3"), ("max", "MiniMax-H3-Max")].map { id, name in
            .init(id: id, name: name, provider: "gpt", modelName: name,
                  enabled: true, taskEnabled: false, hasAPIKey: true)
        }
    }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw URLError(.badServerResponse)
    }

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        throw URLError(.badServerResponse)
    }
}
