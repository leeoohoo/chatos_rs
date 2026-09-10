import ChatOSCore
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
        XCTAssertEqual(vm.projects.first?.models, draft.models)
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

    func testBatchFailureSavesTaskAndResumeDoesNotResubmit() async throws {
        let (vm, store, service) = try await fixture()
        await service.setVideoFailure(true)
        let draft = try await readyProject(store)
        _ = await vm.create(draft, availableModels: models)
        vm.selectedSegments = ["s1", "s2"]
        vm.generateBatch(availableModels: models); try await idle(vm)
        XCTAssertEqual(vm.project?.segments[0].attempt?.jobID, "remote-job")
        XCTAssertNil(vm.project?.segments[1].attempt, "Failure must pause later requests")
        let persisted = try await store.load(owner: "alice")
        XCTAssertEqual(persisted.projects[0].segments[0].attempt?.jobID, "remote-job")
        vm.generateBatch(availableModels: models); try await idle(vm)
        // s1 remains locked; only s2 was eligible for a new submission.
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
        let segment = StorySegment(id: "s1", title: "进屋", synopsis: "主角进屋", sourceRange: .init(start: 0, end: 1),
                                   characterIDs: ["hero"], sceneIDs: ["room"])
        project.segments = [segment]
        project.relations = [.init(id: "r1", segmentID: "s1", characterID: "hero", sceneID: "room",
                                   action: "推门进入", position: "门口")]
        let context = try StoryGenerationContext.text(project, segment: segment)
        XCTAssertTrue(context.contains("hero")); XCTAssertTrue(context.contains("room")); XCTAssertTrue(context.contains("推门进入"))
        XCTAssertFalse(context.contains("villain")); XCTAssertFalse(context.contains("yard"))
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
        let batch = try StoryMediaBatch(project: project, owner: "alice", kind: .pipeline, targets: ["s1"], models: models)
        XCTAssertEqual(batch.steps.map(\.kind), [.assets, .assets, .frames, .videos])
        XCTAssertEqual(batch.steps.map(\.targetID), ["hero", "room", "s1", "s1"])
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
        project.segments = [.init(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))]
        let detail = try StoryPlanningTools.detailRequest(project, segmentID: "s1")
        XCTAssertEqual(detail.toolName, "story_update_segment")
        XCTAssertThrowsError(try StoryPlanningTools.detailRequest(project, segmentID: "another-project-id"))
    }

    func testPendingVideoConfigurationCannotBeChanged() async throws {
        let (vm, _, _) = try await fixture()
        var draft = makeProject()
        var segment = StorySegment(id: "s1", title: "A", synopsis: "A", sourceRange: .init(start: 0, end: 1))
        segment.attempt = .init(modelConfigID: "video", prompt: "original", size: "768P", ratio: "16:9")
        draft.segments = [segment]
        _ = await vm.create(draft, availableModels: models)
        draft.models.videoModelID = "text"
        let changed = await vm.updateSettings(draft, availableModels: models)
        XCTAssertFalse(changed)
        XCTAssertEqual(vm.project?.models.videoModelID, "video")
        vm.attachVerifiedJobID("original-id", segmentID: "s1"); try await idle(vm)
        XCTAssertEqual(vm.project?.segments[0].attempt?.jobID, "original-id")
        XCTAssertEqual(vm.project?.segments[0].attempt?.prompt, "original")
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
              continuityIn: "enter", continuityOut: "exit", audio: "wind", constraints: "same character")
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
        for _ in 0..<300 where vm.isBusy || vm.isLoading { try await Task.sleep(for: .milliseconds(10)) }
        XCTAssertFalse(vm.isBusy); XCTAssertFalse(vm.isLoading)
    }
}

private actor StoryTestService: ResumableVideoGenerationServicing, StoryPlanningServicing {
    private var calls: [String] = []
    private var failVideo = false
    private var delayed = false
    func events() -> [String] { calls }
    func setVideoFailure(_ value: Bool) { failVideo = value }
    func setDelay(_ value: Bool) { delayed = value }
    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func plan(_ request: StoryPlanningRequest) async throws -> Data {
        calls.append(request.toolName)
        if delayed { try await Task.sleep(for: .milliseconds(50)) }
        if request.toolName == "story_suggest_optimized_text" {
            return Data(#"{"optimizedText":"Improved candidate","rationale":"Clearer pacing"}"#.utf8)
        }
        if request.toolName == "story_save_outline" {
            return Data(#"{"summary":"beginning to ending","props":[],"segments":[{"id":"s1","title":"opening","synopsis":"start","propIDs":[]},{"id":"s2","title":"middle","synopsis":"middle","propIDs":[]},{"id":"s3","title":"ending","synopsis":"end","propIDs":[]}]}"#.utf8)
        }
        return Data(#"{"firstFramePrompt":"a girl","shots":[{"start":0,"end":15,"prompt":"tracking shot"}],"continuityIn":"enter","continuityOut":"exit","audio":"wind","constraints":"same character"}"#.utf8)
    }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        calls.append("image"); throw URLError(.badServerResponse)
    }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        calls.append("video")
        await progress(.init(status: "queued", jobID: "remote-job"))
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
