import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSApp

extension StoryStudioTests {
    func testProjectLoadActivatesStagedStoryJobAndCreatesOnce() async throws {
        let fixture = try await durableStoryRecoveryFixture()
        let vm = StoryStudioViewModel(
            media: fixture.service,
            planner: fixture.service,
            store: fixture.store,
            videoJobStore: fixture.jobStore
        )

        vm.activate(userID: "alice")
        try await idle(vm)

        let segment = try XCTUnwrap(vm.projects.first?.segments.first(where: { $0.id == "s1" }))
        XCTAssertEqual(segment.video?.jobID, "remote-job")
        XCTAssertEqual(segment.videoJobRecordID, fixture.attempt.id)
        let snapshot = try await fixture.jobStore.snapshot(
            id: fixture.attempt.id,
            owner: "alice"
        )
        XCTAssertTrue(snapshot.isActive)
        XCTAssertEqual(snapshot.status, .completed)
        let events = await fixture.service.events()
        XCTAssertEqual(events.filter { $0 == "video" }.count, 1)
        XCTAssertFalse(events.contains("resume"))
    }

    func testProjectLoadResumesActiveProviderJobWithoutCreatingAgain() async throws {
        let fixture = try await durableStoryRecoveryFixture(
            providerJobID: "existing-provider-job"
        )
        _ = try await fixture.jobStore.activate(
            id: fixture.attempt.id,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: fixture.workload
        )
        let vm = StoryStudioViewModel(
            media: fixture.service,
            planner: fixture.service,
            store: fixture.store,
            videoJobStore: fixture.jobStore
        )

        vm.activate(userID: "alice")
        try await idle(vm)

        let segment = try XCTUnwrap(vm.projects.first?.segments.first(where: { $0.id == "s1" }))
        XCTAssertEqual(segment.video?.jobID, "existing-provider-job")
        let events = await fixture.service.events()
        XCTAssertEqual(events.filter { $0 == "video" }.count, 0)
        XCTAssertEqual(events.filter { $0 == "resume" }.count, 1)
    }

    func testProjectLoadNeverRecreatesUncertainSubmission() async throws {
        let fixture = try await durableStoryRecoveryFixture()
        let oldSession = UUID()
        _ = try await fixture.jobStore.activate(
            id: fixture.attempt.id,
            owner: "alice",
            sessionEpoch: oldSession,
            workload: fixture.workload
        )
        _ = try await fixture.jobStore.markSubmitting(
            id: fixture.attempt.id,
            owner: "alice",
            sessionEpoch: oldSession
        )
        let vm = StoryStudioViewModel(
            media: fixture.service,
            planner: fixture.service,
            store: fixture.store,
            videoJobStore: fixture.jobStore
        )

        vm.activate(userID: "alice")
        try await idle(vm)

        let snapshot = try await fixture.jobStore.snapshot(
            id: fixture.attempt.id,
            owner: "alice"
        )
        XCTAssertEqual(snapshot.status, .submissionUncertain)
        XCTAssertNil(snapshot.providerJobID)
        let segment = try XCTUnwrap(vm.projects.first?.segments.first(where: { $0.id == "s1" }))
        XCTAssertEqual(segment.videoJobRecordID, fixture.attempt.id)
        XCTAssertNotNil(segment.error)
        XCTAssertNotNil(vm.errorMessage)
        let events = await fixture.service.events()
        XCTAssertFalse(events.contains("video"))
        XCTAssertFalse(events.contains("resume"))
    }

    func testProjectLoadReconcilesStorySaveBeforeJobCompletionWithoutProviderCall() async throws {
        let fixture = try await durableStoryRecoveryFixture()
        let oldSession = UUID()
        _ = try await fixture.jobStore.activate(
            id: fixture.attempt.id,
            owner: "alice",
            sessionEpoch: oldSession,
            workload: fixture.workload
        )
        _ = try await fixture.jobStore.markSubmitting(
            id: fixture.attempt.id,
            owner: "alice",
            sessionEpoch: oldSession
        )
        try await fixture.jobStore.recordProgress(
            id: fixture.attempt.id,
            owner: "alice",
            sessionEpoch: oldSession,
            progress: .init(status: "completed", jobID: "saved-provider-job")
        )
        try await fixture.jobStore.markFinalizing(
            id: fixture.attempt.id,
            owner: "alice",
            sessionEpoch: oldSession,
            providerJobID: "saved-provider-job"
        )
        _ = try await fixture.store.completeVideoGenerationFromMediaJob(
            .init(
                id: "saved-provider-job",
                modelConfigID: fixture.attempt.modelConfigID,
                modelName: "MiniMax-H3",
                createdAt: "",
                mimeType: "video/mp4",
                videoData: Data("already-saved".utf8)
            ),
            projectID: fixture.project.id,
            segmentID: "s1",
            jobRecordID: fixture.attempt.id,
            owner: "alice"
        )
        let vm = StoryStudioViewModel(
            media: fixture.service,
            planner: fixture.service,
            store: fixture.store,
            videoJobStore: fixture.jobStore
        )

        vm.activate(userID: "alice")
        try await idle(vm)

        let snapshot = try await fixture.jobStore.snapshot(
            id: fixture.attempt.id,
            owner: "alice"
        )
        XCTAssertEqual(snapshot.status, .completed)
        let segment = try XCTUnwrap(vm.projects.first?.segments.first(where: { $0.id == "s1" }))
        XCTAssertEqual(segment.video?.jobID, "saved-provider-job")
        let events = await fixture.service.events()
        XCTAssertFalse(events.contains("video"))
        XCTAssertFalse(events.contains("resume"))
    }

