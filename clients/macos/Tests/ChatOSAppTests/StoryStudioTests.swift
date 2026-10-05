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

    func testProjectsLoadInBoundedPagesWithoutDroppingOlderProjects() async throws {
        let (_, store, _) = try await fixture()
        var ids: Set<UUID> = []
        for title in ["one", "two", "three"] {
            var project = makeProject()
            project.title = title
            ids.insert(project.id)
            try await store.save(project, owner: "alice")
        }

        let first = try await store.load(owner: "alice", limit: 2)
        XCTAssertEqual(first.projects.count, 2)
        let cursor = try XCTUnwrap(first.nextCursor)
        let second = try await store.load(owner: "alice", after: cursor, limit: 2)
        XCTAssertEqual(second.projects.count, 1)
        XCTAssertNil(second.nextCursor)
        XCTAssertEqual(Set((first.projects + second.projects).map(\.id)), ids)
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

    var models: [MediaGenerationModel] {
        [("text", "text-model"), ("image", "image-model"), ("video", "MiniMax-H3")].map { id, model in
            .init(id: id, name: id, provider: "gpt", modelName: model, enabled: true, taskEnabled: false, hasAPIKey: true)
        }
    }
    func makeProject() -> StoryProject {
        var project = StoryProject(title: "  Test Story  ", description: "Description", models: .init(textModelID: "text", imageModelID: "image", videoModelID: "video"))
        project.source = "测试剧情正文，覆盖所有测试分段。"
        return project
    }
    func readyProject(_ store: StoryProjectStore) async throws -> StoryProject {
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
    func characterProfile() -> StoryCharacterProfile {
        .init(isProtagonist: true, roleInStory: "主角", appearance: "短发", personality: "坚定", motivation: "完成任务",
              relationships: "独立行动", costume: "深色外套", consistencyNotes: "保持发型与服装一致")
    }
    func sceneProfile() -> StorySceneProfile {
        .init(roleInStory: "故事发生地", setting: "当代室内", spatialLayout: "门在北侧，窗在东侧",
              lightingAndPalette: "暖色", keyElements: "木桌", atmosphere: "安静", consistencyNotes: "保持空间方位一致")
    }
    func makeDetail() -> StorySegmentDetail {
        .init(firstFramePrompt: "first frame", shots: [.init(start: 0, end: 15, prompt: "slow tracking shot")],
              continuityIn: "enter", continuityOut: "exit", audio: "wind", constraints: "same character",
              lastFramePrompt: "last frame")
    }
    func fixture() async throws -> (StoryStudioViewModel, StoryProjectStore, StoryTestService) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryStudioTests-\(UUID().uuidString)", isDirectory: true)
        let store = StoryProjectStore(root: root)
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        let service = StoryTestService()
        let vm = StoryStudioViewModel(media: service, planner: service, store: store)
        vm.activate(userID: "alice"); try await idle(vm)
        return (vm, store, service)
    }
    func idle(_ vm: StoryStudioViewModel) async throws {
        for _ in 0..<300 where vm.isBusy || vm.isLoading || vm.hasActiveAssetGenerations
            || vm.hasActiveFrameGenerations || vm.hasActiveVideoGenerations {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(vm.isBusy); XCTAssertFalse(vm.isLoading)
        XCTAssertFalse(vm.hasActiveAssetGenerations); XCTAssertFalse(vm.hasActiveFrameGenerations)
        XCTAssertFalse(vm.hasActiveVideoGenerations)
    }
}
