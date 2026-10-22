import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

final class MediaStudioVideoJobStoreTests: XCTestCase {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioVideoJobStoreTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    func testPreparedIntentCopiesInputAndKeepsStableSubmissionIdentityAcrossRestart() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("selected.png")
        let bytes = Data(repeating: 0x5A, count: 4_096)
        try bytes.write(to: source)
        let first = MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        let prepared = try await first.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "durable prompt",
                size: "768P",
                seconds: 4,
                inputImage: .init(name: "selected.png", mimeType: "image/png", fileURL: source)
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )

        XCTAssertEqual(prepared.mode, .create)
        XCTAssertEqual(prepared.request.clientRequestID, prepared.snapshot.idempotencyKey)
        let durableInput = try XCTUnwrap(prepared.request.inputImage?.fileURL)
        XCTAssertNotEqual(durableInput.standardizedFileURL, source.standardizedFileURL)
        XCTAssertEqual(try Data(contentsOf: durableInput), bytes)

        let restarted = MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        let recoveredValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recovered = try XCTUnwrap(recoveredValue)
        XCTAssertEqual(recovered.mode, .create)
        XCTAssertEqual(recovered.snapshot.id, prepared.snapshot.id)
        XCTAssertEqual(recovered.request.clientRequestID, prepared.snapshot.idempotencyKey)
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(recovered.request.inputImage?.fileURL)), bytes)
    }

    func testSubmittingWithoutProviderIdentityBecomesUncertainAndCannotBeRecreated() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let first = MediaStudioVideoJobStore(root: root)
        let prepared = try await first.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "possibly accepted",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await first.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: UUID()
        )

        let restarted = MediaStudioVideoJobStore(root: root)
        let recoveredValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recovered = try XCTUnwrap(recoveredValue)
        XCTAssertEqual(recovered.mode, .needsReview)
        XCTAssertEqual(recovered.snapshot.status, .submissionUncertain)
        XCTAssertNil(recovered.snapshot.providerJobID)
        XCTAssertFalse(recovered.snapshot.retryable)

        let recoveredAgainValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recoveredAgain = try XCTUnwrap(recoveredAgainValue)
        XCTAssertEqual(recoveredAgain.mode, .needsReview)
        XCTAssertEqual(recoveredAgain.snapshot.id, prepared.snapshot.id)
    }

    func testChangedDurableInputIsRejectedBeforeAnyRecoveredSubmission() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("selected.png")
        try Data(repeating: 0x11, count: 1_024).write(to: source)
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "immutable input",
                size: "768P",
                seconds: 4,
                inputImage: .init(name: "selected.png", mimeType: "image/png", fileURL: source)
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let durableURL = try XCTUnwrap(prepared.request.inputImage?.fileURL)
        try Data(repeating: 0x22, count: 1_024).write(to: durableURL)

        do {
            _ = try await MediaStudioVideoJobStore(root: jobRoot).recover(
                owner: "alice",
                sessionEpoch: UUID()
            )
            XCTFail("Changed durable input must not be submitted")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.invalidInput.localizedDescription
            )
        }
    }

    func testCorruptCommittedManifestBlocksRecoveryInsteadOfLookingLikeNoJob() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        _ = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "committed intent",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: jobRoot,
            includingPropertiesForKeys: [.isRegularFileKey]
        ))
        let manifestURL = try XCTUnwrap(
            enumerator.compactMap { $0 as? URL }.first { $0.lastPathComponent == "job.json" }
        )
        try Data("broken".utf8).write(to: manifestURL)

        do {
            _ = try await MediaStudioVideoJobStore(root: jobRoot).recover(
                owner: "alice",
                sessionEpoch: UUID()
            )
            XCTFail("A corrupt committed job must block recovery")
        } catch {
            XCTAssertFalse(error.localizedDescription.isEmpty)
        }
    }

    func testProviderIdentityResumesWithoutCreateAndReleasesSubmissionInputs() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("selected.png")
        try Data(repeating: 0x44, count: 2_048).write(to: source)
        let jobRoot = root.appendingPathComponent("jobs")
        let first = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await first.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "resume me",
                size: "768P",
                seconds: 4,
                inputImage: .init(name: "selected.png", mimeType: "image/png", fileURL: source)
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await first.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: UUID()
        )
        try await first.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "queued", jobID: "provider-42")
        )

        let restarted = MediaStudioVideoJobStore(root: jobRoot)
        let recoveredValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recovered = try XCTUnwrap(recoveredValue)
        XCTAssertEqual(recovered.mode, .resume(providerJobID: "provider-42"))
        XCTAssertEqual(recovered.snapshot.retryCount, 1)
        XCTAssertNil(recovered.request.inputImage)
        XCTAssertEqual(recovered.request.clientRequestID, prepared.snapshot.idempotencyKey)
    }

    func testRecoverySessionEpochFencesLateWriterFromPreviousSession() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let oldEpoch = UUID()
        let newEpoch = UUID()
        let store = MediaStudioVideoJobStore(root: root)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "fenced",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: oldEpoch
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: oldEpoch
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: oldEpoch,
            progress: .init(status: "queued", jobID: "provider-fenced")
        )
        _ = try await store.recover(owner: "alice", sessionEpoch: newEpoch)

        do {
            try await store.recordProgress(
                id: prepared.snapshot.id,
                owner: "alice",
                sessionEpoch: oldEpoch,
                progress: .init(status: "failed", jobID: "provider-fenced")
            )
            XCTFail("Previous session must not advance recovered job state")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.staleSession.localizedDescription
            )
        }
        let snapshot = try await store.snapshot(id: prepared.snapshot.id, owner: "alice")
        XCTAssertEqual(snapshot.sessionEpoch, newEpoch)
        XCTAssertNotEqual(snapshot.status, .failed)
    }

    func testServiceNeverSubmitsAnUncertainRecoveredJob() async throws {
        let root = try directory()
        let store = MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "do not duplicate",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: UUID()
        )
        let generator = DurableVideoTestService()
        let history = DurableVideoTestHistory(root: root.appendingPathComponent("history"))
        let service = MediaStudioVideoJobService(
            generator: generator,
            history: history,
            store: MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        )

        let result = try await service.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            progress: { _ in }
        )
        guard case let .needsReview(progress, _) = result else {
            return XCTFail("Expected uncertain submission review")
        }
        XCTAssertEqual(progress.status, "submission_uncertain")
        let calls = await generator.calls()
        XCTAssertEqual(calls.creates, 0)
        XCTAssertEqual(calls.resumes, 0)
    }

    func testServiceResumesProviderJobAndCompletesDurableRecord() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "finish after restart",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: UUID()
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "queued", jobID: "provider-77")
        )
        let generator = DurableVideoTestService()
        let service = MediaStudioVideoJobService(
            generator: generator,
            history: DurableVideoTestHistory(root: root.appendingPathComponent("history")),
            store: MediaStudioVideoJobStore(root: jobRoot)
        )

        let result = try await service.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            progress: { _ in }
        )
        guard case let .finished(.saved(item)) = result else {
            return XCTFail("Expected resumed video to finish")
        }
        XCTAssertEqual(item.prompt, "finish after restart")
        let calls = await generator.calls()
        XCTAssertEqual(calls.creates, 0)
        XCTAssertEqual(calls.resumes, 1)
        XCTAssertEqual(calls.lastProviderJobID, "provider-77")
        let snapshot = try await store.snapshot(id: prepared.snapshot.id, owner: "alice")
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, "provider-77")
    }

    func testFinalizeRetryReusesCommittedHistoryRecordInsteadOfDuplicatingVideo() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let history = MediaStudioHistoryStore(root: root.appendingPathComponent("history"))
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "commit exactly once",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "completed", jobID: "provider-finalize")
        )
        let providerResult = VideoGenerationResult(
            id: "provider-finalize",
            modelConfigID: "video-model",
            modelName: "MiniMax-H3",
            createdAt: "2026-10-22T00:00:00Z",
            mimeType: "video/mp4",
            videoData: Data("video".utf8)
        )
        _ = try await history.saveVideo(
            providerResult,
            prompt: "commit exactly once",
            owner: "alice",
            durableRecordID: prepared.snapshot.id.uuidString.lowercased()
        )

        let service = MediaStudioVideoJobService(
            generator: DurableVideoTestService(),
            history: history,
            store: MediaStudioVideoJobStore(root: jobRoot)
        )
        _ = try await service.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            progress: { _ in }
        )

        let page = try await history.load(owner: "alice")
        XCTAssertEqual(page.videos.count, 1)
        XCTAssertEqual(page.videos.first?.id, prepared.snapshot.id.uuidString.lowercased())
        XCTAssertEqual(page.videos.first?.prompt, "commit exactly once")
    }

    @MainActor
    func testViewModelActivationRestoresProviderJobWithoutUserResubmission() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "restore in studio",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: UUID()
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "queued", jobID: "provider-vm")
        )
        let generator = DurableVideoTestService()
        let viewModel = MediaStudioViewModel(
            service: generator,
            historyStore: MediaStudioHistoryStore(root: root.appendingPathComponent("history")),
            videoJobStore: MediaStudioVideoJobStore(root: jobRoot)
        )

        viewModel.activate(userID: "alice")
        for _ in 0..<200 {
            if !viewModel.isRecoveringVideoJob && !viewModel.isGeneratingVideo,
               viewModel.videoHistory.first?.prompt == "restore in studio" {
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }

        XCTAssertEqual(viewModel.videoHistory.first?.prompt, "restore in studio")
        XCTAssertEqual(viewModel.videoProgress?.status, "completed")
        let calls = await generator.calls()
        XCTAssertEqual(calls.creates, 0)
        XCTAssertEqual(calls.resumes, 1)
        XCTAssertEqual(calls.lastProviderJobID, "provider-vm")
    }
}