    func testRecoveredStoryJobCannotPublishAfterAccountSwitch() async throws {
        let fixture = try await durableStoryRecoveryFixture()
        await fixture.service.setDelay(true)
        let vm = StoryStudioViewModel(
            media: fixture.service,
            planner: fixture.service,
            store: fixture.store,
            videoJobStore: fixture.jobStore
        )
        vm.activate(userID: "alice")
        for _ in 0..<100 {
            let events = await fixture.service.events()
            if events.contains("video") { break }
            try await Task.sleep(for: .milliseconds(5))
        }
        let startedEvents = await fixture.service.events()
        XCTAssertTrue(startedEvents.contains("video"))

        vm.activate(userID: "bob")
        try await idle(vm)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(vm.owner, "bob")
        XCTAssertTrue(vm.projects.isEmpty)
        XCTAssertTrue(vm.activeVideoGenerations.isEmpty)
    }

    private func durableStoryRecoveryFixture(
        providerJobID: String? = nil
    ) async throws -> DurableStoryRecoveryFixture {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryDurableRecoveryTests-\(UUID().uuidString)",
            isDirectory: true
        )
        addTeardownBlock {
            if FileManager.default.fileExists(atPath: root.path) {
                try? FileManager.default.removeItem(at: root)
            }
        }
        let store = StoryProjectStore(root: root)
        let jobStore = MediaStudioVideoJobStore(
            root: root.appendingPathComponent("MediaJobs", isDirectory: true)
        )
        let service = StoryTestService()
        let project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        let attempt = StoryVideoAttempt(
            modelConfigID: project.models.videoModelID,
            prompt: "recover durable Story video",
            size: "768P",
            ratio: project.ratio,
            seconds: project.segments[0].seconds
        )
        _ = try await store.beginVideoGeneration(
            projectID: project.id,
            segmentID: "s1",
            attempt: attempt,
            owner: "alice"
        )
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: project.id,
            segmentID: "s1",
            attemptID: attempt.id
        )
        let request = VideoGenerationRequest(
            modelConfigID: attempt.modelConfigID,
            prompt: attempt.prompt,
            size: attempt.size,
            seconds: attempt.seconds,
            ratio: attempt.ratio
        )
        if let providerJobID {
            _ = try await jobStore.stageImportedProviderJob(
                request: request,
                id: attempt.id,
                providerJobID: providerJobID,
                owner: "alice",
                sessionEpoch: UUID(),
                workload: workload
            )
        } else {
            _ = try await jobStore.stagePrepared(
                request: request,
                id: attempt.id,
                owner: "alice",
                sessionEpoch: UUID(),
                workload: workload
            )
        }
        let handedOff = try await store.handoffVideoGenerationToMediaJob(
            projectID: project.id,
            segmentID: "s1",
            expectedAttempt: attempt,
            owner: "alice"
        )
        return .init(
            store: store,
            jobStore: jobStore,
            service: service,
            project: handedOff,
            attempt: attempt,
            workload: workload
        )
    }

    func testStoryVideoStartTransfersToDurableJobBeforeProviderSubmission() async throws {
        let (vm, store, service) = try await fixture()
        let draft = try await readyProject(store)
        let created = await vm.create(draft, availableModels: models)
        XCTAssertTrue(created)

        vm.selectedSegments = ["s1"]
        vm.generateBatch(availableModels: models)
        try await idle(vm)

        let segment = try XCTUnwrap(vm.project?.segments.first(where: { $0.id == "s1" }))
        let recordID = try XCTUnwrap(segment.videoJobRecordID)
        XCTAssertNil(segment.attempt)
        XCTAssertEqual(segment.video?.jobID, "remote-job")
        XCTAssertNil(segment.error)
        let jobStore = MediaStudioVideoJobStore(
            root: store.root.appendingPathComponent("MediaJobs", isDirectory: true)
        )
        let snapshot = try await jobStore.snapshot(id: recordID, owner: "alice")
        XCTAssertTrue(snapshot.isActive)
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, "remote-job")
        XCTAssertEqual(
            snapshot.workload,
            .storyVideo(projectID: draft.id, segmentID: "s1", attemptID: recordID)
        )
        let events = await service.events()
        XCTAssertEqual(events.filter { $0 == "video" }.count, 1)
        XCTAssertFalse(events.contains("resume"))
    }

    func testLegacyStoryResumeImportsProviderJobWithoutCreatingAgain() async throws {
        let (vm, store, service) = try await fixture()
        var draft = try await readyProject(store)
        var attempt = StoryVideoAttempt(
            modelConfigID: draft.models.videoModelID,
            prompt: "legacy prompt",
            size: "768P",
            ratio: draft.ratio,
            seconds: draft.segments[0].seconds
        )
        attempt.jobID = "legacy-provider-job"
        attempt.status = "failed"
        draft.segments[0].attempt = attempt
        let created = await vm.create(draft, availableModels: models)
        XCTAssertTrue(created)

        vm.resumeVideo("s1")
        try await idle(vm)

        let segment = try XCTUnwrap(vm.project?.segments.first(where: { $0.id == "s1" }))
        XCTAssertEqual(segment.videoJobRecordID, attempt.id)
        XCTAssertNil(segment.attempt)
        XCTAssertEqual(segment.video?.jobID, "legacy-provider-job")
        let events = await service.events()
        XCTAssertEqual(events.filter { $0 == "video" }.count, 0)
        XCTAssertEqual(events.filter { $0 == "resume" }.count, 1)
        let jobStore = MediaStudioVideoJobStore(
            root: store.root.appendingPathComponent("MediaJobs", isDirectory: true)
        )
        let snapshot = try await jobStore.snapshot(id: attempt.id, owner: "alice")
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, "legacy-provider-job")
    }

    func testStoryMediaJobFinalizationReplayDoesNotDuplicateVideoFile() async throws {
        let (_, store, _) = try await fixture()
        var project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        let attempt = StoryVideoAttempt(
            modelConfigID: project.models.videoModelID,
            prompt: "durable finalization",
            size: "768P",
            ratio: project.ratio,
            seconds: project.segments[0].seconds
        )
        _ = try await store.beginVideoGeneration(
            projectID: project.id,
            segmentID: "s1",
            attempt: attempt,
            owner: "alice"
        )
        project = try await store.handoffVideoGenerationToMediaJob(
            projectID: project.id,
            segmentID: "s1",
            expectedAttempt: attempt,
            owner: "alice"
        )
        let result = VideoGenerationResult(
            id: "same-provider-result",
            modelConfigID: project.models.videoModelID,
            modelName: "MiniMax-H3",
            createdAt: "",
            mimeType: "video/mp4",
            videoData: Data("one-video".utf8)
        )

        let first = try await store.completeVideoGenerationFromMediaJob(
            result,
            projectID: project.id,
            segmentID: "s1",
            jobRecordID: attempt.id,
            owner: "alice"
        )
        let replayed = try await store.completeVideoGenerationFromMediaJob(
            result,
            projectID: project.id,
            segmentID: "s1",
            jobRecordID: attempt.id,
            owner: "alice"
        )

        XCTAssertEqual(replayed.1, first.1)
        XCTAssertEqual(replayed.0.updatedAt, first.0.updatedAt)
        let videoURL = try store.fileURL(first.1.filename, projectID: project.id, owner: "alice")
        let videoFiles = try FileManager.default.contentsOfDirectory(
            at: videoURL.deletingLastPathComponent(),
            includingPropertiesForKeys: nil
        ).filter { $0.pathExtension == "mp4" }
        XCTAssertEqual(videoFiles.map(\.lastPathComponent), [first.1.filename])
    }

    func testVideoJobHandoffAtomicallyReplacesLegacyAttemptAndPreservesOwnership() async throws {
        let (_, store, _) = try await fixture()
        var project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        let segmentID = try XCTUnwrap(project.segments.first?.id)
        let attempt = StoryVideoAttempt(
            modelConfigID: project.models.videoModelID,
            prompt: "handoff prompt",
            size: "768P",
            ratio: project.ratio,
            seconds: project.segments[0].seconds
        )
        let intent = try await store.beginVideoGeneration(
            projectID: project.id,
            segmentID: segmentID,
            attempt: attempt,
            owner: "alice"
        )

        let handedOff = try await store.handoffVideoGenerationToMediaJob(
            projectID: project.id,
            segmentID: segmentID,
            expectedAttempt: attempt,
            owner: "alice"
        )
        XCTAssertEqual(handedOff.version, 3)
        XCTAssertEqual(handedOff.segments[0].videoJobRecordID, attempt.id)
        XCTAssertNil(handedOff.segments[0].attempt)
        XCTAssertTrue(handedOff.segments[0].hasVideoJob)
        XCTAssertTrue(handedOff.hasUnresolvedVideoJobs)
        XCTAssertFalse(handedOff.segments[0].isReady)

        let repeated = try await store.handoffVideoGenerationToMediaJob(
            projectID: project.id,
            segmentID: segmentID,
            expectedAttempt: attempt,
            owner: "alice"
        )
        XCTAssertEqual(repeated.updatedAt, handedOff.updatedAt)

        var staleEdit = intent.project
        staleEdit.title = "A concurrent title edit"
        let merged = try await store.save(
            staleEdit,
            owner: "alice",
            preservingVideoSegmentIDs: []
        )
        XCTAssertEqual(merged.version, 3)
        XCTAssertEqual(merged.segments[0].videoJobRecordID, attempt.id)
        XCTAssertNil(merged.segments[0].attempt)

        do {
            _ = try await store.updateVideoGenerationProgress(
                .init(status: "queued", jobID: "legacy-writer-must-stop"),
                projectID: project.id,
                segmentID: segmentID,
                attemptID: attempt.id,
                owner: "alice"
            )
            XCTFail("The legacy writer must be fenced after the project handoff")
        } catch {
            XCTAssertEqual(error.localizedDescription, StoryError.unresolvedSubmission.localizedDescription)
        }

        project = merged
        project.version = 2
        XCTAssertThrowsError(try project.validate())
    }

    func testRepeatedVideoProgressDoesNotRewriteTheStoryManifest() async throws {
        let (_, store, _) = try await fixture()
        var project = try await readyProject(store)
        try await store.save(project, owner: "alice")
        let attempt = StoryVideoAttempt(
            modelConfigID: project.models.videoModelID,
            prompt: "prompt",
            size: "1920x1080",
            ratio: project.ratio,
            seconds: project.segments[0].seconds
        )
        let intent = try await store.beginVideoGeneration(
            projectID: project.id,
            segmentID: project.segments[0].id,
            attempt: attempt,
            owner: "alice"
        )
        project = intent.project
        let first = try await store.updateVideoGenerationProgress(
            .init(status: "queued", jobID: "job-1"),
            projectID: project.id,
            segmentID: project.segments[0].id,
            attemptID: attempt.id,
            owner: "alice"
        )
        let repeated = try await store.updateVideoGenerationProgress(
            .init(status: "queued", jobID: "job-1"),
            projectID: project.id,
            segmentID: project.segments[0].id,
            attemptID: attempt.id,
            owner: "alice"
        )

        XCTAssertEqual(repeated.updatedAt, first.updatedAt)
        XCTAssertEqual(repeated.segments[0].attempt?.jobID, "job-1")
        XCTAssertEqual(repeated.segments[0].attempt?.status, "queued")
    }

    func testConfirmLatestFramesMakesGeneratedSegmentReadyWithoutAnotherProviderCall() async throws {
        let (vm, store, service) = try await fixture()
        var project = makeProject()
        let first = try await store.saveImage(Data("first".utf8), mimeType: "image/png",
                                              projectID: project.id, owner: "alice")
        let last = try await store.saveImage(Data("last".utf8), mimeType: "image/png",
                                             projectID: project.id, owner: "alice")
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A",
                                   sourceRange: .init(start: 0, end: 1))
        segment.detail = makeDetail()
        segment.firstFrames.images = [first]
        segment.lastFrames.images = [last]
        project.segments = [segment]
        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created)

        vm.confirmLatestFrames("s1", useConfirmedLastFrameForVideo: true)
        try await idle(vm)

        let saved = try XCTUnwrap(vm.project?.segments.first)
        XCTAssertEqual(saved.confirmedFrameID, first.id)
        XCTAssertEqual(saved.confirmedLastFrameID, last.id)
        XCTAssertTrue(saved.useLastFrameForVideo)
        XCTAssertTrue(saved.isReady)
        let events = await service.events()
        XCTAssertTrue(events.isEmpty, "Confirming local versions must not call a provider")
    }

    func testConfirmedLastFrameCanBeRemovedWithoutDeletingItsImage() async throws {
        let (vm, store, service) = try await fixture()
        var project = makeProject()
        let first = try await store.saveImage(Data("first".utf8), mimeType: "image/png",
                                              projectID: project.id, owner: "alice")
        let last = try await store.saveImage(Data("last".utf8), mimeType: "image/png",
                                             projectID: project.id, owner: "alice")
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A",
                                   sourceRange: .init(start: 0, end: 1))
        segment.detail = makeDetail()
        segment.firstFrames.images = [first]
        segment.confirmedFrameID = first.id
        segment.lastFrames.images = [last]
        segment.confirmedLastFrameID = last.id
        segment.useLastFrameForVideo = true
        project.segments = [segment]
        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created)

        vm.clearConfirmedLastFrame("s1")
        try await idle(vm)

        let saved = try XCTUnwrap(vm.project?.segments.first)
        XCTAssertNil(saved.confirmedLastFrameID)
        XCTAssertNil(saved.lastFrame)
        XCTAssertFalse(saved.useLastFrameForVideo)
        XCTAssertEqual(saved.lastFrames.images.map(\.id), [last.id])
        XCTAssertNotNil(saved.firstFrame)
        let events = await service.events()
        XCTAssertTrue(events.isEmpty)
    }

    func testAmbiguousImageFailurePersistsIntentAndBlocksResubmissionUntilVerified() async throws {
        let (vm, _, service) = try await fixture()
        var project = makeProject()
        project.props = [.init(id: "key", name: "钥匙", description: "一把旧铜钥匙")]
        _ = await vm.create(project, availableModels: models)
        vm.generateAsset("key"); try await idle(vm)
        XCTAssertNotNil(vm.project?.props[0].media.generationAttemptID)
        var events = await service.events()
        XCTAssertEqual(events.filter { $0 == "image" }.count, 1)
        var settings = try XCTUnwrap(vm.project)
        settings.models.imageModelID = "text"
        let changed = await vm.updateSettings(settings, availableModels: models)
        XCTAssertFalse(changed)
        XCTAssertEqual(vm.project?.models.imageModelID, "image")
        vm.generateAsset("key")
        events = await service.events()
        XCTAssertEqual(events.filter { $0 == "image" }.count, 1)
        vm.allowImageRetryAfterVerification(assetID: "key", segmentID: nil); try await idle(vm)
        XCTAssertNil(vm.project?.props[0].media.generationAttemptID)
    }

    func testConcurrentAssetGenerationsUseStableIDsAndMergeReverseCompletions() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryConcurrentTests-\(UUID().uuidString)", isDirectory: true)
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 16, pixelsHigh: 16,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let service = ConcurrentStoryImageService(imageData: png)
        let vm = StoryStudioViewModel(
            media: service,
            store: store,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )
        vm.activate(userID: "alice"); try await idle(vm)
        var project = makeProject()
        project.props = [
            .init(id: "key", name: "钥匙", description: "一把旧铜钥匙"),
            .init(id: "map", name: "地图", description: "一张手绘地图"),
        ]
        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created)

        vm.generateAsset("key")
        vm.generateAsset("key")
        vm.generateAsset("map")
        for _ in 0..<300 where await service.maximumConcurrentCalls() < 2 {
            try await Task.sleep(for: .milliseconds(10))
        }
        let maximumConcurrentCalls = await service.maximumConcurrentCalls()
        XCTAssertEqual(maximumConcurrentCalls, 2)
        let keyCallCount = await service.callCount(resourceID: "key")
        XCTAssertEqual(keyCallCount, 1)
        XCTAssertTrue(vm.isGeneratingAsset("key", projectID: project.id))
        XCTAssertTrue(vm.isGeneratingAsset("map", projectID: project.id))
        await service.finish(resourceID: "map")
        await service.finish(resourceID: "key")
        try await idle(vm)

        let snapshot = try await store.load(owner: "alice")
        let saved = try XCTUnwrap(snapshot.projects.first)
        let keyImage = try XCTUnwrap(saved.resource(id: "key")?.images.first)
        let mapImage = try XCTUnwrap(saved.resource(id: "map")?.images.first)
        XCTAssertEqual(keyImage.sourceResourceID, "key")
        XCTAssertEqual(keyImage.providerResultID, "result-key")
        XCTAssertEqual(keyImage.providerAssetID, "asset-key")
        XCTAssertEqual(mapImage.sourceResourceID, "map")
        XCTAssertEqual(mapImage.providerResultID, "result-map")
        XCTAssertEqual(saved.resource(id: "key")?.confirmedImageID, keyImage.id)
        XCTAssertEqual(saved.resource(id: "map")?.confirmedImageID, mapImage.id)
        XCTAssertEqual(mapImage.providerAssetID, "asset-map")
        XCTAssertNotEqual(keyImage.generationAttemptID, mapImage.generationAttemptID)
        XCTAssertNil(saved.resource(id: "key")?.imageGenerationAttemptID)
        XCTAssertNil(saved.resource(id: "map")?.imageGenerationAttemptID)
    }

    func testFirstFrameGenerationAutomaticallyIncludesPreviousConfirmedTail() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryContinuityFrameTests-\(UUID().uuidString)", isDirectory: true)
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 16, pixelsHigh: 16,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let service = RecordingFrameImageService(imageData: png)
        let vm = StoryStudioViewModel(
            media: service,
            store: store,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )
        vm.activate(userID: "alice"); try await idle(vm)

        var project = makeProject(); project.source = "测试"
        let heroImage = try await store.saveImage(png, mimeType: "image/png", projectID: project.id, owner: "alice")
        let tailImage = try await store.saveImage(png, mimeType: "image/png", projectID: project.id, owner: "alice")
        var hero = StoryCharacter(id: "hero", name: "主角", profile: characterProfile())
        hero.media.images = [heroImage]; hero.media.confirmedImageID = heroImage.id
        project.characters = [hero]
        project.scenes = [.init(id: "room", name: "室内", profile: sceneProfile())]
        var previous = StorySegment(id: "s1", title: "上一段", synopsis: "主角走向窗边",
                                    sourceRange: .init(start: 0, end: 1), characterIDs: ["hero"], sceneIDs: ["room"])
        previous.detail = makeDetail(); previous.lastFrames.images = [tailImage]; previous.confirmedLastFrameID = tailImage.id
        var current = StorySegment(id: "s2", title: "当前段", synopsis: "主角继续行动",
                                   sourceRange: .init(start: 1, end: 2), characterIDs: ["hero"], sceneIDs: ["room"])
        current.detail = makeDetail()
        project.segments = [previous, current]
        project.relations = [
            .init(id: "r1", segmentID: "s1", characterID: "hero", sceneID: "room", action: "走向窗边", position: "东侧窗边"),
            .init(id: "r2", segmentID: "s2", characterID: "hero", sceneID: "room", action: "继续行动", position: "东侧窗边"),
        ]
        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created)

        vm.generateFrame("s2", role: .first, referenceAssetIDs: ["hero"])
        try await idle(vm)
        let capturedRequest = await service.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(request.referenceImages.map(\.name), ["主角.png", "previous-segment-last-frame.png"])
        XCTAssertTrue(request.referenceImages.allSatisfy(\.base64Data.isEmpty))
        XCTAssertTrue(request.referenceImages.allSatisfy { $0.fileURL?.isFileURL == true })
        for reference in request.referenceImages {
            XCTAssertEqual(try Data(contentsOf: XCTUnwrap(reference.fileURL)), png)
        }
        XCTAssertTrue(request.prompt.contains("previousSegmentTailReferenceIndex\":2"))
        XCTAssertNotNil(vm.project?.segments[1].firstFrame)

        vm.generateFrame("s2", role: .last, referenceAssetIDs: ["hero"])
        try await idle(vm)
        let capturedLastRequest = await service.lastRequest()
        let lastRequest = try XCTUnwrap(capturedLastRequest)
        XCTAssertEqual(lastRequest.referenceImages.map(\.name), ["主角.png", "current-segment-first-frame.png"])
        XCTAssertTrue(lastRequest.referenceImages.allSatisfy(\.base64Data.isEmpty))
        XCTAssertTrue(lastRequest.referenceImages.allSatisfy { $0.fileURL?.isFileURL == true })
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(lastRequest.referenceImages[0].fileURL)), png)
        let currentFirstFrame = try XCTUnwrap(vm.project?.segments[1].firstFrame)
        let currentFirstFrameURL = try store.fileURL(
            currentFirstFrame.filename,
            projectID: project.id,
            owner: "alice"
        )
        XCTAssertEqual(
            try Data(contentsOf: XCTUnwrap(lastRequest.referenceImages[1].fileURL)),
            try Data(contentsOf: currentFirstFrameURL)
        )
        XCTAssertTrue(lastRequest.prompt.contains("currentSegmentFirstFrameReferenceIndex\":2"))
        XCTAssertTrue(lastRequest.prompt.contains("本段已确认首帧"))
        XCTAssertNotNil(vm.project?.segments[1].lastFrame)
    }

    func testFrameGenerationRemainsObservableAfterLeavingAndReopeningProject() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryFrameLoadingTests-\(UUID().uuidString)", isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        addTeardownBlock {
            if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) }
        }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 16, pixelsHigh: 16,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let service = ConcurrentStoryImageService(imageData: png)
        let vm = StoryStudioViewModel(
            media: service,
            store: store,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )
        vm.activate(userID: "alice")
        try await idle(vm)

        var project = makeProject()
        let heroImage = try await store.saveImage(
            png, mimeType: "image/png", projectID: project.id, owner: "alice"
        )
        var hero = StoryCharacter(id: "hero", name: "主角", profile: characterProfile())
        hero.media.images = [heroImage]
        hero.media.confirmedImageID = heroImage.id
        project.characters = [hero]
        var segment = StorySegment(id: "s1", title: "开场", synopsis: "主角入场",
                                   sourceRange: .init(start: 0, end: 1), characterIDs: ["hero"])
        segment.detail = makeDetail()
        project.segments = [segment]
        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created)

        vm.generateFrame("s1", role: .first, referenceAssetIDs: ["hero"])
        XCTAssertFalse(vm.isBusy, "Frame generation should not hide behind the global busy state")
        XCTAssertTrue(vm.isGeneratingFrame("s1", role: .first, projectID: project.id))
        XCTAssertFalse(vm.isGeneratingFrame("s1", role: .last, projectID: project.id))

        vm.backToList()
        vm.open(project.id)
        XCTAssertTrue(vm.isGeneratingFrame("s1", role: .first, projectID: project.id),
                      "Closing and reopening the project must retain its loading state")

        for _ in 0..<100 {
            if await service.callCount(resourceID: "s1:first") > 0 { break }
            try await Task.sleep(for: .milliseconds(5))
        }
        let frameCalls = await service.callCount(resourceID: "s1:first")
        XCTAssertEqual(frameCalls, 1)
        await service.finish(resourceID: "s1:first")
        try await idle(vm)
        XCTAssertFalse(vm.isGeneratingFrame("s1", role: .first, projectID: project.id))
        XCTAssertNotNil(vm.project?.segments.first?.firstFrame)
    }

    func testPendingAssetGenerationDoesNotRetainStoryStudioViewModel() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryAssetLifetimeTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        addTeardownBlock {
            if FileManager.default.fileExists(atPath: root.path) {
                try FileManager.default.removeItem(at: root)
            }
        }
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
        let service = ConcurrentStoryImageService(imageData: png)
        var viewModel: StoryStudioViewModel? = StoryStudioViewModel(media: service, store: store)
        weak let weakViewModel = viewModel
        viewModel?.activate(userID: "alice")
        if let viewModel { try await idle(viewModel) }
        var project = makeProject()
        project.props = [.init(id: "key", name: "钥匙", description: "一把旧铜钥匙")]
        let created = await viewModel?.create(project, availableModels: models)
        XCTAssertTrue(created == true)

        viewModel?.generateAsset("key")
        for _ in 0..<100 where await service.callCount(resourceID: "key") == 0 {
            try await Task.sleep(for: .milliseconds(5))
        }
        let callCount = await service.callCount(resourceID: "key")
        XCTAssertEqual(callCount, 1)

        viewModel = nil
        for _ in 0..<20 where weakViewModel != nil { await Task.yield() }
        XCTAssertNil(weakViewModel)
        await service.finish(resourceID: "key")
    }

    func testLateAssetGenerationCannotPublishIntoAnotherAccount() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "StoryAssetAccountIsolationTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let store = StoryProjectStore(root: root)
        addTeardownBlock {
            if FileManager.default.fileExists(atPath: root.path) {
                try FileManager.default.removeItem(at: root)
            }
        }
        var project = makeProject()
        project.props = [.init(id: "key", name: "钥匙", description: "一把旧铜钥匙")]
        let generator = DeferredStoryAssetImageGenerator(project: project)
        let service = StoryTestService()
        let vm = StoryStudioViewModel(
            media: service,
            planner: service,
            store: store,
            storyAssetImageGeneration: generator
        )
        vm.activate(userID: "alice")
        try await idle(vm)
        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created)

        vm.generateAsset("key")
        for _ in 0..<100 where await !generator.hasRequest {
            try await Task.sleep(for: .milliseconds(5))
        }
        let hasRequest = await generator.hasRequest
        XCTAssertTrue(hasRequest)

        vm.activate(userID: "bob")
        try await idle(vm)
        await generator.complete()
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(vm.owner, "bob")
        XCTAssertTrue(vm.projects.isEmpty)
        XCTAssertTrue(vm.activeAssetGenerations.isEmpty)
        XCTAssertTrue(vm.assetGenerationErrors.isEmpty)
    }

    func testStoreRejectsCompletionForAnotherAttemptID() async throws {
        let (_, store, _) = try await fixture()
        var project = makeProject()
        project.props = [.init(id: "key", name: "钥匙", description: "一把旧铜钥匙")]
        try await store.save(project, owner: "alice")
        let attempt = UUID()
        _ = try await store.beginAssetImageGeneration(projectID: project.id, resourceID: "key", attemptID: attempt, owner: "alice")
        do {
            _ = try await store.completeAssetImageGeneration(Data("image".utf8), mimeType: "image/png",
                projectID: project.id, resourceID: "key", attemptID: UUID(),
                providerResultID: "wrong", providerAssetID: "wrong", owner: "alice")
            XCTFail("A completion must not be assigned to a different attempt")
        } catch {
            let snapshot = try await store.load(owner: "alice")
            XCTAssertEqual(snapshot.projects.first?.resource(id: "key")?.images.count, 0)
        }
    }

    func testPlanningToolSchemasDoNotOfferBillableGenerationTools() throws {
        var project = makeProject(); project.source = "Story with untrusted instructions to submit videos."
        let outline = try StoryPlanningTools.outlineRequest(project)
        XCTAssertEqual(outline.toolName, "story_save_outline")
        XCTAssertTrue(outline.context.contains(project.source))
        XCTAssertFalse(String(decoding: outline.schema, as: UTF8.self).contains("submit_batch"))
        let optimization = try StoryPlanningTools.optimizationRequest(project, source: project.source,
                                                                       style: project.style, target: .source)
        XCTAssertEqual(optimization.toolName, "story_suggest_optimized_text")
        XCTAssertFalse(String(decoding: optimization.schema, as: UTF8.self).contains("generate"))
        var previous = StorySegment(id: "s0", title: "Previous", synopsis: "Previous",
                                    sourceRange: .init(start: 0, end: 1))
        previous.detail = .init(firstFramePrompt: "previous first",
                                shots: [.init(start: 0, end: 15, prompt: "previous final movement")],
                                continuityIn: "previous in", continuityOut: "previous concrete exit",
                                audio: "room tone", constraints: "same axis", lastFramePrompt: "previous exact tail")
        project.segments = [previous, .init(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 1, end: 2))]
        let detail = try StoryPlanningTools.detailRequest(project, segmentID: "s1")
        XCTAssertEqual(detail.toolName, "story_update_segment")
        XCTAssertTrue(detail.context.contains("previous exact tail"))
        XCTAssertTrue(detail.context.contains("previous final movement"))
        XCTAssertTrue(detail.context.contains("previous concrete exit"))
        XCTAssertThrowsError(try StoryPlanningTools.detailRequest(project, segmentID: "another-project-id"))
    }

}

