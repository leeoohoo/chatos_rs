import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class StoryStudioTests: XCTestCase {
    func testCreationOnlyPersistsProjectAndOpensWorkspace() async throws {
        let (vm, store, service) = try await fixture()
        let draft = makeProject()
        let created = await vm.create(draft, availableModels: models)
        XCTAssertTrue(created)
        XCTAssertEqual(vm.selectedProjectID, draft.id)
        XCTAssertEqual(vm.projects.first?.models.textModelID, draft.models.textModelID)
        XCTAssertEqual(vm.projects.first?.models.imageModelID, draft.models.imageModelID)
        XCTAssertEqual(vm.projects.first?.models.videoModelID, draft.models.videoModelID)
        XCTAssertEqual(vm.projects.first?.models.supportedVideoDurations, Array(4...15))
        XCTAssertTrue(vm.project?.segments.isEmpty == true)
        let calls = await service.events()
        XCTAssertTrue(calls.isEmpty, "Creating must not call text/image/video providers")
        let snapshot = try await store.load(owner: "alice")
        XCTAssertEqual(snapshot.projects.first?.title, "Test Story")
        vm.backToList()
        XCTAssertNil(vm.project)
        vm.open(draft.id)
        XCTAssertEqual(vm.project?.id, draft.id)
    }

    func testTaskDisabledModelsAreSelectableAndMissingModelsRejectCreation() async throws {
        let (vm, _, _) = try await fixture()
        XCTAssertFalse(models[0].taskEnabled)
        let first = await vm.create(makeProject(), availableModels: models)
        XCTAssertTrue(first)
        var draft = makeProject(); draft.models.videoModelID = "removed"
        let second = await vm.create(draft, availableModels: models)
        XCTAssertFalse(second)
        XCTAssertEqual(vm.projects.count, 1)
    }

    func testAccountsRemainIsolatedAndReopeningRestoresDraft() async throws {
        let (vm, _, _) = try await fixture()
        let draft = makeProject()
        _ = await vm.create(draft, availableModels: models)
        vm.activate(userID: "bob")
        try await idle(vm)
        XCTAssertTrue(vm.projects.isEmpty)
        XCTAssertNil(vm.selectedProjectID)
        vm.activate(userID: "alice")
        try await idle(vm)
        XCTAssertEqual(vm.projects.map(\.id), [draft.id])
        XCTAssertNil(vm.selectedProjectID, "Entering the tab should start at the list")
    }

    func testSettingsPreserveSourcePlansAndChangeOnlyProjectModels() async throws {
        let (vm, _, _) = try await fixture()
        var draft = makeProject(); draft.source = "Full original story"
        draft.segments = [.init(id: "one", title: "Opening", synopsis: "Opening", sourceRange: .init(start: 0, end: 1))]
        _ = await vm.create(draft, availableModels: models)
        var settings = draft
        settings.title = "Updated"; settings.models.textModelID = "video"
        settings.source = "stale form must not overwrite actual story"
        let updated = await vm.updateSettings(settings, availableModels: models)
        XCTAssertTrue(updated)
        XCTAssertEqual(vm.project?.source, draft.source)
        XCTAssertEqual(vm.project?.segments, draft.segments)
        XCTAssertEqual(vm.project?.models.textModelID, "video")
    }

    func testOutlineIsSeparateFromPerSegmentDetailsAndCanResumeRefinement() async throws {
        let (vm, store, service) = try await fixture()
        var draft = makeProject(); draft.source = "Complete story, including its ending."
        _ = await vm.create(draft, availableModels: models)
        vm.planOutline(); try await idle(vm)
        XCTAssertEqual(vm.project?.segments.count, 3)
        XCTAssertEqual(vm.project?.totalSeconds, 45)
        XCTAssertTrue(vm.project?.segments.allSatisfy { $0.detail == nil } == true)
        var events = await service.events()
        XCTAssertEqual(events, ["story_save_outline"])
        vm.refineSegments(["s1"]); try await idle(vm)
        XCTAssertNotNil(vm.project?.segments[0].detail)
        XCTAssertNil(vm.project?.segments[1].detail)
        vm.refineSegments(["s1", "s2", "s3"]); try await idle(vm)
        events = await service.events()
        XCTAssertEqual(events.filter { $0 == "story_update_segment" }.count, 3, "Already finished details must not be re-requested")
        let saved = try await store.load(owner: "alice")
        XCTAssertTrue(saved.projects[0].segments.allSatisfy { $0.detail != nil })
        XCTAssertFalse(events.contains("video")); XCTAssertFalse(events.contains("image"))
    }

    func testAIOptimizationReturnsSuggestionWithoutOverwritingSavedStory() async throws {
        let (vm, _, service) = try await fixture()
        var draft = makeProject(); draft.source = "Original ending stays unchanged."
        _ = await vm.create(draft, availableModels: models)
        vm.optimizeSourceDraft(draft.source, style: draft.style, target: .source)
        try await idle(vm)
        XCTAssertEqual(vm.project?.source, draft.source)
        XCTAssertEqual(vm.optimizationTarget, .source)
        XCTAssertEqual(vm.optimizationSuggestion?.optimizedText, "Improved candidate")
        let events = await service.events()
        XCTAssertTrue(events.contains("story_suggest_optimized_text"))
        vm.clearOptimizationSuggestion()
        XCTAssertNil(vm.optimizationSuggestion)
    }

    func testInvalidOutlineDoesNotOverwriteProject() throws {
        var project = makeProject(); project.source = "Story"
        let invalid = Data(#"{"summary":"summary","props":[],"segments":[{"id":"s1","title":"one","synopsis":"one","propIDs":["missing"]}]}"#.utf8)
        XCTAssertThrowsError(try StoryPlanningTools.applyOutline(invalid, to: project))
        XCTAssertTrue(project.segments.isEmpty)
        let duplicate = Data(#"{"summary":"summary","props":[],"segments":[{"id":"s1","title":"one","synopsis":"one","propIDs":[]},{"id":"s1","title":"two","synopsis":"two","propIDs":[]}]}"#.utf8)
        XCTAssertThrowsError(try StoryPlanningTools.applyOutline(duplicate, to: project))
    }

    func testShotIntervalsMustCoverExactlyFifteenSeconds() throws {
        var detail = makeDetail()
        XCTAssertNoThrow(try detail.validate())
        detail.shots[0].end = 14
        XCTAssertThrowsError(try detail.validate())
        detail.shots = [.init(start: 0, end: 5, prompt: "first"), .init(start: 6, end: 15, prompt: "second")]
        XCTAssertThrowsError(try detail.validate())
        detail.shots = [.init(start: 0, end: 8, prompt: "first"), .init(start: 7, end: 15, prompt: "second")]
        XCTAssertThrowsError(try detail.validate())
    }

    func testReorderingRecomputesPositionsAndInvalidatesContinuity() async throws {
        let (vm, _, _) = try await fixture()
        var draft = makeProject()
        draft.segments = ["s1", "s2"].enumerated().map { index, id in
            var segment = StorySegment(id: id, title: id, synopsis: id, sourceRange: .init(start: index, end: index + 1))
            segment.detail = makeDetail(); return segment
        }
        _ = await vm.create(draft, availableModels: models)
        vm.moveSegment("s2", offset: -1); try await idle(vm)
        XCTAssertEqual(vm.project?.segments.map(\.id), ["s2", "s1"])
        XCTAssertEqual(vm.project?.totalSeconds, 30)
        XCTAssertTrue(vm.project?.segments.allSatisfy { $0.detail == nil } == true)
    }

    func testParallelFailureSavesEveryTaskAndResumeDoesNotResubmit() async throws {
        let (vm, store, service) = try await fixture()
        await service.setVideoFailure(true)
        let draft = try await readyProject(store)
        _ = await vm.create(draft, availableModels: models)
        vm.selectedSegments = ["s1", "s2"]
        vm.generateBatch(availableModels: models); try await idle(vm)
        XCTAssertEqual(vm.project?.segments[0].attempt?.jobID, "remote-job")
        XCTAssertEqual(vm.project?.segments[1].attempt?.jobID, "remote-job",
                       "One failed segment must not stop another parallel submission")
        let persisted = try await store.load(owner: "alice")
        XCTAssertEqual(persisted.projects[0].segments[0].attempt?.jobID, "remote-job")
        XCTAssertEqual(persisted.projects[0].segments[1].attempt?.jobID, "remote-job")
        vm.generateBatch(availableModels: models); try await idle(vm)
        // Both segments remain locked to their original provider jobs.
        var events = await service.events()
        XCTAssertEqual(events.filter { $0 == "video" }.count, 2)
        await service.setVideoFailure(false)
        vm.resumeVideo("s1"); try await idle(vm)
        events = await service.events()
        XCTAssertEqual(events.filter { $0 == "video" }.count, 2)
        XCTAssertEqual(events.filter { $0 == "resume" }.count, 1)
        XCTAssertNotNil(vm.project?.segments[0].video)
        vm.selectedSegments = ["s1"]
        vm.generateBatch(availableModels: models); try await idle(vm)
        let last = await service.events()
        XCTAssertEqual(last, events, "A completed segment cannot be resubmitted by batch")
    }

    func testSelectedVideosRunSequentiallySoActualTailCanFeedTheNextRequest() async throws {
        let (vm, store, service) = try await fixture()
        await service.setDelay(true)
        let draft = try await readyProject(store)
        _ = await vm.create(draft, availableModels: models)
        vm.selectedSegments = ["s1", "s2"]
        vm.generateBatch(availableModels: models)
        try await Task.sleep(for: .milliseconds(20))
        XCTAssertFalse(vm.isBusy, "Per-segment media must not lock the whole story workspace")
        XCTAssertTrue(vm.isGeneratingVideo("s1", projectID: draft.id))
        XCTAssertFalse(vm.isGeneratingVideo("s2", projectID: draft.id))
        try await idle(vm)
        let maximumConcurrentVideoCalls = await service.maximumConcurrentVideoCalls()
        XCTAssertEqual(maximumConcurrentVideoCalls, 1)
        XCTAssertTrue(vm.project?.segments.allSatisfy { $0.video != nil } == true)
        let persisted = try await store.load(owner: "alice")
        XCTAssertTrue(persisted.projects[0].segments.allSatisfy { $0.video != nil })
    }

    func testPreviousVideoGuidanceSendsPreviousCutWithoutFrameInputs() async throws {
        let (vm, store, service) = try await fixture()
        var draft = makeProject()
        draft.source = "测试"
        let frame = try await store.saveImage(
            Data("frame".utf8), mimeType: "image/png", projectID: draft.id, owner: "alice"
        )
        let previousBytes = Data("previous-video".utf8)
        let previousVideo = try await store.saveVideo(
            .init(id: "previous-job", modelConfigID: "video", modelName: "MiniMax-H3", createdAt: "",
                  mimeType: "video/mp4", videoData: previousBytes),
            projectID: draft.id, owner: "alice"
        )
        var previous = StorySegment(id: "s1", title: "上一段", synopsis: "上一段",
                                    sourceRange: .init(start: 0, end: 1))
        previous.detail = makeDetail()
        previous.firstFrames.images = [frame]; previous.confirmedFrameID = frame.id
        previous.video = previousVideo
        var current = StorySegment(id: "s2", title: "当前段", synopsis: "当前段",
                                   sourceRange: .init(start: 1, end: 2))
        current.detail = makeDetail()
        current.firstFrames.images = [frame]; current.confirmedFrameID = frame.id
        current.videoGuidanceMode = .previousVideo
        draft.segments = [previous, current]
        let created = await vm.create(draft, availableModels: models)
        XCTAssertTrue(created)

        vm.selectedSegments = ["s2"]
        vm.generateBatch(availableModels: models)
        try await idle(vm)

        let capturedRequest = await service.lastVideoRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertNil(request.inputImage)
        XCTAssertNil(request.lastFrameImage)
        XCTAssertEqual(Data(base64Encoded: try XCTUnwrap(request.referenceVideo?.base64Data)), previousBytes)
        XCTAssertEqual(request.referencePurpose, .extend)
        XCTAssertTrue(request.prompt.contains("参考视频1就是紧邻本段之前的完整成片"))
    }

    func testCompletedVideoCanBeReopenedForEditingWithoutLosingHistory() async throws {
        let (vm, store, service) = try await fixture()
        var draft = makeProject()
        let frame = try await store.saveImage(
            Data("frame".utf8), mimeType: "image/png", projectID: draft.id, owner: "alice"
        )
        let oldVideo = try await store.saveVideo(
            .init(id: "old-job", modelConfigID: "video", modelName: "MiniMax-H3", createdAt: "",
                  mimeType: "video/mp4", videoData: Data("old-video".utf8)),
            projectID: draft.id, owner: "alice"
        )
        var attempt = StoryVideoAttempt(
            modelConfigID: "video", prompt: "old prompt", size: "768P", ratio: "16:9"
        )
        attempt.jobID = "old-job"; attempt.status = "completed"
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        segment.detail = makeDetail()
        segment.firstFrames.images = [frame]; segment.confirmedFrameID = frame.id
        segment.attempt = attempt; segment.video = oldVideo
        draft.segments = [segment]
        let createdForEditing = await vm.create(draft, availableModels: models)
        XCTAssertTrue(createdForEditing)

        vm.prepareCompletedVideoForEditing("s1")
        try await idle(vm)

        let saved = try XCTUnwrap(vm.project?.segments.first)
        XCTAssertNil(saved.video)
        XCTAssertNil(saved.attempt)
        XCTAssertEqual(saved.archivedVideos.map(\.jobID), ["old-job"])
        XCTAssertEqual(saved.previousAttempts.map(\.id), [attempt.id])
        XCTAssertTrue(saved.isReady)
        let group = try XCTUnwrap(vm.creationHistoryGroups.first)
        XCTAssertEqual(group.videos.map(\.fileURL.lastPathComponent), [oldVideo.filename])
        XCTAssertTrue(group.currentVideos.isEmpty)
        XCTAssertFalse(group.isComplete)
        let editingEvents = await service.events()
        XCTAssertTrue(editingEvents.isEmpty)
    }

    func testCompletedVideoCanBeRegeneratedWhileOldVersionRemainsInHistory() async throws {
        let (vm, store, service) = try await fixture()
        var draft = makeProject()
        let frame = try await store.saveImage(
            Data("frame".utf8), mimeType: "image/png", projectID: draft.id, owner: "alice"
        )
        let oldVideo = try await store.saveVideo(
            .init(id: "old-job", modelConfigID: "video", modelName: "MiniMax-H3", createdAt: "",
                  mimeType: "video/mp4", videoData: Data("old-video".utf8)),
            projectID: draft.id, owner: "alice"
        )
        var attempt = StoryVideoAttempt(
            modelConfigID: "video", prompt: "old prompt", size: "768P", ratio: "16:9"
        )
        attempt.jobID = "old-job"; attempt.status = "completed"
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        segment.detail = makeDetail()
        segment.firstFrames.images = [frame]; segment.confirmedFrameID = frame.id
        segment.attempt = attempt; segment.video = oldVideo
        draft.segments = [segment]
        let createdForRegeneration = await vm.create(draft, availableModels: models)
        XCTAssertTrue(createdForRegeneration)

        vm.regenerateCompletedVideo("s1", availableModels: models, userIdeas: "节奏更舒缓")
        try await idle(vm)

        let saved = try XCTUnwrap(vm.project?.segments.first)
        XCTAssertEqual(saved.video?.jobID, "remote-job")
        XCTAssertEqual(saved.archivedVideos.map(\.jobID), ["old-job"])
        XCTAssertEqual(saved.previousAttempts.map(\.id), [attempt.id])
        let group = try XCTUnwrap(vm.creationHistoryGroups.first)
        XCTAssertEqual(group.videos.count, 2)
        XCTAssertEqual(group.currentVideos.map(\.segmentID), ["s1"])
        XCTAssertTrue(group.isComplete)
        let regenerationEvents = await service.events()
        XCTAssertEqual(regenerationEvents.filter { $0 == "video" }.count, 1)
        let capturedRequest = await service.lastVideoRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertNil(request.inputImage)
        XCTAssertNil(request.lastFrameImage)
        XCTAssertEqual(
            Data(base64Encoded: try XCTUnwrap(request.referenceVideo?.base64Data)),
            Data("old-video".utf8)
        )
        XCTAssertEqual(request.referencePurpose, .edit)
        XCTAssertTrue(request.prompt.contains("参考视频1就是本段需要修改的原视频"))
        XCTAssertTrue(request.prompt.contains("节奏更舒缓"))
    }

    func testCompletedSegmentPlanCanBeRegeneratedAndArchivesItsOldVideo() async throws {
        let (vm, store, service) = try await fixture()
        var draft = makeProject()
        let frame = try await store.saveImage(
            Data("frame".utf8), mimeType: "image/png", projectID: draft.id, owner: "alice"
        )
        let oldVideo = try await store.saveVideo(
            .init(id: "old-plan-job", modelConfigID: "video", modelName: "MiniMax-H3", createdAt: "",
                  mimeType: "video/mp4", videoData: Data("old-video".utf8)),
            projectID: draft.id, owner: "alice"
        )
        var attempt = StoryVideoAttempt(
            modelConfigID: "video", prompt: "old plan", size: "768P", ratio: "16:9"
        )
        attempt.jobID = "old-plan-job"; attempt.status = "completed"
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        segment.detail = makeDetail()
        segment.firstFrames.images = [frame]; segment.confirmedFrameID = frame.id
        segment.attempt = attempt; segment.video = oldVideo
        draft.segments = [segment]
        let created = await vm.create(draft, availableModels: models)
        XCTAssertTrue(created)

        vm.regenerateSegmentPlans(["s1"], userIdeas: "换成近景")
        try await idle(vm)

        let saved = try XCTUnwrap(vm.project?.segments.first)
        XCTAssertNil(saved.video)
        XCTAssertNil(saved.attempt)
        XCTAssertEqual(saved.archivedVideos.map(\.jobID), ["old-plan-job"])
        XCTAssertEqual(saved.detail?.firstFramePrompt, "a girl")
        XCTAssertNil(saved.confirmedFrameID)
        let planningEvents = await service.events()
        XCTAssertEqual(planningEvents.filter { $0 == "story_update_segment" }.count, 1)
    }

    func testUnsupportedDurationStopsBeforeSubmitting() async throws {
        let (vm, store, service) = try await fixture()
        var draft = try await readyProject(store); draft.models.videoModelID = "text"
        _ = await vm.create(draft, availableModels: models)
        vm.selectedSegments = ["s1"]
        vm.generateBatch(availableModels: models); try await idle(vm)
        XCTAssertNotNil(vm.errorMessage)
        XCTAssertNil(vm.project?.segments[0].attempt)
        let events = await service.events(); XCTAssertTrue(events.isEmpty)
    }

    func testPreflightVideoFailureReleasesIntentWithoutInventingARecoverableTask() async throws {
        let (vm, store, service) = try await fixture()
        await service.setPreflightVideoFailure(true)
        let draft = try await readyProject(store)
        _ = await vm.create(draft, availableModels: models)
        vm.selectedSegments = ["s1"]
        vm.generateBatch(availableModels: models)
        try await idle(vm)

        let segment = try XCTUnwrap(vm.project?.segments.first)
        XCTAssertNil(segment.attempt, "A request rejected before POST must not retain a fake remote-task lock")
        XCTAssertEqual(segment.error, "preflight rejected")
        XCTAssertNil(segment.video)
    }

    func testLegacyMiniMaxPromptFailureIsUnlockedWhenProjectLoads() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryPreflightMigration-\(UUID())")
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        var project = makeProject()
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        segment.attempt = .init(modelConfigID: "video", prompt: String(repeating: "长提示", count: 3_000),
                                size: "768P", ratio: "16:9")
        segment.error = "MiniMax 视频提示词不能为空，且不能超过 7000 字符。"
        project.segments = [segment]
        try await store.save(project, owner: "alice")

        let loaded = try await store.load(owner: "alice")
        XCTAssertNil(loaded.projects.first?.segments.first?.attempt)
        XCTAssertEqual(loaded.projects.first?.segments.first?.error,
                       "MiniMax 视频提示词不能为空，且不能超过 7000 字符。")
    }

    func testManualRetryArchivesIntentAndNeverImmediatelyGenerates() async throws {
        let (vm, _, service) = try await fixture()
        var draft = makeProject()
        var segment = StorySegment(id: "s1", title: "s1", synopsis: "s1", sourceRange: .init(start: 0, end: 1))
        segment.attempt = .init(modelConfigID: "video", prompt: "test", size: "768P", ratio: "16:9")
        draft.segments = [segment]
        _ = await vm.create(draft, availableModels: models)
        vm.resumeVideo("s1")
        XCTAssertNotNil(vm.errorMessage)
        vm.allowRetryAfterVerification("s1"); try await idle(vm)
        XCTAssertNil(vm.project?.segments[0].attempt)
        XCTAssertEqual(vm.project?.segments[0].previousAttempts.count, 1)
        let events = await service.events(); XCTAssertTrue(events.isEmpty)
    }

    func testStoreRejectsTraversalAndPreservesCorruptProjects() async throws {
        let (_, store, _) = try await fixture()
        let draft = makeProject()
        try await store.save(draft, owner: "alice")
        XCTAssertThrowsError(try store.fileURL("../../secret", projectID: draft.id, owner: "alice"))
        let manifest = try store.fileURL("project.json", projectID: draft.id, owner: "alice")
        try Data("broken".utf8).write(to: manifest)
        let snapshot = try await store.load(owner: "alice")
        XCTAssertEqual(snapshot.unreadableCount, 1)
        XCTAssertTrue(snapshot.projects.isEmpty)
        XCTAssertEqual(try String(contentsOf: manifest, encoding: .utf8), "broken")
    }

    func testSigningOutDuringPlanningDropsLateOutput() async throws {
        let (vm, store, service) = try await fixture()
        await service.setDelay(true)
        var draft = makeProject(); draft.source = "Full story"
        _ = await vm.create(draft, availableModels: models)
        vm.planOutline(); vm.activate(userID: "bob")
        try await idle(vm)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertTrue(vm.projects.isEmpty)
        let alice = try await store.load(owner: "alice")
        XCTAssertTrue(alice.projects[0].segments.isEmpty)
    }

    func testPauseFinishesCurrentRefinementAndRetainsItsCheckpoint() async throws {
        let (vm, _, service) = try await fixture()
        var draft = makeProject(); draft.source = "Story"
        _ = await vm.create(draft, availableModels: models)
        vm.planOutline(); try await idle(vm)
        await service.setDelay(true)
        vm.refineSegments(["s1", "s2", "s3"])
        for _ in 0..<30 {
            if await service.events().contains("story_update_segment") { break }
            try await Task.sleep(for: .milliseconds(2))
        }
        vm.requestPause(); try await idle(vm)
        XCTAssertNotNil(vm.project?.segments[0].detail)
        XCTAssertNil(vm.project?.segments[1].detail)
        let events = await service.events()
        XCTAssertEqual(events.filter { $0 == "story_update_segment" }.count, 1)
    }

    func testRunningProjectCanBeReopenedFromListAndUnsavedTextSurvivesNavigation() async throws {
        let (vm, _, service) = try await fixture()
        var draft = makeProject(); draft.source = "Story"
        _ = await vm.create(draft, availableModels: models)
        vm.rememberSourceDraft(projectID: draft.id, source: "Unsaved edit", style: draft.style, ratio: draft.ratio)
        vm.backToList(); vm.open(draft.id)
        XCTAssertEqual(vm.sourceDraft(for: draft).source, "Unsaved edit")
        await service.setDelay(true)
        vm.planOutline()
        XCTAssertEqual(vm.activeProjectID, draft.id)
        vm.backToList(); vm.open(draft.id)
        XCTAssertEqual(vm.selectedProjectID, draft.id, "A running project must remain accessible from the list")
        try await idle(vm)
    }

    func testPausedAgentDraftDoesNotLockOrReplaceCanonicalProjectAfterReopen() async throws {
        let (vm, store, _) = try await fixture()
        let canonical = makeProject()
        _ = await vm.create(canonical, availableModels: models)

        var run = try StoryAgentRun(project: canonical, owner: "alice", stage: .outline,
                                    targetIDs: [], policy: .init())
        run.draft.characters = [.init(id: "hero", name: "主角", profile: characterProfile())]
        run.draft.scenes = [.init(id: "room", name: "房间", profile: sceneProfile())]
        run.draft.segments = [.init(id: "s1", title: "开场", synopsis: "主角进入房间",
                                    sourceRange: .init(start: 0, end: canonical.source.count),
                                    characterIDs: ["hero"], sceneIDs: ["room"])]
        run.checkpoint.status = .paused
        run.checkpoint.stopReason = "Paused for test"
        run.updatedAt = Date().addingTimeInterval(1)
        try await store.saveRun(run, owner: "alice")

        let other = StoryProject(title: "Another Story", description: "", models: canonical.models)
        _ = await vm.create(other, availableModels: models)
        vm.open(canonical.id)
        for _ in 0..<100 where vm.isLoadingAgentRuns { try await Task.sleep(for: .milliseconds(10)) }

        let saved = try XCTUnwrap(vm.project)
        XCTAssertTrue(saved.characters.isEmpty, "An unfinished run must not overwrite the canonical project")
        XCTAssertFalse(vm.isPresentingAgentDraft(projectID: canonical.id))
        let presented = vm.presentationProject(for: saved)
        XCTAssertTrue(presented.characters.isEmpty)
        XCTAssertTrue(presented.scenes.isEmpty)
        XCTAssertTrue(presented.segments.isEmpty)
        XCTAssertEqual(vm.recoverableAgentRun(projectID: canonical.id)?.draft.characters.map(\.id), ["hero"])

        vm.open(other.id)
        for _ in 0..<100 where vm.isLoadingAgentRuns { try await Task.sleep(for: .milliseconds(10)) }
        let otherSaved = try XCTUnwrap(vm.project)
        XCTAssertFalse(vm.isPresentingAgentDraft(projectID: other.id))
        XCTAssertTrue(vm.presentationProject(for: otherSaved).resources.isEmpty)
    }

    func testOpeningProjectAutomaticallyAppliesPausedDraftThatAlreadyPassesCompletionValidation() async throws {
        let (vm, store, _) = try await fixture()
        var canonical = makeProject()
        canonical.scenes = [.init(id: "room", name: "房间", profile: sceneProfile())]
        canonical.segments = [.init(id: "s1", title: "开场", synopsis: "完整剧情",
                                    sourceRange: .init(start: 0, end: canonical.source.count), sceneIDs: ["room"])]
        _ = await vm.create(canonical, availableModels: models)

        var run = try StoryAgentRun(project: canonical, owner: "alice", stage: .refine,
                                    targetIDs: ["s1"], policy: .init())
        run.draft.segments[0].detail = makeDetail()
        run.checkpoint.status = .paused
        run.checkpoint.stopReason = "旧版本误判为无进展"
        run.updatedAt = Date().addingTimeInterval(1)
        try await store.saveRun(run, owner: "alice")

        vm.backToList(); vm.open(canonical.id)
        for _ in 0..<100 where vm.isLoadingAgentRuns { try await Task.sleep(for: .milliseconds(10)) }

        XCTAssertNotNil(vm.project?.segments[0].detail)
        XCTAssertFalse(vm.isPresentingAgentDraft(projectID: canonical.id))
        let history = try await store.loadRuns(owner: "alice", projectID: canonical.id)
        let savedRun = try XCTUnwrap(history.runs.first)
        XCTAssertTrue(savedRun.applied)
        XCTAssertEqual(savedRun.checkpoint.status, .completed)
        XCTAssertNil(savedRun.checkpoint.stopReason)
    }

    func testChangingSharedAssetVersionInvalidatesDependentFramesOnly() async throws {
        let (vm, store, _) = try await fixture()
        var draft = try await readyProject(store)
        var asset = StoryCharacter(id: "actor", name: "Actor", profile: characterProfile(), imagePrompt: "same appearance")
        let first = try await store.saveImage(Data("first".utf8), mimeType: "image/png", projectID: draft.id, owner: "alice")
        let second = try await store.saveImage(Data("second".utf8), mimeType: "image/png", projectID: draft.id, owner: "alice")
        asset.media.images = [first, second]; asset.media.confirmedImageID = first.id
        draft.characters = [asset]; draft.segments[0].characterIDs = [asset.id]
        _ = await vm.create(draft, availableModels: models)
        vm.confirmImage(second, assetID: asset.id, segmentID: nil); try await idle(vm)
        XCTAssertEqual(vm.project?.characters[0].media.confirmedImageID, second.id)
        XCTAssertEqual(vm.project?.characters[0].media.images.count, 2, "Old images must remain available")
        XCTAssertNil(vm.project?.segments[0].firstFrame)
        XCTAssertNotNil(vm.project?.segments[1].firstFrame)
        XCTAssertEqual(vm.project?.segments[0].firstFrames.images.count, 1)
    }

    func testGenerationContextContainsOnlySegmentRelationsAndReferencedEntities() throws {
        var project = makeProject()
        project.characters = [
            .init(id: "hero", name: "主角", profile: characterProfile()),
            .init(id: "villain", name: "反派", profile: characterProfile()),
        ]
        project.scenes = [
            .init(id: "room", name: "室内", profile: sceneProfile()),
            .init(id: "yard", name: "庭院", profile: sceneProfile()),
        ]
        project.props = [
            .init(id: "key", name: "钥匙", description: "一把旧铜钥匙"),
            .init(id: "map", name: "地图", description: "一张无关地图"),
        ]
        var segment = StorySegment(id: "s1", title: "进屋", synopsis: "主角进屋", sourceRange: .init(start: 0, end: 1),
                                   characterIDs: ["hero"], sceneIDs: ["room"], propIDs: ["key"])
        segment.detail = makeDetail()
        project.segments = [segment]
        project.relations = [.init(id: "r1", segmentID: "s1", characterID: "hero", sceneID: "room",
                                   action: "推门进入", position: "门口")]
        let context = try StoryGenerationContext.text(project, segment: segment)
        XCTAssertTrue(context.contains("hero")); XCTAssertTrue(context.contains("room")); XCTAssertTrue(context.contains("推门进入"))
        XCTAssertTrue(context.contains("key")); XCTAssertTrue(context.contains("门在北侧")); XCTAssertTrue(context.contains("短发"))
        XCTAssertFalse(context.contains("villain")); XCTAssertFalse(context.contains("yard")); XCTAssertFalse(context.contains("map"))

        let firstFramePrompt = try StoryGenerationContext.framePrompt(project, segment: segment, role: .first,
                                                                       referenceResourceIDs: ["hero"])
        let lastFramePrompt = try StoryGenerationContext.framePrompt(project, segment: segment, role: .last,
                                                                      referenceResourceIDs: ["hero"])
        for prompt in [firstFramePrompt, lastFramePrompt] {
            XCTAssertTrue(prompt.contains("slow tracking shot"), "Frame generation must include the full shot language")
            XCTAssertTrue(prompt.contains("门在北侧"), "Linked scene text must remain even when its image is not selected")
            XCTAssertTrue(prompt.contains("短发"), "Frame generation must include the linked character profile")
            XCTAssertTrue(prompt.contains("一把旧铜钥匙"), "Linked prop text must remain even when its image is not selected")
            XCTAssertTrue(prompt.contains("推门进入"), "Frame generation must include explicit character-scene relations")
        }
        let userIdeas = "使用低机位，并突出钥匙上的划痕"
        let heroResource = try XCTUnwrap(project.resource(id: "hero"))
        XCTAssertTrue(StoryGenerationContext.assetPrompt(
            project, resource: heroResource, userIdeas: userIdeas
        ).contains(userIdeas))
        XCTAssertTrue(try StoryGenerationContext.framePrompt(
            project, segment: segment, role: .first, referenceResourceIDs: ["hero"],
            userIdeas: userIdeas
        ).contains(userIdeas))
        XCTAssertTrue(try StoryGenerationContext.videoPrompt(
            project, segment: segment, userIdeas: userIdeas
        ).contains(userIdeas))
        var previousVideoSegment = segment
        previousVideoSegment.videoGuidanceMode = .previousVideo
        project.segments = [previousVideoSegment]
        XCTAssertTrue(try StoryGenerationContext.videoPrompt(
            project, segment: previousVideoSegment
        ).contains("参考视频1就是紧邻本段之前的完整成片"))

        var verboseSegment = segment
        verboseSegment.detail = .init(
            firstFramePrompt: String(repeating: "首帧", count: 1_500),
            shots: [.init(start: 0, end: 15, prompt: String(repeating: "镜头动作", count: 500))],
            continuityIn: String(repeating: "入镜", count: 500),
            continuityOut: String(repeating: "出镜", count: 500),
            audio: String(repeating: "声音", count: 500),
            constraints: String(repeating: "约束", count: 1_000),
            lastFramePrompt: String(repeating: "尾帧", count: 1_500)
        )
        project.segments = [verboseSegment]
        let budgetedVideoPrompt = try StoryGenerationContext.videoPrompt(project, segment: verboseSegment)
        XCTAssertLessThanOrEqual(budgetedVideoPrompt.count, 6_800)
        XCTAssertTrue(budgetedVideoPrompt.contains("0–15秒"), "Prompt budgeting must retain every timed shot")

        let tail = StoryImage(filename: "previous-tail.png", mimeType: "image/png")
        var previous = StorySegment(id: "s0", title: "上一段", synopsis: "主角走到门口",
                                    sourceRange: .init(start: 0, end: 1), characterIDs: ["hero"],
                                    sceneIDs: ["room"], propIDs: ["key"])
        previous.detail = .init(firstFramePrompt: "wide room",
                                shots: [.init(start: 0, end: 15, prompt: "move toward the north door")],
                                continuityIn: "at the table", continuityOut: "facing north while holding the key",
                                audio: "steps", constraints: "same room", lastFramePrompt: "north-facing tail frame")
        previous.lastFrames.images = [tail]; previous.confirmedLastFrameID = tail.id
        segment.sourceRange = .init(start: 1, end: 2)
        project.source = "测试"
        project.segments = [previous, segment]
        project.relations.append(.init(id: "r0", segmentID: "s0", characterID: "hero", sceneID: "room",
                                       action: "走到门口", position: "北侧门口"))
        let continued = try StoryGenerationContext.framePrompt(
            project, segment: segment, role: .first, referenceResourceIDs: ["hero"],
            previousTailReferenceIndex: 2
        )
        XCTAssertTrue(continued.contains("previousSegmentTailReferenceIndex\":2"))
        XCTAssertTrue(continued.contains("north-facing tail frame"))
        XCTAssertTrue(continued.contains("facing north while holding the key"))
        XCTAssertThrowsError(try StoryGenerationContext.framePrompt(
            project, segment: segment, role: .first, referenceResourceIDs: ["hero"],
            previousTailReferenceIndex: 1
        ))
    }

    func testDeterministicPipelineOrdersResourcesThenFramesThenVideos() throws {
        var project = makeProject()
        project.characters = [.init(id: "hero", name: "主角", profile: characterProfile())]
        project.scenes = [.init(id: "room", name: "室内", profile: sceneProfile())]
        var segment = StorySegment(id: "s1", title: "进屋", synopsis: "主角进屋", sourceRange: .init(start: 0, end: 1),
                                   characterIDs: ["hero"], sceneIDs: ["room"])
        segment.detail = makeDetail()
        project.segments = [segment]
        project.relations = [.init(id: "r1", segmentID: "s1", characterID: "hero", sceneID: "room",
                                   action: "推门进入", position: "门口")]
        let batch = try StoryMediaBatch(project: project, owner: "alice", kind: .pipeline,
                                        targets: ["s1"], models: models,
                                        userIdeas: "整体使用温暖的晨光")
        XCTAssertEqual(batch.steps.map(\.kind), [.assets, .assets, .frames, .videos])
        XCTAssertEqual(batch.steps.map(\.targetID), ["hero", "room", "s1", "s1"])
        XCTAssertEqual(batch.userIdeas, "整体使用温暖的晨光")

        project.segments[0].useLastFrameForVideo = true
        let withLastFrame = try StoryMediaBatch(project: project, owner: "alice", kind: .pipeline,
                                                targets: ["s1"], models: models)
        XCTAssertEqual(withLastFrame.steps.map(\.kind), [.assets, .assets, .frames, .lastFrames, .videos])
        var legacyObject = try XCTUnwrap(JSONSerialization.jsonObject(
            with: JSONEncoder().encode(batch)
        ) as? [String: Any])
        legacyObject.removeValue(forKey: "userIdeas")
        let legacyBatch = try JSONDecoder().decode(
            StoryMediaBatch.self, from: JSONSerialization.data(withJSONObject: legacyObject)
        )
        XCTAssertNil(legacyBatch.userIdeas)
    }

    func testCreationHistoryGroupsGeneratedMediaByStoryAndKeepsTimelineOrder() async throws {
        let (vm, store, _) = try await fixture()
        var project = makeProject()

        let generatedAsset = try await store.saveImage(
            Data("generated-asset".utf8), mimeType: "image/png", projectID: project.id, owner: "alice",
            sourceResourceID: "key", generationAttemptID: UUID(), providerResultID: "asset-result"
        )
        let uploadedAsset = try await store.saveImage(
            Data("uploaded-asset".utf8), mimeType: "image/png", projectID: project.id, owner: "alice"
        )
        var prop = StoryProp(id: "key", name: "钥匙", description: "旧铜钥匙")
        prop.media.images = [generatedAsset, uploadedAsset]
        prop.media.confirmedImageID = generatedAsset.id
        project.props = [prop]

        let firstFrame = try await store.saveImage(
            Data("first".utf8), mimeType: "image/png", projectID: project.id, owner: "alice",
            sourceResourceID: "s1", generationAttemptID: UUID(), providerResultID: "first-result"
        )
        let sharedTail = try await store.saveImage(
            Data("tail".utf8), mimeType: "image/png", projectID: project.id, owner: "alice",
            sourceResourceID: "s1", generationAttemptID: UUID(), providerResultID: "tail-result"
        )
        let videoOne = try await store.saveVideo(
            .init(id: "video-one", modelConfigID: "video", modelName: "video-model", createdAt: "",
                  mimeType: "video/mp4", videoData: Data("video-one".utf8)),
            projectID: project.id, owner: "alice"
        )
        let videoTwo = try await store.saveVideo(
            .init(id: "video-two", modelConfigID: "video", modelName: "video-model", createdAt: "",
                  mimeType: "video/mp4", videoData: Data("video-two".utf8)),
            projectID: project.id, owner: "alice"
        )

        var first = StorySegment(id: "s1", title: "开场", synopsis: "开场", sourceRange: .init(start: 0, end: 1))
        first.detail = makeDetail()
        first.firstFrames.images = [firstFrame]; first.confirmedFrameID = firstFrame.id
        first.lastFrames.images = [sharedTail]; first.confirmedLastFrameID = sharedTail.id
        first.actualVideoLastFrameID = sharedTail.id
        first.attempt = .init(modelConfigID: "video", prompt: "开场视频", size: "768P", ratio: "16:9")
        first.video = videoOne

        var second = StorySegment(id: "s2", title: "结尾", synopsis: "结尾", sourceRange: .init(start: 1, end: 2))
        second.detail = makeDetail()
        second.firstFrames.images = [sharedTail]; second.confirmedFrameID = sharedTail.id
        second.inheritedFirstFrameSourceSegmentID = "s1"
        second.attempt = .init(modelConfigID: "video", prompt: "结尾视频", size: "768P", ratio: "16:9")
        second.video = videoTwo
        project.segments = [first, second]

        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created)
        let group = try XCTUnwrap(vm.creationHistoryGroups.first)
        XCTAssertEqual(group.projectTitle, "Test Story")
        XCTAssertEqual(group.totalSegmentCount, 2)
        XCTAssertTrue(group.isComplete)
        XCTAssertEqual(group.images.map(\.kind), [.prop, .firstFrame, .lastFrame])
        XCTAssertFalse(group.images.contains { $0.asset.id == uploadedAsset.id.uuidString })
        XCTAssertEqual(group.videos.map(\.segmentID), ["s1", "s2"])
        XCTAssertEqual(group.videos.map(\.segmentNumber), [1, 2])
        XCTAssertEqual(group.videos.map(\.fileURL.lastPathComponent), [videoOne.filename, videoTwo.filename])
    }

    func testReusableStoryImagesOnlyReturnsMatchingAssetsFromOtherStories() async throws {
        let (vm, store, _) = try await fixture()
        var source = makeProject()
        source.title = "旧剧情"
        let confirmedCharacter = try await store.saveImage(
            Data("confirmed-character".utf8), mimeType: "image/png",
            projectID: source.id, owner: "alice"
        )
        let newerCharacter = try await store.saveImage(
            Data("newer-character".utf8), mimeType: "image/png",
            projectID: source.id, owner: "alice"
        )
        let sceneImage = try await store.saveImage(
            Data("scene".utf8), mimeType: "image/png",
            projectID: source.id, owner: "alice"
        )
        var character = StoryCharacter(id: "hero", name: "旧主角", profile: characterProfile())
        character.media.images = [confirmedCharacter, newerCharacter]
        character.media.confirmedImageID = confirmedCharacter.id
        var scene = StoryScene(id: "room", name: "旧房间", profile: sceneProfile())
        scene.media.images = [sceneImage]
        scene.media.confirmedImageID = sceneImage.id
        source.characters = [character]
        source.scenes = [scene]
        let createdSource = await vm.create(source, availableModels: models)
        XCTAssertTrue(createdSource)

        var current = makeProject()
        current.title = "当前剧情"
        let createdCurrent = await vm.create(current, availableModels: models)
        XCTAssertTrue(createdCurrent)

        let characters = vm.reusableStoryImages(kind: .character, excluding: current.id)
        XCTAssertEqual(characters.map(\.projectTitle), ["旧剧情"])
        XCTAssertEqual(characters.map(\.resourceName), ["旧主角"])
        XCTAssertEqual(characters.map(\.image.id), [confirmedCharacter.id],
                       "The confirmed version should be offered instead of an unconfirmed newer attempt")
        XCTAssertEqual(vm.reusableStoryImages(kind: .scene, excluding: current.id).map(\.resourceName), ["旧房间"])
        XCTAssertTrue(vm.reusableStoryImages(kind: .prop, excluding: current.id).isEmpty)
        XCTAssertFalse(characters.contains { $0.projectID == current.id })
    }

    func testLegacySegmentWithoutTailFrameFieldsStillDecodes() throws {
        let original = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        var object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(original)) as? [String: Any])
        XCTAssertNil(object["archivedVideos"], "Empty video history must keep legacy project digests stable")
        object.removeValue(forKey: "lastFrames")
        object.removeValue(forKey: "useLastFrameForVideo")
        object.removeValue(forKey: "videoGuidanceMode")
        let decoded = try JSONDecoder().decode(StorySegment.self, from: JSONSerialization.data(withJSONObject: object))
        XCTAssertTrue(decoded.lastFrames.images.isEmpty)
        XCTAssertNil(decoded.lastFrameGenerationAttemptID)
        XCTAssertFalse(decoded.useLastFrameForVideo)
        XCTAssertTrue(decoded.archivedVideos.isEmpty)
    }

    func testVideoGuidanceModeRoundTripsAndMigratesLegacyTailFlag() throws {
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        segment.videoGuidanceMode = .previousVideo
        let decoded = try JSONDecoder().decode(StorySegment.self, from: JSONEncoder().encode(segment))
        XCTAssertEqual(decoded.videoGuidanceMode, .previousVideo)
        XCTAssertFalse(decoded.useLastFrameForVideo)

        var legacy = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(segment)) as? [String: Any])
        legacy.removeValue(forKey: "videoGuidanceMode")
        legacy["useLastFrameForVideo"] = true
        let migrated = try JSONDecoder().decode(
            StorySegment.self, from: JSONSerialization.data(withJSONObject: legacy)
        )
        XCTAssertEqual(migrated.videoGuidanceMode, .firstAndLastFrames)
        XCTAssertTrue(migrated.useLastFrameForVideo)
    }

    func testFirstAndLastFrameAttemptsMergeByRoleAndAttemptID() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryFrameStoreTests-\(UUID().uuidString)", isDirectory: true)
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        var project = makeProject()
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        segment.detail = makeDetail()
        project.segments = [segment]
        try await store.save(project, owner: "alice")
        let firstAttempt = UUID(), lastAttempt = UUID()
        _ = try await store.beginFrameImageGeneration(projectID: project.id, segmentID: "s1", role: .first,
                                                       attemptID: firstAttempt, owner: "alice")
        _ = try await store.beginFrameImageGeneration(projectID: project.id, segmentID: "s1", role: .last,
                                                       attemptID: lastAttempt, owner: "alice")
        _ = try await store.completeFrameImageGeneration(Data("last".utf8), mimeType: "image/png", projectID: project.id,
                                                          segmentID: "s1", role: .last, attemptID: lastAttempt,
                                                          providerResultID: "result-last", providerAssetID: "asset-last", owner: "alice")
        _ = try await store.completeFrameImageGeneration(Data("first".utf8), mimeType: "image/png", projectID: project.id,
                                                          segmentID: "s1", role: .first, attemptID: firstAttempt,
                                                          providerResultID: "result-first", providerAssetID: "asset-first", owner: "alice")
        let saved = try await store.load(owner: "alice").projects.first
        XCTAssertEqual(saved?.segments[0].firstFrames.images.first?.generationAttemptID, firstAttempt)
        XCTAssertEqual(saved?.segments[0].lastFrames.images.first?.generationAttemptID, lastAttempt)
        XCTAssertEqual(saved?.segments[0].firstFrames.images.first?.providerResultID, "result-first")
        XCTAssertEqual(saved?.segments[0].lastFrames.images.first?.providerResultID, "result-last")
        XCTAssertEqual(saved?.segments[0].firstFrame?.generationAttemptID, firstAttempt)
        XCTAssertEqual(saved?.segments[0].lastFrame?.generationAttemptID, lastAttempt)
    }

    func testRegenerationKeepsPreviouslyConfirmedAssetAndFrameVersions() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryRegenerationTests-\(UUID().uuidString)", isDirectory: true)
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        var project = makeProject()
        project.props = [.init(id: "key", name: "钥匙", description: "一把旧铜钥匙")]
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1),
                                   propIDs: ["key"])
        segment.detail = makeDetail()
        project.segments = [segment]
        try await store.save(project, owner: "alice")

        let firstAssetAttempt = UUID()
        _ = try await store.beginAssetImageGeneration(projectID: project.id, resourceID: "key",
                                                       attemptID: firstAssetAttempt, owner: "alice")
        let (_, firstAsset) = try await store.completeAssetImageGeneration(
            Data("asset-1".utf8), mimeType: "image/png", projectID: project.id, resourceID: "key",
            attemptID: firstAssetAttempt, providerResultID: "asset-result-1", providerAssetID: "asset-1", owner: "alice"
        )
        let secondAssetAttempt = UUID()
        _ = try await store.beginAssetImageGeneration(projectID: project.id, resourceID: "key",
                                                       attemptID: secondAssetAttempt, owner: "alice")
        let (_, secondAsset) = try await store.completeAssetImageGeneration(
            Data("asset-2".utf8), mimeType: "image/png", projectID: project.id, resourceID: "key",
            attemptID: secondAssetAttempt, providerResultID: "asset-result-2", providerAssetID: "asset-2", owner: "alice"
        )

        let firstFrameAttempt = UUID()
        _ = try await store.beginFrameImageGeneration(projectID: project.id, segmentID: "s1", role: .first,
                                                       attemptID: firstFrameAttempt, owner: "alice")
        let (_, firstFrame) = try await store.completeFrameImageGeneration(
            Data("frame-1".utf8), mimeType: "image/png", projectID: project.id, segmentID: "s1", role: .first,
            attemptID: firstFrameAttempt, providerResultID: "frame-result-1", providerAssetID: "frame-1", owner: "alice"
        )
        let secondFrameAttempt = UUID()
        _ = try await store.beginFrameImageGeneration(projectID: project.id, segmentID: "s1", role: .first,
                                                       attemptID: secondFrameAttempt, owner: "alice")
        let (_, secondFrame) = try await store.completeFrameImageGeneration(
            Data("frame-2".utf8), mimeType: "image/png", projectID: project.id, segmentID: "s1", role: .first,
            attemptID: secondFrameAttempt, providerResultID: "frame-result-2", providerAssetID: "frame-2", owner: "alice"
        )

        let snapshot = try await store.load(owner: "alice")
        let saved = try XCTUnwrap(snapshot.projects.first)
        let savedAsset = try XCTUnwrap(saved.resource(id: "key"))
        XCTAssertEqual(savedAsset.images.map(\.id), [firstAsset.id, secondAsset.id])
        XCTAssertEqual(savedAsset.confirmedImageID, firstAsset.id)
        XCTAssertEqual(saved.segments[0].firstFrames.images.map(\.id), [firstFrame.id, secondFrame.id])
        XCTAssertEqual(saved.segments[0].confirmedFrameID, firstFrame.id)
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
        let vm = StoryStudioViewModel(media: service, store: store)
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
        let vm = StoryStudioViewModel(media: service, store: store)
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
        XCTAssertTrue(request.prompt.contains("previousSegmentTailReferenceIndex\":2"))
        XCTAssertNotNil(vm.project?.segments[1].firstFrame)

        vm.generateFrame("s2", role: .last, referenceAssetIDs: ["hero"])
        try await idle(vm)
        let capturedLastRequest = await service.lastRequest()
        let lastRequest = try XCTUnwrap(capturedLastRequest)
        XCTAssertEqual(lastRequest.referenceImages.map(\.name), ["主角.png", "current-segment-first-frame.png"])
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
        let vm = StoryStudioViewModel(media: service, store: store)
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

    private var models: [MediaGenerationModel] {
        [("text", "text-model"), ("image", "image-model"), ("video", "MiniMax-H3")].map { id, model in
            .init(id: id, name: id, provider: "gpt", modelName: model, enabled: true, taskEnabled: false, hasAPIKey: true)
        }
    }
    private func makeProject() -> StoryProject {
        var project = StoryProject(title: "  Test Story  ", description: "Description", models: .init(textModelID: "text", imageModelID: "image", videoModelID: "video"))
        project.source = "测试剧情正文，覆盖所有测试分段。"
        return project
    }
    private func readyProject(_ store: StoryProjectStore) async throws -> StoryProject {
        var draft = makeProject()
        let frame = try await store.saveImage(Data("test-image".utf8), mimeType: "image/png", projectID: draft.id, owner: "alice")
        draft.segments = ["s1", "s2"].enumerated().map { index, id in
            var segment = StorySegment(id: id, title: id, synopsis: id, sourceRange: .init(start: index, end: index + 1))
            segment.detail = makeDetail(); segment.firstFrames.images = [frame]; segment.confirmedFrameID = frame.id
            segment.lastFrames.images = [frame]; segment.confirmedLastFrameID = frame.id
            return segment
        }
        return draft
    }
    private func characterProfile() -> StoryCharacterProfile {
        .init(isProtagonist: true, roleInStory: "主角", appearance: "短发", personality: "坚定", motivation: "完成任务",
              relationships: "独立行动", costume: "深色外套", consistencyNotes: "保持发型与服装一致")
    }
    private func sceneProfile() -> StorySceneProfile {
        .init(roleInStory: "故事发生地", setting: "当代室内", spatialLayout: "门在北侧，窗在东侧",
              lightingAndPalette: "暖色", keyElements: "木桌", atmosphere: "安静", consistencyNotes: "保持空间方位一致")
    }
    private func makeDetail() -> StorySegmentDetail {
        .init(firstFramePrompt: "first frame", shots: [.init(start: 0, end: 15, prompt: "slow tracking shot")],
              continuityIn: "enter", continuityOut: "exit", audio: "wind", constraints: "same character",
              lastFramePrompt: "last frame")
    }
    private func fixture() async throws -> (StoryStudioViewModel, StoryProjectStore, StoryTestService) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryStudioTests-\(UUID().uuidString)", isDirectory: true)
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        let service = StoryTestService()
        let vm = StoryStudioViewModel(media: service, planner: service, store: store)
        vm.activate(userID: "alice"); try await idle(vm)
        return (vm, store, service)
    }
    private func idle(_ vm: StoryStudioViewModel) async throws {
        for _ in 0..<300 where vm.isBusy || vm.isLoading || vm.hasActiveAssetGenerations
            || vm.hasActiveFrameGenerations || vm.hasActiveVideoGenerations {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(vm.isBusy); XCTAssertFalse(vm.isLoading)
        XCTAssertFalse(vm.hasActiveAssetGenerations); XCTAssertFalse(vm.hasActiveFrameGenerations)
        XCTAssertFalse(vm.hasActiveVideoGenerations)
    }
}

