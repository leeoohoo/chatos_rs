import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MediaStudioViewModelTests: XCTestCase {
    func testModelChangesResetUnsupportedResolutionAndDuration() async throws {
        let viewModel = MediaStudioViewModel(service: MediaStudioFailureService())
        viewModel.activate(userID: "media-studio-test")
        viewModel.loadIfNeeded()
        for _ in 0..<100 where viewModel.isLoadingModels {
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
