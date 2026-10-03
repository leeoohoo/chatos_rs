import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MediaStudioViewModelTests: XCTestCase {
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