private actor ConcurrentStoryImageService: MediaGenerationServicing {
    private let imageData: Data
    private var activeCalls = 0
    private var maximumCalls = 0
    private var calls: [String: Int] = [:]
    private var continuations: [String: CheckedContinuation<Void, Never>] = [:]

    init(imageData: Data) { self.imageData = imageData }

    func fetchModels() async throws -> [MediaGenerationModel] { [] }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        guard let resourceID = request.resourceID else { throw StoryError.invalidPlan }
        calls[resourceID, default: 0] += 1
        activeCalls += 1
        maximumCalls = max(maximumCalls, activeCalls)
        await withCheckedContinuation { continuation in continuations[resourceID] = continuation }
        activeCalls -= 1
        return .init(id: "result-\(resourceID)", modelConfigID: request.modelConfigID,
                     modelName: "image-model", createdAt: "", images: [
                        .init(id: "asset-\(resourceID)", mimeType: "image/png", base64Data: imageData.base64EncodedString()),
                     ], clientRequestID: request.clientRequestID, projectID: request.projectID, resourceID: resourceID)
    }

    func generateVideo(_ request: VideoGenerationRequest,
                       progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        throw StoryError.unavailable
    }

    func maximumConcurrentCalls() -> Int { maximumCalls }
    func callCount(resourceID: String) -> Int { calls[resourceID, default: 0] }
    func finish(resourceID: String) { continuations.removeValue(forKey: resourceID)?.resume() }
}