private struct DurableStoryRecoveryFixture {
    let store: StoryProjectStore
    let jobStore: MediaStudioVideoJobStore
    let service: StoryTestService
    let project: StoryProject
    let attempt: StoryVideoAttempt
    let workload: MediaStudioVideoJobStore.Workload
}

private actor DeferredStoryAssetImageGenerator: StoryAssetImageGenerating {
    private let project: StoryProject
    private var command: StoryAssetImageGenerationCommand?
    private var continuation: CheckedContinuation<StoryAssetImageGenerationOutcome, Error>?

    init(project: StoryProject) {
        self.project = project
    }

    var hasRequest: Bool { command != nil }

    func generateStoryAssetImage(
        ownerUserID: String,
        command: StoryAssetImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryAssetImageGenerationEvent) async -> Void
    ) async throws -> StoryAssetImageGenerationOutcome {
        self.command = command
        return try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
        }
    }

    func complete() {
        guard let command, let continuation else { return }
        var completed = project
        completed.props[0].media.images = [
            StoryImage(
                filename: "key.png",
                mimeType: "image/png",
                sourceResourceID: command.resourceID,
                generationAttemptID: command.attemptID
            ),
        ]
        completed.props[0].media.generationAttemptID = nil
        self.continuation = nil
        continuation.resume(returning: .init(project: completed))
    }
}
