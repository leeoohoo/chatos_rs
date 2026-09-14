import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MediaStudioViewModelTests: XCTestCase {
    func testModelCatalogRetriesAfterTransientStartupFailure() async throws {
        let service = RetryableMediaStudioService()
        let viewModel = MediaStudioViewModel(
            service: service,
            historyStore: testHistoryStore(),
            storyStore: testStoryStore(),
            agentRuntimeSettings: AppAgentRuntimePreferencesTestProvider()
        )

        // Merely presenting the UI before the account runtime is ready must
        // not start and permanently cache a failed model request.
        viewModel.loadIfNeeded()
        try await Task.sleep(for: .milliseconds(20))
        let fetchCountBeforeActivation = await service.currentFetchCount()
        XCTAssertEqual(fetchCountBeforeActivation, 0)

        viewModel.activate(userID: "media-studio-test")
        viewModel.loadIfNeeded()
        try await waitForModelLoad(viewModel)
        XCTAssertNotNil(viewModel.errorMessage)
        XCTAssertTrue(viewModel.models.isEmpty)

        viewModel.loadIfNeeded()
        try await waitForModelLoad(viewModel)
        XCTAssertNil(viewModel.errorMessage)
        XCTAssertEqual(viewModel.models.map(\.id), ["ready-model"])
        let finalFetchCount = await service.currentFetchCount()
        XCTAssertEqual(finalFetchCount, 2)
    }

    func testModelChangesResetUnsupportedResolutionAndDuration() async throws {
        let viewModel = MediaStudioViewModel(
            service: MediaStudioFailureService(),
            historyStore: testHistoryStore(),
            storyStore: testStoryStore(),
            agentRuntimeSettings: AppAgentRuntimePreferencesTestProvider()
        )
        viewModel.activate(userID: "media-studio-test")
        viewModel.loadIfNeeded()
        for _ in 0..<100 where viewModel.isLoadingModels || viewModel.isLoadingHistory {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(viewModel.videoSize, "768P")
        XCTAssertEqual(viewModel.videoSeconds, 4)
        viewModel.videoSize = "2K"
        viewModel.selectedVideoModelID = "max"
        XCTAssertEqual(viewModel.videoSize, "768P")
        XCTAssertEqual(viewModel.videoSeconds, 5)
    }

    func testRejectedCreationClearsQueuedProgressAndAllowsRetry() async throws {
        let viewModel = MediaStudioViewModel(
            service: MediaStudioFailureService(),
            historyStore: testHistoryStore(),
            storyStore: testStoryStore(),
            agentRuntimeSettings: AppAgentRuntimePreferencesTestProvider()
        )
        viewModel.activate(userID: "media-studio-test")
        viewModel.loadIfNeeded()
        for _ in 0..<100 where viewModel.isLoadingModels || viewModel.isLoadingHistory {
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

    private func testHistoryStore() -> MediaStudioHistoryStore {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("MediaStudioViewModelTests-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return makeMediaStudioHistoryStore(root: root)
    }

    private func testStoryStore() -> StoryProjectStore {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("MediaStudioStoryTests-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return makeStoryProjectStore(root: root)
    }

    private func waitForModelLoad(_ viewModel: MediaStudioViewModel) async throws {
        for _ in 0..<100 {
            if !viewModel.isLoadingModels { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for model catalog")
    }
}

private actor RetryableMediaStudioService: MediaGenerationServicing {
    private(set) var fetchCount = 0

    func currentFetchCount() -> Int { fetchCount }

    func fetchModels() async throws -> [MediaGenerationModel] {
        fetchCount += 1
        if fetchCount == 1 { throw URLError(.cannotConnectToHost) }
        return [
            .init(
                id: "ready-model",
                name: "Ready Model",
                provider: "openai",
                modelName: "ready-model",
                enabled: true,
                taskEnabled: true,
                hasAPIKey: true
            ),
        ]
    }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw URLError(.unsupportedURL)
    }

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        throw URLError(.unsupportedURL)
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
