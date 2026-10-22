import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSApp

extension StoryStudioTests {
    func testPersistentMediaBatchUsesFileBackedFrameReferences() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchImageReferenceTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 16,
            pixelsHigh: 16,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let service = RecordingBatchImageService(imageData: png)
        var project = try await readyProject(store)
        let guide = try XCTUnwrap(project.segments[0].firstFrame)
        var hero = StoryCharacter(id: "hero", name: "主角", profile: characterProfile())
        hero.media.images = [guide]
        hero.media.confirmedImageID = guide.id
        project.characters = [hero]
        project.segments[1].characterIDs = [hero.id]
        project.segments[1].firstFrames = .init()
        project.segments[1].lastFrames = .init()
        try await store.save(project, owner: "alice")
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )

        let frameBatch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .frames,
            targets: ["s2"],
            models: models
        )
        let frameResult = try await StoryMediaBatchSession(
            batch: frameBatch,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: UUID()
        ).run()
        XCTAssertEqual(frameResult.status, .completed, frameResult.error ?? "missing batch error")
        var frameProject = frameResult.draft
        frameProject.segments[1].confirmedFrameID = try XCTUnwrap(
            frameProject.segments[1].firstFrames.images.last?.id
        )
        try await store.save(frameProject, owner: "alice")

        let lastFrameBatch = try StoryMediaBatch(
            project: frameProject,
            owner: "alice",
            kind: .lastFrames,
            targets: ["s2"],
            models: models
        )
        let lastFrameResult = try await StoryMediaBatchSession(
            batch: lastFrameBatch,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: UUID()
        ).run()
        XCTAssertEqual(lastFrameResult.status, .completed, lastFrameResult.error ?? "missing batch error")

        let requests = await service.imageRequests()
        guard requests.count == 2 else {
            return XCTFail("Expected two image requests, got \(requests.count)")
        }
        XCTAssertEqual(
            requests[0].referenceImages.map(\.name),
            ["主角.png", "previous-segment-last-frame.png"]
        )
        XCTAssertEqual(
            requests[1].referenceImages.map(\.name),
            ["主角.png", "current-segment-first-frame.png"]
        )
        for request in requests {
            XCTAssertTrue(request.referenceImages.allSatisfy(\.base64Data.isEmpty))
            XCTAssertTrue(request.referenceImages.allSatisfy { $0.fileURL?.isFileURL == true })
            for reference in request.referenceImages {
                XCTAssertFalse(try Data(contentsOf: XCTUnwrap(reference.fileURL)).isEmpty)
            }
        }
    }

    func testPersistentMediaBatchUsesDurableVideoProgressAsItsOnlyWriter() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchProgressTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        addTeardownBlock {
            if FileManager.default.fileExists(atPath: root.path) {
                try FileManager.default.removeItem(at: root)
            }
        }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        let batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        let progressCollector = StoryBatchProgressCollector()
        let session = StoryMediaBatchSession(
            batch: batch,
            store: store,
            media: RepeatedStoryVideoProgressService(),
            videoJobStore: videoJobStore,
            accountSessionID: UUID(),
            publishVideoProgress: { _, _, progress in
                await progressCollector.append(progress.status)
            }
        )

        let completed = try await session.run()
        let progressStatuses = await progressCollector.statuses()

        XCTAssertEqual(completed.status, .completed)
        XCTAssertTrue(completed.events.allSatisfy { !$0.detail.contains("任务 ID") })
        XCTAssertNil(completed.jobs["videos:s1"]?.jobID)
        XCTAssertNil(completed.draft.segments[0].attempt)
        let recordID = try XCTUnwrap(completed.jobs["videos:s1"]?.videoJobRecordID)
        XCTAssertEqual(completed.draft.segments[0].videoJobRecordID, recordID)
        XCTAssertEqual(completed.draft.segments[0].video?.jobID, "repeated-progress-job")
        let snapshot = try await videoJobStore.snapshot(id: recordID, owner: "alice")
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, "repeated-progress-job")
        XCTAssertEqual(snapshot.stateVersion, 7, "Repeated provider phases must be coalesced")
        XCTAssertEqual(progressStatuses, ["submitting", "queued", "in_progress"])
    }

    func testLegacyMediaBatchProviderJobMigratesAndOnlyResumes() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchLegacyProviderTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        var job = StoryMediaBatch.Job()
        job.jobID = "legacy-provider-job"
        batch.jobs["videos:s1"] = job
        batch.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "legacy prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        batch.draft.segments[0].attempt?.jobID = "legacy-provider-job"
        try await store.commitMediaBatch(batch)
        let service = StoryTestService()
        let session = StoryMediaBatchSession(
            batch: batch,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: UUID()
        )

        let completed = try await session.run()
        let serviceEvents = await service.events()

        XCTAssertEqual(serviceEvents, ["resume"])
        XCTAssertEqual(completed.status, .completed)
        XCTAssertEqual(completed.jobs["videos:s1"]?.jobID, "legacy-provider-job")
        let recordID = try XCTUnwrap(completed.jobs["videos:s1"]?.videoJobRecordID)
        XCTAssertNil(completed.draft.segments[0].attempt)
        XCTAssertEqual(completed.draft.segments[0].video?.jobID, "legacy-provider-job")
        let snapshot = try await videoJobStore.snapshot(id: recordID, owner: "alice")
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, "legacy-provider-job")
    }

    func testHistoryPaginationMigratesLegacyBatchWhenItsPageIsLoaded() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchHistoryMigrationTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")

        var newerFinished = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        newerFinished.status = .completed
        try await store.commitMediaBatch(newerFinished)

        var legacy = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        var job = StoryMediaBatch.Job()
        job.jobID = "historical-provider-job"
        legacy.jobs["videos:s1"] = job
        legacy.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "historical prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        legacy.draft.segments[0].attempt?.jobID = job.jobID
        try await store.commitMediaBatch(legacy)

        let finishedURL = try store.fileURL(
            "media-batch-\(newerFinished.id).json",
            projectID: project.id,
            owner: "alice"
        )
        let legacyURL = try store.fileURL(
            "media-batch-\(legacy.id).json",
            projectID: project.id,
            owner: "alice"
        )
        let now = Date()
        try FileManager.default.setAttributes([.modificationDate: now], ofItemAtPath: finishedURL.path)
        try FileManager.default.setAttributes(
            [.modificationDate: now.addingTimeInterval(-60)],
            ofItemAtPath: legacyURL.path
        )

        let adapter = StoryHistoryAdapter(store: store, videoJobStore: videoJobStore)
        let session = UUID()
        let first = try await adapter.loadStoryMediaBatches(
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: project.id,
            after: nil,
            limit: 1
        )
        XCTAssertEqual(first.batches.map(\.id), [newerFinished.id])
        XCTAssertNotNil(first.nextCursor)
        let beforeOlderPage = try await videoJobStore.snapshotIfPresent(
            id: job.intentID,
            owner: "alice"
        )
        XCTAssertNil(beforeOlderPage)

        let second = try await adapter.loadStoryMediaBatches(
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: project.id,
            after: try XCTUnwrap(first.nextCursor),
            limit: 1
        )
        XCTAssertEqual(second.batches.map(\.id), [legacy.id])
        XCTAssertEqual(second.migrationIssueCount, 0)
        XCTAssertEqual(second.migratedProject?.segments[0].videoJobRecordID, job.intentID)

        let migrated = try await store.loadMediaBatch(
            owner: "alice",
            projectID: project.id,
            batchID: legacy.id
        )
        XCTAssertEqual(migrated.jobs["videos:s1"]?.videoJobRecordID, job.intentID)
        XCTAssertNil(migrated.draft.segments[0].attempt)
        XCTAssertEqual(migrated.draft.segments[0].videoJobRecordID, job.intentID)
        let snapshot = try await videoJobStore.snapshot(id: job.intentID, owner: "alice")
        XCTAssertTrue(snapshot.isActive)
        XCTAssertEqual(snapshot.status, .submitted)
        XCTAssertEqual(snapshot.providerJobID, "historical-provider-job")
    }

    func testHistoryMigrationWithoutProviderIdentityBecomesReviewOnly() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchHistoryReviewTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var legacy = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        legacy.jobs["videos:s1"] = job
        legacy.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "uncertain historical prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        try await store.commitMediaBatch(legacy)

        let session = UUID()
        let page = try await StoryHistoryAdapter(
            store: store,
            videoJobStore: videoJobStore
        ).loadStoryMediaBatches(
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: project.id,
            after: nil,
            limit: 20
        )
        XCTAssertEqual(page.migrationIssueCount, 0)
        let snapshot = try await videoJobStore.snapshot(id: job.intentID, owner: "alice")
        XCTAssertTrue(snapshot.isActive)
        XCTAssertEqual(snapshot.status, .submissionUncertain)
        XCTAssertNil(snapshot.providerJobID)
        let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
            projectID: project.id,
            batchID: legacy.id,
            segmentID: "s1",
            attemptID: job.intentID
        )
        let recovery = try await videoJobStore.recover(
            owner: "alice",
            sessionEpoch: session,
            workload: workload
        )
        XCTAssertEqual(recovery?.mode, .needsReview)
    }

    func testHistoryMigrationPreservesPreparedRecordFromPreHandoffCrash() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchHistoryPreparedTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
            projectID: project.id,
            batchID: batch.id,
            segmentID: "s1",
            attemptID: job.intentID
        )
        let firstFrame = try XCTUnwrap(project.segments[0].firstFrame)
        let firstFrameURL = try store.fileURL(
            firstFrame.filename,
            projectID: project.id,
            owner: "alice"
        )
        let request = VideoGenerationRequest(
            modelConfigID: "video",
            prompt: "prepared historical prompt",
            size: "768P",
            seconds: 15,
            inputImage: .init(
                name: "first-frame.png",
                mimeType: firstFrame.mimeType,
                fileURL: firstFrameURL
            ),
            ratio: "16:9"
        )
        let session = UUID()
        _ = try await videoJobStore.stagePrepared(
            request: request,
            id: job.intentID,
            owner: "alice",
            sessionEpoch: session,
            workload: workload
        )
        batch.jobs["videos:s1"] = job
        batch.draft.segments[0].attempt = .init(
            modelConfigID: request.modelConfigID,
            prompt: request.prompt,
            size: request.size,
            ratio: request.ratio,
            seconds: request.seconds
        )
        batch.draft.segments[0].attempt?.id = job.intentID
        try await store.commitMediaBatch(batch)

        let page = try await StoryHistoryAdapter(
            store: store,
            videoJobStore: videoJobStore
        ).loadStoryMediaBatches(
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: project.id,
            after: nil,
            limit: 20
        )
        XCTAssertEqual(page.migrationIssueCount, 0)
        let migrated = try await store.loadMediaBatch(
            owner: "alice",
            projectID: project.id,
            batchID: batch.id
        )
        XCTAssertEqual(migrated.jobs["videos:s1"]?.videoJobRecordID, job.intentID)
        let snapshot = try await videoJobStore.snapshot(id: job.intentID, owner: "alice")
        XCTAssertTrue(snapshot.isActive)
        XCTAssertEqual(snapshot.status, .prepared)
        let recovered = try await videoJobStore.recover(
            owner: "alice",
            sessionEpoch: session,
            workload: workload
        )
        XCTAssertEqual(recovered?.mode, .create)
        XCTAssertNotNil(recovered?.request.inputImage)
    }

    func testHistoryMigrationConflictPreservesLegacyBatchAndCountsIssue() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchHistoryConflictTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var legacy = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        legacy.jobs["videos:s1"] = job
        legacy.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "conflicting historical prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        try await store.commitMediaBatch(legacy)
        let conflictingWorkload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: project.id,
            segmentID: "s1",
            attemptID: job.intentID
        )
        _ = try await videoJobStore.stageSubmissionUncertain(
            request: .init(
                modelConfigID: "video",
                prompt: "conflicting historical prompt",
                size: "768P",
                seconds: 15,
                ratio: "16:9"
            ),
            id: job.intentID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: conflictingWorkload
        )

        let page = try await StoryHistoryAdapter(
            store: store,
            videoJobStore: videoJobStore
        ).loadStoryMediaBatches(
            ownerUserID: "alice",
            accountSessionID: UUID(),
            projectID: project.id,
            after: nil,
            limit: 20
        )
        XCTAssertEqual(page.batches.map(\.id), [legacy.id])
        XCTAssertEqual(page.migrationIssueCount, 1)
        let preserved = try await store.loadMediaBatch(
            owner: "alice",
            projectID: project.id,
            batchID: legacy.id
        )
        XCTAssertNil(preserved.jobs["videos:s1"]?.videoJobRecordID)
        XCTAssertNotNil(preserved.draft.segments[0].attempt)
    }

    func testPreparedMediaBatchRecordSurvivesCrashBeforeHandoffAndCreatesOnce() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchPreparedReplayTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
            projectID: project.id,
            batchID: batch.id,
            segmentID: "s1",
            attemptID: job.intentID
        )
        let firstFrame = try XCTUnwrap(project.segments[0].firstFrame)
        let firstFrameURL = try store.fileURL(
            firstFrame.filename,
            projectID: project.id,
            owner: "alice"
        )
        let request = VideoGenerationRequest(
            modelConfigID: "video",
            prompt: "prepared prompt",
            size: "768P",
            seconds: 15,
            inputImage: .init(
                name: "first-frame.png",
                mimeType: firstFrame.mimeType,
                fileURL: firstFrameURL
            ),
            ratio: "16:9"
        )
        let epoch = UUID()
        _ = try await videoJobStore.stagePrepared(
            request: request,
            id: job.intentID,
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )
        batch.jobs["videos:s1"] = job
        batch.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "prepared prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        batch.draft.segments[0].attempt?.id = job.intentID
        try await store.commitMediaBatch(batch)
        let service = StoryTestService()
        let session = StoryMediaBatchSession(
            batch: batch,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: UUID()
        )

        let completed = try await session.run()
        let serviceEvents = await service.events()

        XCTAssertEqual(serviceEvents, ["video"])
        XCTAssertEqual(completed.status, .completed)
        XCTAssertEqual(completed.jobs["videos:s1"]?.videoJobRecordID, job.intentID)
        XCTAssertNil(completed.draft.segments[0].attempt)
        let snapshot = try await videoJobStore.snapshot(id: job.intentID, owner: "alice")
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, "remote-job")
    }

    func testLegacyMediaBatchWithoutProviderIdentityNeverCreatesAgain() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchUncertainTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        batch.jobs["videos:s1"] = job
        batch.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "ambiguous legacy prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        try await store.commitMediaBatch(batch)
        let service = StoryTestService()
        let session = StoryMediaBatchSession(
            batch: batch,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: UUID()
        )

        let stopped = try await session.run()
        let serviceEvents = await service.events()

        XCTAssertEqual(serviceEvents, [])
        XCTAssertEqual(stopped.status, .needsReview)
        XCTAssertEqual(stopped.jobs["videos:s1"]?.videoJobRecordID, job.intentID)
        XCTAssertNil(stopped.draft.segments[0].attempt)
        let snapshot = try await videoJobStore.snapshot(id: job.intentID, owner: "alice")
        XCTAssertEqual(snapshot.status, .submissionUncertain)
        XCTAssertNil(snapshot.providerJobID)
    }

    func testVerifiedMediaBatchAbandonTerminalizesJobBeforeClearingAssociation() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchVerifiedReleaseTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        batch.jobs["videos:s1"] = job
        batch.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "ambiguous legacy prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        try await store.commitMediaBatch(batch)
        let service = StoryTestService()
        let stopped = try await StoryMediaBatchSession(
            batch: batch,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: UUID()
        ).run()
        XCTAssertEqual(stopped.status, .needsReview)
        let canonical = try await store.load(owner: "alice").projects[0]
        let adapter = StoryMediaBatchExecutionAdapter(
            store: store,
            media: service,
            videoJobStore: videoJobStore
        )

        let abandoned = try await adapter.executeStoryMediaBatch(
            ownerUserID: "alice",
            accountSessionID: UUID(),
            command: .abandon(project: canonical, batchID: batch.id),
            continuationCheck: {},
            shouldPause: { false },
            onEvent: { _ in }
        )

        XCTAssertEqual(abandoned.batch.status, .abandoned)
        XCTAssertNil(abandoned.project.segments[0].videoJobRecordID)
        let reloaded = try await store.loadMediaBatch(
            owner: "alice",
            projectID: project.id,
            batchID: batch.id
        )
        XCTAssertNil(reloaded.jobs["videos:s1"]?.videoJobRecordID)
        let snapshot = try await videoJobStore.snapshot(id: job.intentID, owner: "alice")
        XCTAssertEqual(snapshot.status, .cancelled)
        XCTAssertEqual(snapshot.failurePhase, "verified_retry_release")
    }

    func testCompletedMediaBatchResultReconcilesFinalizingJobWithoutProviderCall() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryBatchFinalizeReplayTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        let videoJobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("jobs", isDirectory: true)
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        var job = StoryMediaBatch.Job()
        job.jobID = "finalizing-provider-job"
        batch.jobs["videos:s1"] = job
        batch.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "legacy prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        batch.draft.segments[0].attempt?.jobID = "finalizing-provider-job"
        try await store.commitMediaBatch(batch)
        let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
            projectID: project.id,
            batchID: batch.id,
            segmentID: "s1",
            attemptID: job.intentID
        )
        let request = VideoGenerationRequest(
            modelConfigID: "video",
            prompt: "legacy prompt",
            size: "768P",
            seconds: 15,
            ratio: "16:9"
        )
        let epoch = UUID()
        _ = try await videoJobStore.stageImportedProviderJob(
            request: request,
            id: job.intentID,
            providerJobID: "finalizing-provider-job",
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )
        var handedOff = try await store.handoffMediaBatchVideoGenerationToMediaJob(
            owner: "alice",
            projectID: project.id,
            batchID: batch.id,
            stepID: "videos:s1",
            expectedIntentID: job.intentID
        )
        _ = try await videoJobStore.activate(
            id: job.intentID,
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )
        try await videoJobStore.markFinalizing(
            id: job.intentID,
            owner: "alice",
            sessionEpoch: epoch,
            providerJobID: "finalizing-provider-job"
        )
        let video = try await store.saveVideo(
            .init(
                id: "finalizing-provider-job",
                modelConfigID: "video",
                modelName: "MiniMax-H3",
                createdAt: "",
                mimeType: "video/mp4",
                videoData: Data("video".utf8)
            ),
            projectID: project.id,
            owner: "alice"
        )
        handedOff.expectedProjectDigest = try StoryAgentRun.digest(handedOff.draft)
        handedOff.jobs["videos:s1"]?.completed = true
        handedOff.draft.segments[0].video = video
        try await store.commitMediaBatch(handedOff)
        let service = StoryTestService()
        let recoveryEpoch = UUID()
        let session = StoryMediaBatchSession(
            batch: handedOff,
            store: store,
            media: service,
            videoJobStore: videoJobStore,
            accountSessionID: recoveryEpoch
        )

        let completed = try await session.run()
        let serviceEvents = await service.events()

        XCTAssertEqual(serviceEvents, [])
        XCTAssertEqual(completed.status, .completed)
        let snapshot = try await videoJobStore.snapshot(id: job.intentID, owner: "alice")
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.sessionEpoch, recoveryEpoch)
    }

    func testPendingVideoConfigurationCannotBeChanged() async throws {
        let (vm, _, _) = try await fixture()
        var draft = makeProject()
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        var attempt = StoryVideoAttempt(modelConfigID: "video", prompt: "original", size: "768P", ratio: "16:9")
        attempt.jobID = "original-id"
        segment.attempt = attempt
        draft.segments = [segment]
        _ = await vm.create(draft, availableModels: models)
        draft.models.videoModelID = "text"
        let changed = await vm.updateSettings(draft, availableModels: models)
        XCTAssertFalse(changed)
        XCTAssertEqual(vm.project?.models.videoModelID, "video")
        XCTAssertEqual(vm.project?.segments[0].attempt?.jobID, "original-id")
        XCTAssertEqual(vm.project?.segments[0].attempt?.prompt, "original")
    }

    func testOneTapDurationAdjustmentExtendsLastShotAndRelation() async throws {
        let (vm, _, _) = try await fixture()
        var draft = makeProject()
        draft.characters = [.init(id: "hero", name: "主角", profile: characterProfile())]
        draft.scenes = [.init(id: "room", name: "房间", profile: sceneProfile())]
        var segment = StorySegment(id: "s1", title: "短片段", synopsis: "短片段",
                                   sourceRange: .init(start: 0, end: 1), seconds: 2,
                                   characterIDs: ["hero"], sceneIDs: ["room"])
        segment.detail = .init(firstFramePrompt: "start",
                               shots: [.init(start: 0, end: 2, prompt: "move")],
                               continuityIn: "in", continuityOut: "out", audio: "", constraints: "")
        draft.segments = [segment]
        draft.relations = [.init(id: "r1", segmentID: "s1", characterID: "hero", sceneID: "room",
                                 action: "走进房间", position: "门口", startSecond: 0, endSecond: 2)]
        let created = await vm.create(draft, availableModels: models)
        XCTAssertTrue(created)

        let videoModel = try XCTUnwrap(models.first { $0.id == "video" })
        vm.adjustSegmentDurationsForVideoModel(["s1"], model: videoModel)
        try await idle(vm)

        XCTAssertEqual(vm.project?.segments[0].seconds, 4)
        XCTAssertEqual(vm.project?.segments[0].detail?.shots.last?.end, 4)
        XCTAssertEqual(vm.project?.relations[0].endSecond, 4)
        XCTAssertEqual(vm.project?.models.supportedVideoDurations, Array(4...15))
    }

    func testStaleVideoBatchIsCompletedWhenItsExactJobAlreadyExistsInProject() async throws {
        let (_, store, _) = try await fixture()
        let original = try await readyProject(store)
        try await store.save(original, owner: "alice")

        var batch = try StoryMediaBatch(project: original, owner: "alice", kind: .videos,
                                        targets: ["s1"], models: models)
        batch.status = .running
        batch.jobs["videos:s1"] = .init(jobID: "provider-job-1", completed: false)
        try await store.commitMediaBatch(batch)

        var completedProject = original
        completedProject.segments[0].attempt = .init(
            modelConfigID: "video", prompt: "prompt", size: "768P", ratio: "16:9", seconds: 15
        )
        completedProject.segments[0].attempt?.jobID = "provider-job-1"
        completedProject.segments[0].attempt?.status = "completed"
        completedProject.segments[0].video = .init(
            filename: "completed.mp4", jobID: "provider-job-1", modelName: "MiniMax-H3"
        )
        try await store.save(completedProject, owner: "alice")

        let recoveryResult = try await store.reconcileCompletedVideoBatch(batch)
        let recovered = try XCTUnwrap(recoveryResult)
        XCTAssertTrue(recovered.finished)
        XCTAssertEqual(recovered.status, .completed)
        XCTAssertTrue(recovered.jobs["videos:s1"]?.completed == true)
        XCTAssertEqual(recovered.draft.segments[0].video?.jobID, "provider-job-1")
        XCTAssertNil(recovered.error)
    }

    func testMediaBatchVideoHandoffReplacesLegacyWritersAndIsIdempotent() async throws {
        let (_, store, _) = try await fixture()
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var batch = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        var job = StoryMediaBatch.Job()
        job.jobID = "provider-existing"
        batch.jobs["videos:s1"] = job
        batch.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "legacy batch prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        batch.draft.segments[0].attempt?.jobID = "provider-existing"
        try await store.commitMediaBatch(batch)

        let handedOff = try await store.handoffMediaBatchVideoGenerationToMediaJob(
            owner: "alice",
            projectID: project.id,
            batchID: batch.id,
            stepID: "videos:s1",
            expectedIntentID: job.intentID
        )
        XCTAssertEqual(handedOff.jobs["videos:s1"]?.videoJobRecordID, job.intentID)
        XCTAssertEqual(handedOff.jobs["videos:s1"]?.jobID, "provider-existing")
        XCTAssertEqual(handedOff.draft.segments[0].videoJobRecordID, job.intentID)
        XCTAssertNil(handedOff.draft.segments[0].attempt)
        let canonical = try await store.load(owner: "alice").projects[0]
        XCTAssertEqual(canonical.segments[0].videoJobRecordID, job.intentID)
        XCTAssertNil(canonical.segments[0].attempt)

        let repeated = try await store.handoffMediaBatchVideoGenerationToMediaJob(
            owner: "alice",
            projectID: project.id,
            batchID: batch.id,
            stepID: "videos:s1",
            expectedIntentID: job.intentID
        )
        XCTAssertEqual(repeated, handedOff)
        do {
            _ = try await store.handoffMediaBatchVideoGenerationToMediaJob(
                owner: "alice",
                projectID: project.id,
                batchID: batch.id,
                stepID: "videos:s1",
                expectedIntentID: UUID()
            )
            XCTFail("A different Batch intent must not claim this durable association")
        } catch {
            XCTAssertEqual(error.localizedDescription, StoryError.unresolvedSubmission.localizedDescription)
        }
    }

    func testMediaBatchHandoffReplayRepairsBatchFirstCrashWindow() async throws {
        let (_, store, _) = try await fixture()
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var legacy = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        legacy.jobs["videos:s1"] = job
        legacy.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "legacy batch prompt",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        try await store.commitMediaBatch(legacy)

        var batchFirst = legacy
        batchFirst.expectedProjectDigest = try StoryAgentRun.digest(legacy.draft)
        batchFirst.jobs["videos:s1"]?.videoJobRecordID = job.intentID
        batchFirst.draft.version = 3
        batchFirst.draft.segments[0].attempt = nil
        batchFirst.draft.segments[0].videoJobRecordID = job.intentID
        try batchFirst.validate(owner: "alice", projectID: project.id)
        let batchManifest = try store.fileURL(
            "media-batch-\(legacy.id).json",
            projectID: project.id,
            owner: "alice"
        )
        try JSONEncoder().encode(batchFirst).write(to: batchManifest, options: .atomic)
        let beforeReplay = try await store.load(owner: "alice").projects[0]
        XCTAssertNotNil(beforeReplay.segments[0].attempt)
        XCTAssertNil(beforeReplay.segments[0].videoJobRecordID)

        _ = try await store.handoffMediaBatchVideoGenerationToMediaJob(
            owner: "alice",
            projectID: project.id,
            batchID: legacy.id,
            stepID: "videos:s1",
            expectedIntentID: job.intentID
        )
        let repaired = try await store.load(owner: "alice").projects[0]
        XCTAssertNil(repaired.segments[0].attempt)
        XCTAssertEqual(repaired.segments[0].videoJobRecordID, job.intentID)
    }

    func testVideoJobReferenceLookupCoversProjectAndBatchFirstCrashWindow() async throws {
        let (_, store, _) = try await fixture()
        var project = try await readyProject(store)
        let directID = UUID()
        project.version = 3
        project.segments[0].videoJobRecordID = directID
        try await store.save(project, owner: "alice")

        let directReference = try await store.referencesVideoJob(
            id: directID,
            owner: "alice",
            projectID: project.id,
            segmentID: "s1",
            batchID: nil
        )
        let missingReference = try await store.referencesVideoJob(
            id: UUID(),
            owner: "alice",
            projectID: project.id,
            segmentID: "s1",
            batchID: nil
        )
        XCTAssertTrue(directReference)
        XCTAssertFalse(missingReference)

        project.segments[0].videoJobRecordID = nil
        try await store.save(project, owner: "alice")
        var legacy = try StoryMediaBatch(
            project: project,
            owner: "alice",
            kind: .videos,
            targets: ["s1"],
            models: models
        )
        let job = StoryMediaBatch.Job()
        legacy.jobs["videos:s1"] = job
        legacy.draft.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "batch crash window",
            size: "768P",
            ratio: "16:9",
            seconds: 15
        )
        try await store.commitMediaBatch(legacy)

        var batchFirst = legacy
        batchFirst.expectedProjectDigest = try StoryAgentRun.digest(legacy.draft)
        batchFirst.jobs["videos:s1"]?.videoJobRecordID = job.intentID
        batchFirst.draft.version = 3
        batchFirst.draft.segments[0].attempt = nil
        batchFirst.draft.segments[0].videoJobRecordID = job.intentID
        try batchFirst.validate(owner: "alice", projectID: project.id)
        let batchManifest = try store.fileURL(
            "media-batch-\(legacy.id).json",
            projectID: project.id,
            owner: "alice"
        )
        try JSONEncoder().encode(batchFirst).write(to: batchManifest, options: .atomic)

        let batchReference = try await store.referencesVideoJob(
            id: job.intentID,
            owner: "alice",
            projectID: project.id,
            segmentID: "s1",
            batchID: legacy.id
        )
        XCTAssertTrue(batchReference)
        try Data("broken batch".utf8).write(to: batchManifest, options: .atomic)
        do {
            _ = try await store.referencesVideoJob(
                id: job.intentID,
                owner: "alice",
                projectID: project.id,
                segmentID: "s1",
                batchID: legacy.id
            )
            XCTFail("A damaged batch reference must fail closed")
        } catch {}
    }

    func testMediaBatchesLoadInBoundedPagesAndRemainDirectlyAddressable() async throws {
        let (_, store, _) = try await fixture()
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        var ids: Set<UUID> = []
        for _ in 0..<3 {
            let batch = try StoryMediaBatch(project: project, owner: "alice", kind: .videos,
                                            targets: ["s1"], models: models)
            ids.insert(batch.id)
            try await store.commitMediaBatch(batch)
        }

        let first = try await store.loadMediaBatches(owner: "alice", projectID: project.id, limit: 2)
        XCTAssertEqual(first.batches.count, 2)
        let cursor = try XCTUnwrap(first.nextCursor)
        let second = try await store.loadMediaBatches(
            owner: "alice", projectID: project.id, after: cursor, limit: 2
        )
        XCTAssertEqual(second.batches.count, 1)
        XCTAssertNil(second.nextCursor)
        XCTAssertEqual(Set((first.batches + second.batches).map(\.id)), ids)
        for id in ids {
            let loaded = try await store.loadMediaBatch(owner: "alice", projectID: project.id, batchID: id)
            XCTAssertEqual(loaded.id, id)
        }
    }

    func testMediaBatchExecutionAdapterOwnsPreparationAndAbandonPersistence() async throws {
        let (_, store, service) = try await fixture()
        var project = makeProject()
        project.props = [
            .init(id: "hero", name: "Hero", description: "Hero portrait"),
        ]
        try await store.save(project, owner: "alice")
        let adapter = StoryMediaBatchExecutionAdapter(
            store: store,
            media: service,
            videoJobStore: MediaStudioVideoJobStore(
                root: store.root.appendingPathComponent("MediaJobs", isDirectory: true)
            )
        )
        let preparation = try adapter.prepareStoryMediaBatch(
            ownerUserID: "alice",
            project: project,
            kind: .assets,
            targetIDs: ["hero"],
            availableModels: models,
            userIdeas: ""
        )
        XCTAssertEqual(preparation.executionMode, .persistentBatch)
        XCTAssertEqual(preparation.steps.map(\.id), ["assets:hero"])

        var persisted = try StoryMediaBatch(
            id: preparation.batchID,
            project: project,
            owner: "alice",
            kind: .assets,
            targets: ["hero"],
            models: models
        )
        let intent = StoryMediaBatch.Job()
        persisted.jobs["assets:hero"] = intent
        persisted.draft.props[0].media.generationAttemptID = intent.intentID
        try await store.commitMediaBatch(persisted)
        let storedProjects = try await store.load(owner: "alice").projects
        let current = try XCTUnwrap(storedProjects.first(where: { $0.id == project.id }))

        let outcome = try await adapter.executeStoryMediaBatch(
            ownerUserID: "alice",
            accountSessionID: UUID(),
            command: .abandon(project: current, batchID: preparation.batchID),
            continuationCheck: {},
            shouldPause: { false },
            onEvent: { _ in }
        )
        XCTAssertEqual(outcome.batch.status, .abandoned)
        XCTAssertEqual(outcome.project.resources[0].imageGenerationAttemptID, nil)
        let reloaded = try await store.loadMediaBatch(
            owner: "alice",
            projectID: project.id,
            batchID: preparation.batchID
        )
        XCTAssertEqual(reloaded.status, .abandoned)
        XCTAssertNil(reloaded.draft.resources[0].imageGenerationAttemptID)
    }

    func testVariableDurationTransitionUsesExplicitKindAndZeroLengthSourceRange() throws {
        var project = makeProject()
        project.source = "前半后半"
        var first = StorySegment(id: "s1", title: "前半", synopsis: "人物留在室内",
                                 sourceRange: .init(start: 0, end: 2), seconds: 10)
        first.detail = .init(firstFramePrompt: "start", shots: [.init(start: 0, end: 10, prompt: "story")],
                             continuityIn: "in", continuityOut: "out", audio: "", constraints: "")
        var transition = StorySegment(id: "t1", title: "转场", synopsis: "从室内转至街道",
                                      sourceRange: .init(start: 2, end: 2), kind: .transition, seconds: 3)
        transition.detail = .init(firstFramePrompt: "same as previous tail",
                                  shots: [.init(start: 0, end: 3, prompt: "match cut")],
                                  continuityIn: "previous tail", continuityOut: "street", audio: "", constraints: "")
        var last = StorySegment(id: "s2", title: "后半", synopsis: "人物到达街道",
                                sourceRange: .init(start: 2, end: 4), seconds: 5)
        last.detail = .init(firstFramePrompt: "street", shots: [.init(start: 0, end: 5, prompt: "story")],
                            continuityIn: "street", continuityOut: "out", audio: "", constraints: "")
        project.segments = [first, transition, last]

        XCTAssertNoThrow(try project.validate())
        XCTAssertEqual(project.totalSeconds, 18)

        project.segments[1].sourceRange.end = 3
        XCTAssertThrowsError(try project.validate(), "A transition must not consume source text")
    }

    func testPromptRegistryAndInspectorUseStableKeysAndProductionPayloads() throws {
        let definitions = StoryPromptRegistry.definitions
        XCTAssertEqual(definitions.count, StoryPromptRegistry.Key.allCases.count)
        XCTAssertEqual(Set(definitions.map(\.key)).count, definitions.count)
        XCTAssertEqual(StoryAgentTools.systemPrompt, StoryPromptRegistry.render(.agentSystem))

        let items = StoryPromptCatalog.items()
        XCTAssertEqual(items.first(where: { $0.registryKey == StoryPromptRegistry.Key.agentSystem.rawValue })?.content,
                       StoryAgentTools.systemPrompt)
        let tool = try XCTUnwrap(items.first { $0.registryKey == "story.tool.story_append_segments" })
        XCTAssertTrue(tool.content?.contains("\"kind\"") == true)
        XCTAssertTrue(tool.content?.contains("\"seconds\"") == true)
        let video = try XCTUnwrap(items.first { $0.registryKey == StoryPromptRegistry.Key.transitionVideo.rawValue })
        XCTAssertTrue(video.content?.contains("独立转场片段") == true)
        XCTAssertTrue(video.content?.contains("不推进新剧情") == true)
        let firstFrame = try XCTUnwrap(items.first { $0.registryKey == StoryPromptRegistry.Key.firstFrameImage.rawValue })
        XCTAssertTrue(firstFrame.usedWhen.contains("自动承接上一段尾帧"))
        XCTAssertTrue(firstFrame.usedWhen.contains("不会调用图片模型"))
    }

    func testOnlyActualVideoTailIsInheritedAndUserFirstFrameIsNotOverwritten() throws {
        var project = makeProject()
        project.segments = [
            .init(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1)),
            .init(id: "s2", title: "B", synopsis: "B", sourceRange: .init(start: 1, end: 2)),
        ]
        let firstTail = StoryImage(filename: "tail-a.png", mimeType: "image/png")
        let secondTail = StoryImage(filename: "tail-b.png", mimeType: "image/png")
        project.segments[0].lastFrames.images = [firstTail, secondTail]
        project.segments[0].lastFrames.confirmedImageID = firstTail.id

        XCTAssertFalse(StoryContinuityContext.reconcileInheritedFirstFrames(&project))
        XCTAssertNil(project.segments[1].firstFrame)
        XCTAssertNil(project.segments[1].inheritedFirstFrameSourceSegmentID)

        project.segments[0].actualVideoLastFrameID = firstTail.id
        XCTAssertTrue(StoryContinuityContext.reconcileInheritedFirstFrames(&project))
        XCTAssertEqual(project.segments[1].firstFrame?.id, firstTail.id)
        XCTAssertEqual(project.segments[1].inheritedFirstFrameSourceSegmentID, "s1")

        project.segments[0].actualVideoLastFrameID = secondTail.id
        XCTAssertTrue(StoryContinuityContext.reconcileInheritedFirstFrames(&project))
        XCTAssertEqual(project.segments[1].firstFrame?.id, secondTail.id)
        XCTAssertEqual(project.segments[0].confirmedLastFrameID, firstTail.id,
                       "The pre-generated provider guide must remain independently confirmed")

        let custom = StoryImage(filename: "custom.png", mimeType: "image/png", generationAttemptID: UUID())
        project.segments[1].firstFrames.images.append(custom)
        project.segments[1].firstFrames.confirmedImageID = custom.id
        project.segments[1].userSelectedFirstFrameID = custom.id
        project.segments[1].inheritedFirstFrameSourceSegmentID = nil
        project.segments[0].actualVideoLastFrameID = firstTail.id
        _ = StoryContinuityContext.reconcileInheritedFirstFrames(&project)
        XCTAssertEqual(project.segments[1].firstFrame?.id, custom.id)
    }

    func testApplyingActualVideoTailIsIdempotentAndReplacesAIPregeneratedNextFirstFrame() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryActualTailTests-\(UUID())")
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        var project = makeProject(); project.source = "测试"
        let guide = try await store.saveImage(Data("guide".utf8), mimeType: "image/png",
                                              projectID: project.id, owner: "alice")
        let generatedFirst = try await store.saveImage(
            Data("generated-first".utf8), mimeType: "image/png", projectID: project.id, owner: "alice",
            sourceResourceID: "s2", generationAttemptID: UUID(), providerResultID: "image-job"
        )
        let video = try await store.saveVideo(
            .init(id: "video-job", modelConfigID: "video", modelName: "video-model", createdAt: "",
                  mimeType: "video/mp4", videoData: Data("video".utf8)),
            projectID: project.id, owner: "alice"
        )
        var first = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        first.lastFrames.images = [guide]; first.confirmedLastFrameID = guide.id; first.video = video
        var second = StorySegment(id: "s2", title: "B", synopsis: "B", sourceRange: .init(start: 1, end: 2))
        second.firstFrames.images = [generatedFirst]; second.confirmedFrameID = generatedFirst.id
        project.segments = [first, second]
        try await store.save(project, owner: "alice")

        let firstApply = try await store.applyActualVideoLastFrame(
            Data("actual-frame".utf8), projectID: project.id, segmentID: "s1",
            videoJobID: video.jobID, owner: "alice"
        )
        let actual = firstApply.1
        XCTAssertEqual(firstApply.0.segments[0].confirmedLastFrameID, guide.id)
        XCTAssertEqual(firstApply.0.segments[0].actualVideoLastFrameID, actual.id)
        XCTAssertEqual(firstApply.0.segments[1].confirmedFrameID, actual.id)
        XCTAssertEqual(firstApply.0.segments[1].inheritedFirstFrameSourceSegmentID, "s1")

        let secondApply = try await store.applyActualVideoLastFrame(
            Data("ignored-duplicate".utf8), projectID: project.id, segmentID: "s1",
            videoJobID: video.jobID, owner: "alice"
        )
        XCTAssertEqual(secondApply.1.id, actual.id)
        XCTAssertEqual(secondApply.0.segments[0].lastFrames.images.filter {
            $0.derivedFromVideoJobID == video.jobID
        }.count, 1)
    }

    func testLegacySegmentWithoutActualVideoTailMetadataStillDecodes() throws {
        let segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        var object = try XCTUnwrap(JSONSerialization.jsonObject(
            with: JSONEncoder().encode(segment)
        ) as? [String: Any])
        object.removeValue(forKey: "actualVideoLastFrameID")
        let decoded = try JSONDecoder().decode(
            StorySegment.self, from: JSONSerialization.data(withJSONObject: object)
        )
        XCTAssertNil(decoded.actualVideoLastFrameID)
    }

}
