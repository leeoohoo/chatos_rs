import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSApp

extension StoryStudioTests {
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
