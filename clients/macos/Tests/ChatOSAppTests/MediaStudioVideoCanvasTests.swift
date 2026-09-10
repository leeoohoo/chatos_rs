import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MediaStudioVideoCanvasTests: XCTestCase {
    func testRegenerationShowsProgressOverOldVideoThenNewResult() async throws {
        let (vm, service) = try await fixture()
        let old = try XCTUnwrap(vm.videoHistory.first)
        XCTAssertEqual(vm.videoCanvasState, .result(old))
        vm.generateVideo()
        XCTAssertEqual(vm.videoCanvasState, .progress(.init(status: "submitting")))
        try await wait { vm.videoProgress?.status == "queued" }
        for progress in [VideoGenerationProgress(status: "queued"), .init(status: "in_progress", percent: 42), .init(status: "downloading")] {
            await service.emit(progress)
            XCTAssertEqual(vm.videoCanvasState, .progress(progress))
            XCTAssertEqual(vm.videoHistory, [old])
        }
        await service.finish(success: true)
        try await wait { !vm.isGeneratingVideo }
        XCTAssertEqual(vm.videoHistory.count, 2)
        let latest = try XCTUnwrap(vm.videoHistory.first)
        XCTAssertEqual(latest.prompt, "new video prompt")
        XCTAssertNotEqual(latest.id, old.id)
        XCTAssertEqual(vm.videoCanvasState, .result(latest))
        XCTAssertTrue(vm.videoHistory.contains(old))
    }

    func testFailedRegenerationDoesNotPresentPreviousVideoAsCurrentResult() async throws {
        let (vm, service) = try await fixture()
        let old = vm.videoHistory
        vm.generateVideo()
        try await wait { vm.videoProgress?.status == "queued" }
        await service.finish(success: false)
        try await wait { !vm.isGeneratingVideo }
        XCTAssertEqual(vm.videoCanvasState, .failed)
        XCTAssertEqual(vm.videoHistory, old)
        XCTAssertNotNil(vm.errorMessage)
        XCTAssertTrue(vm.canGenerateVideo)
    }

    func testStopWaitingRestoresPreviousVideoAndIgnoresLateCompletion() async throws {
        let (vm, service) = try await fixture()
        let old = try XCTUnwrap(vm.videoHistory.first)
        vm.generateVideo()
        try await wait { vm.videoProgress?.status == "queued" }
        vm.cancelVideoGeneration()
        XCTAssertEqual(vm.videoCanvasState, .result(old))
        await service.finish(success: true)
        try await Task.sleep(for: .milliseconds(30))
        XCTAssertEqual(vm.videoCanvasState, .result(old))
        XCTAssertEqual(vm.videoHistory, [old])
    }

    private func fixture() async throws -> (MediaStudioViewModel, VideoCanvasService) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("VideoCanvasTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let store = MediaStudioHistoryStore(root: root)
        _ = try await store.saveVideo(VideoCanvasService.result, prompt: "previous prompt", owner: "canvas-test")
        let service = VideoCanvasService()
        let vm = MediaStudioViewModel(service: service, historyStore: store)
        vm.activate(userID: "canvas-test")
        vm.loadIfNeeded()
        try await wait { !vm.isLoadingHistory && !vm.isLoadingModels }
        vm.videoPrompt = "new video prompt"
        return (vm, service)
    }

    private func wait(_ condition: () -> Bool) async throws {
        for _ in 0..<200 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for video canvas state")
    }
}

private actor VideoCanvasService: MediaGenerationServicing {
    private var continuation: CheckedContinuation<VideoGenerationResult, Error>?
    private var progress: (@Sendable (VideoGenerationProgress) async -> Void)?
    static var result: VideoGenerationResult {
        .init(id: "video", modelConfigID: "h3", modelName: "MiniMax-H3", createdAt: "2026-09-09T00:00:00Z", mimeType: "video/mp4", videoData: Data("test video".utf8))
    }
    func fetchModels() async throws -> [MediaGenerationModel] {
        [.init(id: "h3", name: "H3", provider: "gpt", modelName: "MiniMax-H3", enabled: true, taskEnabled: false, hasAPIKey: true)]
    }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw URLError(.unsupportedURL)
    }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        self.progress = progress
        await progress(.init(status: "queued"))
        return try await withCheckedThrowingContinuation { continuation = $0 }
    }
    func emit(_ value: VideoGenerationProgress) async { await progress?(value) }
    func finish(success: Bool) {
        if success { continuation?.resume(returning: Self.result) }
        else { continuation?.resume(throwing: URLError(.badServerResponse)) }
        continuation = nil
    }
}