private actor DurableVideoTestService: ResumableVideoGenerationServicing {
    private var createCount = 0
    private var resumeCount = 0
    private var providerJobID: String?

    func fetchModels() async throws -> [MediaGenerationModel] { [] }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw URLError(.unsupportedURL)
    }

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        createCount += 1
        return Self.result(id: "unexpected-create")
    }

    func resumeVideo(
        _ request: VideoGenerationRequest,
        jobID: String,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        resumeCount += 1
        providerJobID = jobID
        await progress(.init(status: "completed", percent: 100, jobID: jobID))
        await progress(.init(status: "downloading", jobID: jobID))
        return Self.result(id: jobID)
    }

    func calls() -> (creates: Int, resumes: Int, lastProviderJobID: String?) {
        (createCount, resumeCount, providerJobID)
    }

    nonisolated private static func result(id: String) -> VideoGenerationResult {
        .init(
            id: id,
            modelConfigID: "video-model",
            modelName: "MiniMax-H3",
            createdAt: "2026-10-22T00:00:00Z",
            mimeType: "video/mp4",
            videoData: Data("video".utf8)
        )
    }
}

private actor DurableVideoTestHistory: MediaStudioDurableVideoHistoryPersisting {
    let root: URL

    init(root: URL) { self.root = root }

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioVideoHistoryItem {
        try await saveVideo(
            result,
            prompt: prompt,
            owner: owner,
            durableRecordID: result.id
        )
    }

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String,
        durableRecordID: String
    ) async throws -> MediaStudioVideoHistoryItem {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let url = root.appendingPathComponent("\(durableRecordID).mp4")
        try result.videoData.write(to: url)
        return .init(
            id: result.id,
            prompt: prompt,
            modelName: result.modelName,
            createdAt: Date(timeIntervalSince1970: 1),
            fileURL: url
        )
    }
}
