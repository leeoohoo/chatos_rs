import AppKit
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import XCTest
@testable import ChatOSApp

extension StoryStudioTests {
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

}
