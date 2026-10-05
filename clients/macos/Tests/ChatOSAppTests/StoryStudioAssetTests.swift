import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSApp

extension StoryStudioTests {
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

}