private actor RecordingFrameImageService: MediaGenerationServicing {
    private let imageData: Data
    private var request: ImageGenerationRequest?
    init(imageData: Data) { self.imageData = imageData }
    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        self.request = request
        return .init(id: "frame-result", modelConfigID: request.modelConfigID, modelName: "image-model",
                     createdAt: "", images: [.init(id: "frame-image", mimeType: "image/png",
                                                    base64Data: imageData.base64EncodedString())],
                     clientRequestID: request.clientRequestID, projectID: request.projectID,
                     resourceID: request.resourceID)
    }
    func generateVideo(_ request: VideoGenerationRequest,
                       progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        throw StoryError.unavailable
    }
    func lastRequest() -> ImageGenerationRequest? { request }
}

private struct StoryTestPreflightError: LocalizedError, MediaGenerationSubmissionFailure {
    var errorDescription: String? { "preflight rejected" }
    var requestMayHaveBeenSubmitted: Bool { false }
}

private actor StoryTestService: ResumableVideoGenerationServicing, StoryPlanningServicing {
    private var calls: [String] = []
    private var videoRequests: [VideoGenerationRequest] = []
    private var failVideo = false
    private var preflightVideoFailure = false
    private var delayed = false
    private var activeVideoCalls = 0
    private var maximumVideoCalls = 0
    func events() -> [String] { calls }
    func setVideoFailure(_ value: Bool) { failVideo = value }
    func setPreflightVideoFailure(_ value: Bool) { preflightVideoFailure = value }
    func setDelay(_ value: Bool) { delayed = value }
    func maximumConcurrentVideoCalls() -> Int { maximumVideoCalls }
    func lastVideoRequest() -> VideoGenerationRequest? { videoRequests.last }
    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func plan(_ request: StoryPlanningRequest) async throws -> Data {
        calls.append(request.toolName)
        if delayed { try await Task.sleep(for: .milliseconds(50)) }
        if request.toolName == "story_suggest_optimized_text" {
            return Data(#"{"optimizedText":"Improved candidate","rationale":"Clearer pacing"}"#.utf8)
        }
        if request.toolName == "story_save_outline" {
            return Data(#"{"summary":"beginning to ending","props":[],"segments":[{"id":"s1","title":"opening","synopsis":"start","kind":"story","seconds":15,"propIDs":[]},{"id":"s2","title":"middle","synopsis":"middle","kind":"story","seconds":15,"propIDs":[]},{"id":"s3","title":"ending","synopsis":"end","kind":"story","seconds":15,"propIDs":[]}]}"#.utf8)
        }
        return Data(#"{"firstFramePrompt":"a girl","lastFramePrompt":"girl at the exit","shots":[{"start":0,"end":15,"prompt":"tracking shot"}],"continuityIn":"enter","continuityOut":"exit","audio":"wind","constraints":"same character"}"#.utf8)
    }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        calls.append("image"); throw URLError(.badServerResponse)
    }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        calls.append("video")
        videoRequests.append(request)
        if preflightVideoFailure { throw StoryTestPreflightError() }
        activeVideoCalls += 1
        maximumVideoCalls = max(maximumVideoCalls, activeVideoCalls)
        defer { activeVideoCalls -= 1 }
        await progress(.init(status: "queued", jobID: "remote-job"))
        if delayed { try await Task.sleep(for: .milliseconds(80)) }
        if failVideo { throw URLError(.networkConnectionLost) }
        return result(request)
    }
    func resumeVideo(_ request: VideoGenerationRequest, jobID: String, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        calls.append("resume"); return result(request)
    }
    private func result(_ request: VideoGenerationRequest) -> VideoGenerationResult {
        .init(id: "remote-job", modelConfigID: request.modelConfigID, modelName: "MiniMax-H3", createdAt: "", mimeType: "video/mp4", videoData: Data("video".utf8))
    }
}
