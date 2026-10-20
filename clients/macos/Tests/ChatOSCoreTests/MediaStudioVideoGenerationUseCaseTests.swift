import Foundation
import Testing
@testable import ChatOSCore

@Suite("Media Studio video generation application boundary")
struct MediaStudioVideoGenerationUseCaseTests {
    @Test("provider progress is followed by saving and the result is persisted")
    func persistsGeneratedVideoWithProgress() async throws {
        let generator = RecordingVideoGenerator()
        let history = RecordingVideoHistory()
        let progress = ProgressRecorder()
        let request = Self.request(prompt: "  a running dog  ")

        let outcome = try await MediaStudioVideoGenerationUseCase(
            generator: generator,
            history: history
        ).generate(request: request, owner: " alice ") { value in
            await progress.append(value)
        }

        #expect(await generator.requests == [request])
        #expect(await progress.values.map(\.status) == ["queued", "in_progress", "saving"])
        #expect(await history.owners == ["alice"])
        #expect(await history.prompts == [request.prompt])
        guard case let .saved(item) = outcome else {
            Issue.record("Expected a saved history item")
            return
        }
        #expect(item.id == "history-video-1")
    }

    @Test("persistence failure keeps the video canvas failure semantics typed")
    func reportsGeneratedVideoWithoutPersistence() async throws {
        let outcome = try await MediaStudioVideoGenerationUseCase(
            generator: FixedVideoGenerator(),
            history: FailingVideoHistory()
        ).generate(request: Self.request(prompt: "clip"), owner: "alice") { _ in }

        guard case let .generatedWithoutPersistence(message) = outcome else {
            Issue.record("Expected generated-without-persistence outcome")
            return
        }
        #expect(message == "disk full")
    }

    @Test("cancellation after provider completion prevents persistence")
    func cancellationBeforePersistence() async throws {
        let generator = DelayedVideoGenerator()
        let history = RecordingVideoHistory()
        let useCase = MediaStudioVideoGenerationUseCase(generator: generator, history: history)
        let task = Task {
            try await useCase.generate(
                request: Self.request(prompt: "late clip"),
                owner: "alice"
            ) { _ in }
        }

        await generator.waitUntilRequested()
        task.cancel()
        await generator.finishIgnoringCancellation()

        await #expect(throws: CancellationError.self) {
            try await task.value
        }
        #expect(await history.prompts.isEmpty)
    }

    @Test("lifecycle state rejects stale owner session and request tokens")
    func lifecycleRejectsStaleTokens() {
        var state = MediaStudioVideoGenerationState()
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

    private static func request(prompt: String) -> VideoGenerationRequest {
        .init(
            modelConfigID: "video-model",
            prompt: prompt,
            size: "1280x720",
            seconds: 4
        )
    }

    fileprivate static func result() -> VideoGenerationResult {
        .init(
            id: "generation-video-1",
            modelConfigID: "video-model",
            modelName: "MiniMax-H3",
            createdAt: "2026-10-21T00:00:00Z",
            mimeType: "video/mp4",
            videoData: Data("video".utf8)
        )
    }
}

private actor ProgressRecorder {
    var values: [VideoGenerationProgress] = []

    func append(_ value: VideoGenerationProgress) {
        values.append(value)
    }
}

private actor RecordingVideoGenerator: MediaVideoGenerating {
    var requests: [VideoGenerationRequest] = []

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        requests.append(request)
        await progress(.init(status: "queued"))
        await progress(.init(status: "in_progress", percent: 50))
        return MediaStudioVideoGenerationUseCaseTests.result()
    }
}

private struct FixedVideoGenerator: MediaVideoGenerating {
    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        MediaStudioVideoGenerationUseCaseTests.result()
    }
}

private actor RecordingVideoHistory: MediaStudioVideoHistoryPersisting {
    var owners: [String] = []
    var prompts: [String] = []

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioVideoHistoryItem {
        owners.append(owner)
        prompts.append(prompt)
        return .init(
            id: "history-video-1",
            prompt: prompt,
            modelName: result.modelName,
            createdAt: Date(timeIntervalSince1970: 0),
            fileURL: URL(fileURLWithPath: "/tmp/video.mp4")
        )
    }
}

private struct FailingVideoHistory: MediaStudioVideoHistoryPersisting {
    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioVideoHistoryItem {
        throw PersistenceError()
    }

    private struct PersistenceError: LocalizedError {
        var errorDescription: String? { "disk full" }
    }
}

private actor DelayedVideoGenerator: MediaVideoGenerating {
    private var continuation: CheckedContinuation<Void, Never>?
    private var requested = false

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        await withTaskCancellationHandler {
            await withCheckedContinuation {
                continuation = $0
                requested = true
            }
        } onCancel: {
            // Deliberately ignore cancellation to model a provider returning late.
        }
        return MediaStudioVideoGenerationUseCaseTests.result()
    }

    func waitUntilRequested() async {
        while !requested { await Task.yield() }
    }

    func finishIgnoringCancellation() {
        continuation?.resume()
        continuation = nil
    }
}
