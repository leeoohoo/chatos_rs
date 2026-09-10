import ChatOSAgentRuntime
import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class StoryAgentTests: XCTestCase {
    func testRealLoopSavesWrittenProfilesAndMultipleSegmentsWithoutMediaCalls() async throws {
        let store = fixture()
        let project = project()
        try await store.save(project, owner: "alice")
        let run = try run(project)
        let session = StoryAgentSession(run: run, store: store, publish: { _ in })
        let model = ScriptedStoryModel(calls: outlineCalls())
        let result = try await execute(run, session: session, model: model)
        XCTAssertEqual(result.checkpoint.status, .completed, result.checkpoint.stopReason ?? "")
        let (_, applied) = try await store.applyRun(result, owner: "alice")
        XCTAssertEqual(applied.totalSeconds, 30)
        XCTAssertEqual(applied.characters.first?.profile.isProtagonist, true)
        XCTAssertEqual(applied.scenes.first?.profile.spatialLayout, "门在北侧，窗在东侧")
        XCTAssertTrue(applied.resources.allSatisfy { $0.images.isEmpty })
        XCTAssertTrue(applied.segments.allSatisfy { $0.video == nil && $0.attempt == nil })
        let loaded = try await store.loadRuns(owner: "alice", projectID: project.id)
        XCTAssertTrue(try XCTUnwrap(loaded.runs.first).applied)
        XCTAssertEqual(loaded.runs.first?.draft.characters, applied.characters)
        XCTAssertEqual(loaded.runs.first?.draft.scenes, applied.scenes)
        XCTAssertEqual(result.toolReceipts.count, outlineCalls().count)
        var graphRun = result
        let graph = try StoryAgentTools.execute(call("story_read_graph", ["nodeOffset": 0, "edgeOffset": 0, "limit": 50]), run: &graphRun)
        XCTAssertTrue(graph.content.contains("characterInScene"))
        XCTAssertTrue(graph.content.contains("segmentReferences"))
    }

    func testProjectSchemaEncodesNormalizedTablesAndGraphPagesHaveNoGaps() throws {
        var value = project()
        value.characters = [.init(id: "hero", name: "主角", profile: characterProfile())]
        value.scenes = [.init(id: "room", name: "旧屋", profile: sceneProfile())]
        value.props = [.init(id: "key", name: "钥匙", description: "一把旧铜钥匙")]
        value.segments = [.init(id: "s1", title: "归来", synopsis: "主角进入旧屋", sourceRange: .init(start: 0, end: 4),
                                characterIDs: ["hero"], sceneIDs: ["room"], propIDs: ["key"])]
        value.relations = [.init(id: "r1", segmentID: "s1", characterID: "hero", sceneID: "room",
                                 action: "走进房间", position: "北侧门口")]
        try value.validate()

        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any])
        XCTAssertNotNil(object["characters"]); XCTAssertNotNil(object["scenes"])
        XCTAssertNotNil(object["props"]); XCTAssertNotNil(object["segments"]); XCTAssertNotNil(object["relations"])
        XCTAssertNil(object["assets"])

        var nodeOffset = 0; var edgeOffset = 0
        var nodes: [StoryGraphNode] = []; var edges: [StoryGraphEdge] = []
        while true {
            let page = try value.graphPage(nodeOffset: nodeOffset, edgeOffset: edgeOffset, limit: 2)
            nodes += page.nodes; edges += page.edges
            guard page.nextNodeOffset != nil || page.nextEdgeOffset != nil else { break }
            nodeOffset = page.nextNodeOffset ?? page.nodeCount
            edgeOffset = page.nextEdgeOffset ?? page.edgeCount
        }
        XCTAssertEqual(Set(nodes.map(\.id)).count, nodes.count)
        XCTAssertEqual(Set(edges.map(\.id)).count, edges.count)
        XCTAssertTrue(edges.contains { $0.kind == .segmentReferences && $0.to == "character:hero" })
        XCTAssertTrue(edges.contains { $0.kind == .characterInScene && $0.segmentID == "s1" })
    }

    func testRelationValidationRejectsCrossTypeUnknownAndOverlappingScenes() throws {
        var value = project()
        value.characters = [.init(id: "hero", name: "主角", profile: characterProfile())]
        value.scenes = [
            .init(id: "room", name: "旧屋", profile: sceneProfile()),
            .init(id: "street", name: "街道", profile: sceneProfile()),
        ]
        value.segments = [.init(id: "s1", title: "移动", synopsis: "人物移动", sourceRange: .init(start: 0, end: 4),
                                characterIDs: ["hero"], sceneIDs: ["room", "street"])]
        value.relations = [
            .init(id: "r1", segmentID: "s1", characterID: "hero", sceneID: "room", action: "站立", position: "门口", startSecond: 0, endSecond: 10),
            .init(id: "r2", segmentID: "s1", characterID: "hero", sceneID: "street", action: "奔跑", position: "路面", startSecond: 9, endSecond: 15),
        ]
        XCTAssertThrowsError(try value.validate())
        value.relations.removeLast()
        value.segments[0].characterIDs = ["room"]
        XCTAssertThrowsError(try value.validate(), "Scene IDs cannot be stored in the character foreign-key column")
        value.segments[0].characterIDs = ["missing"]
        XCTAssertThrowsError(try value.validate())
    }

    func testInvalidBatchIsAtomicAndCannotSkipStoryEnding() async throws {
        let store = fixture(); let initial = try run(project())
        let session = StoryAgentSession(run: initial, store: store, publish: { _ in })
        _ = try await session.execute(call("story_read_source", ["offset": 0, "limit": 4]))
        let invalid = call("story_append_segments", ["segments": [segment("s1", 0, 2), segment("s2", 3, 4)]])
        let rejected = try await session.execute(invalid)
        XCTAssertTrue(rejected.isError)
        var saved = try await store.loadRuns(owner: "alice", projectID: initial.projectID).runs[0]
        XCTAssertTrue(saved.draft.segments.isEmpty)
        XCTAssertNil(saved.toolReceipts[invalid.id])
        _ = try await session.execute(call("story_append_segments", ["segments": [segment("s1", 0, 2)]]))
        _ = try await session.execute(call("story_save_summary", ["summary": "summary"]))
        let premature = try await session.execute(call("story_finish", [:]))
        XCTAssertTrue(premature.isError)
        saved = try await store.loadRuns(owner: "alice", projectID: initial.projectID).runs[0]
        XCTAssertEqual(saved.draft.segments.count, 1)
        XCTAssertThrowsError(try StoryAgentTools.validateCompletion(saved))
    }

    func testSceneAndCharacterProfilesRequireSourceAndCorrectKinds() async throws {
        let store = fixture(); let initial = try run(project())
        let session = StoryAgentSession(run: initial, store: store, publish: { _ in })
        let scene = call("story_save_scene_profile", sceneArguments)
        let unread = try await session.execute(scene)
        XCTAssertTrue(unread.isError)
        _ = try await session.execute(call("story_read_source", ["offset": 0, "limit": 4]))
        let skipped = try await session.execute(call("story_upsert_asset", ["id": "room", "kind": "scene", "name": "房间", "prompt": "not a profile"]))
        XCTAssertTrue(skipped.isError, "Scenes must use the dedicated written profile tool")
        let saved = try await session.execute(scene)
        XCTAssertFalse(saved.isError)
        var character = characterArguments; character["id"] = "room"
        let collision = try await session.execute(call("story_save_character_profile", character))
        XCTAssertTrue(collision.isError)
        var withImage = try await store.loadRuns(owner: "alice", projectID: initial.projectID).runs[0]
        withImage.draft.scenes[0].media.images = [.init(filename: "saved.png", mimeType: "image/png")]
        let guarded = StoryAgentSession(run: withImage, store: store, publish: { _ in })
        let overwrite = try await guarded.execute(call("story_save_scene_profile", sceneArguments))
        XCTAssertTrue(overwrite.isError, "Existing generated images must not be detached by a profile rewrite")
    }

    func testPersistedDomainReceiptRecoversInFlightWriteWithoutDuplicateSegment() async throws {
        let store = fixture(); let initial = try run(project())
        let session = StoryAgentSession(run: initial, store: store, publish: { _ in })
        _ = try await session.execute(call("story_read_source", ["offset": 0, "limit": 4]))
        let append = call("story_append_segments", ["segments": [segment("s1", 0, 4)]])
        _ = try await session.execute(append)
        var interrupted = try await store.loadRuns(owner: "alice", projectID: initial.projectID).runs[0]
        interrupted.checkpoint.pendingCalls = [append]
        interrupted.checkpoint.inFlightCallID = append.id
        interrupted.checkpoint.status = .needsReview
        interrupted.checkpoint.modelCalls = 9
        try await store.saveRun(interrupted, owner: "alice")
        let restarted = StoryAgentSession(run: interrupted, store: store, publish: { _ in })
        var policy = AgentRunPolicy(); policy.maximumModelCalls = 800
        let resumed = try await restarted.prepareForResume(policy: policy)
        XCTAssertNil(resumed.checkpoint.inFlightCallID)
        XCTAssertNotNil(resumed.checkpoint.receipts[append.id])
        XCTAssertEqual(resumed.policy.maximumModelCalls, 800)
        XCTAssertEqual(resumed.checkpoint.modelCalls, 9)
        _ = try await restarted.execute(append)
        let saved = try await store.loadRuns(owner: "alice", projectID: initial.projectID).runs[0]
        XCTAssertEqual(saved.draft.segments.count, 1)
        var forged = append; forged.arguments = "{}"
        do { _ = try await restarted.execute(forged); XCTFail("Receipt IDs cannot be reused for different arguments") } catch {}
    }

    func testApplyRejectsChangedProjectAndRecoversAppliedMarkerCrash() async throws {
        let store = fixture(); let original = project()
        var completed = try run(original)
        for call in outlineCalls() { _ = try StoryAgentTools.execute(call, run: &completed) }
        completed.checkpoint.status = .completed
        var edited = original; edited.title = "Manual edit"
        try await store.save(edited, owner: "alice")
        do { _ = try await store.applyRun(completed, owner: "alice"); XCTFail("Must preserve manual edit") } catch {}
        let preserved = try await store.load(owner: "alice")
        XCTAssertEqual(preserved.projects.first?.title, "Manual edit")
        try await store.save(completed.draft, owner: "alice") // Simulate crash before applied marker.
        let (applied, _) = try await store.applyRun(completed, owner: "alice")
        XCTAssertTrue(applied.applied)
        let (again, _) = try await store.applyRun(completed, owner: "alice")
        XCTAssertTrue(again.applied)
        let otherAccount = try await store.loadRuns(owner: "bob", projectID: original.id)
        XCTAssertTrue(otherAccount.runs.isEmpty)
    }

    func testRefinementOnlyWritesFrozenTargetsAndRequiresAllOfThem() async throws {
        let store = fixture(); var project = project()
        project.segments = ["s1", "s2", "s3"].enumerated().map {
            .init(id: $0.element, title: $0.element, synopsis: $0.element,
                  sourceRange: .init(start: $0.offset, end: $0.offset + 1))
        }
        let run = try StoryAgentRun(project: project, owner: "alice", stage: .refine, targetIDs: ["s1", "s2"], policy: .init())
        let session = StoryAgentSession(run: run, store: store, publish: { _ in })
        let forbidden = try await session.execute(call("story_update_segment", ["segmentID": "s3", "detail": detail]))
        XCTAssertTrue(forbidden.isError)
        _ = try await session.execute(call("story_update_segment", ["segmentID": "s1", "detail": detail]))
        let early = try await session.execute(call("story_finish", [:]))
        XCTAssertTrue(early.isError)
        var changed = detail; changed["firstFramePrompt"] = "different"
        let overwrite = try await session.execute(call("story_update_segment", ["segmentID": "s1", "detail": changed]))
        XCTAssertTrue(overwrite.isError)
        _ = try await session.execute(call("story_update_segment", ["segmentID": "s2", "detail": detail]))
        let done = try await session.execute(call("story_finish", [:]))
        XCTAssertFalse(done.isError)
        let profile = try await session.execute(call("story_save_scene_profile", sceneArguments))
        XCTAssertTrue(profile.isError)
    }

    func testLongGuidanceCanBeReadBeyondPreviewAndOldDraftSchemaIsRejected() throws {
        var project = project(); project.description = String(repeating: "a", count: 500) + "必须保留结局"
        let decoded = try JSONDecoder().decode(StoryProject.self, from: JSONEncoder().encode(project))
        XCTAssertEqual(decoded.version, 2)
        let old = Data(#"{"version":1,"title":"old","description":"old","models":{"textModelID":"text","imageModelID":"image","videoModelID":"video"},"assets":[],"segments":[]}"#.utf8)
        XCTAssertThrowsError(try JSONDecoder().decode(StoryProject.self, from: old))
        var state = try run(project)
        let result = try StoryAgentTools.execute(call("story_read_text", ["field": "description", "offset": 500, "limit": 100]), run: &state)
        XCTAssertTrue(result.content.contains("必须保留结局"))
        let definitions = try StoryAgentTools.definitions(stage: .outline)
        XCTAssertFalse(definitions.contains { $0.effect == .billable })
        XCTAssertLessThan(try AgentContextBudget.estimate(messages: state.checkpoint.messages, tools: definitions), AgentContextPolicy().hardInputLimit)
    }

    func testViewModelUsesVisibleBudgetAndResumesWithMemoryByDefault() async throws {
        let store = fixture(); let project = project()
        let suite = "StoryAgentTests-\(UUID())"
        addTeardownBlock { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let settings = AgentSettingsStore(suiteName: suite)
        var preferences = AgentRuntimePreferences(); preferences.storyMaximumCalls = 2
        try settings.save(preferences)
        let service = StoryLoopServices(calls: outlineCalls())
        let vm = StoryStudioViewModel(media: service, planner: service, store: store, agentSettings: settings)
        vm.activate(userID: "alice"); try await idle(vm)
        let created = await vm.create(project, availableModels: models)
        XCTAssertTrue(created); XCTAssertTrue(vm.supportsAgentPlanning)
        vm.planOutline(); try await idle(vm)
        XCTAssertEqual(vm.latestAgentRun?.checkpoint.status, .limitReached)
        XCTAssertEqual(vm.latestAgentRun?.cloudMemory, true)
        XCTAssertEqual(vm.latestAgentRun?.checkpoint.modelCalls, 2)
        XCTAssertTrue(vm.project?.segments.isEmpty == true, "Unfinished draft must not replace canonical project")
        let id = try XCTUnwrap(vm.latestAgentRun?.id)
        preferences.storyMaximumCalls = 600; try settings.save(preferences)
        let reopened = StoryStudioViewModel(media: service, planner: service, store: store, agentSettings: settings)
        reopened.activate(userID: "alice"); try await idle(reopened)
        reopened.open(project.id); try await idle(reopened)
        XCTAssertEqual(reopened.latestAgentRun?.id, id)
        reopened.resumeAgent(id); try await idle(reopened)
        XCTAssertTrue(reopened.latestAgentRun?.applied == true, reopened.errorMessage ?? "")
        XCTAssertEqual(reopened.project?.totalSeconds, 30)
        let events = await service.events()
        XCTAssertEqual(events, ["memory", "model:text:2", "memory", "model:text:600"])
    }

    func testViewModelCloudConsentBindsScopeAndCompletedDraftAppliesOffline() async throws {
        let store = fixture(); let project = project()
        let service = StoryLoopServices(calls: outlineCalls())
        let vm = StoryStudioViewModel(media: service, planner: service, store: store)
        vm.activate(userID: "alice"); try await idle(vm)
        _ = await vm.create(project, availableModels: models)
        vm.planOutline(); try await idle(vm)
        var completed = try XCTUnwrap(vm.latestAgentRun)
        XCTAssertTrue(completed.applied, vm.errorMessage ?? "")
        let scopes = await service.scopes()
        XCTAssertEqual(scopes.count, 1)
        XCTAssertEqual(scopes.first?.tenantID, "alice")
        XCTAssertEqual(scopes.first?.sourceID, "chatos")
        XCTAssertEqual(scopes.first?.runID, completed.id)
        XCTAssertTrue(scopes.first?.subjectID.contains(project.id.uuidString) == true)
        let synced = await service.memory.entries
        XCTAssertTrue(synced.contains { $0.message.content.contains("没有生成图片") })
        completed.applied = false
        try await store.saveRun(completed, owner: "alice")
        let before = await service.events()
        vm.resumeAgent(completed.id); try await idle(vm)
        let after = await service.events()
        XCTAssertEqual(after, before, "Completed draft application must not need a model or Memory Engine request")
    }

    func testAccountSwitchIgnoresLateModelResponse() async throws {
        let store = fixture(); let service = StoryLoopServices(calls: outlineCalls(), delay: true)
        let vm = StoryStudioViewModel(media: service, planner: service, store: store)
        vm.activate(userID: "alice"); try await idle(vm)
        _ = await vm.create(project(), availableModels: models)
        vm.planOutline()
        for _ in 0..<100 {
            if await service.model.hasStarted { break }
            try await Task.sleep(for: .milliseconds(5))
        }
        vm.activate(userID: "bob"); try await idle(vm)
        try await Task.sleep(for: .milliseconds(150))
        XCTAssertTrue(vm.projects.isEmpty)
        XCTAssertTrue(vm.agentRuns.isEmpty)
        let saved = try await store.load(owner: "bob")
        XCTAssertTrue(saved.projects.isEmpty)
    }

    private var models: [MediaGenerationModel] {
        ["text", "image", "video"].map { .init(id: $0, name: $0, provider: "gpt", modelName: $0, enabled: true, taskEnabled: false, hasAPIKey: true) }
    }
    private func idle(_ vm: StoryStudioViewModel) async throws {
        for _ in 0..<500 where vm.isBusy || vm.isLoading || vm.isLoadingAgentRuns { try await Task.sleep(for: .milliseconds(10)) }
        XCTAssertFalse(vm.isBusy); XCTAssertFalse(vm.isLoading); XCTAssertFalse(vm.isLoadingAgentRuns)
    }
    private func fixture() -> StoryProjectStore {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("StoryAgentTests-\(UUID())")
        addTeardownBlock { if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) } }
        return StoryProjectStore(root: root)
    }
    private func project() -> StoryProject {
        var project = StoryProject(title: "故事", description: "从开头到结局", models: .init(textModelID: "text", imageModelID: "image", videoModelID: "video"))
        project.source = "起承转合"; return project
    }
    private func run(_ project: StoryProject) throws -> StoryAgentRun {
        try .init(project: project, owner: "alice", stage: .outline, targetIDs: [], policy: .init())
    }
    private func call(_ name: String, _ arguments: [String: Any]) -> AgentToolCall {
        .init(id: UUID().uuidString, name: name, arguments: String(decoding: try! JSONSerialization.data(withJSONObject: arguments, options: [.sortedKeys]), as: UTF8.self))
    }
    private func segment(_ id: String, _ start: Int, _ end: Int, refs: [String] = []) -> [String: Any] {
        ["id": id, "title": id, "synopsis": "镜头情节", "sourceStart": start, "sourceEnd": end]
    }
    private var characterArguments: [String: Any] {
        ["id": "hero", "name": "主角", "profile": ["isProtagonist": true, "roleInStory": "寻找归途的人", "appearance": "原文未说明，视觉设定建议短发", "personality": "坚毅", "motivation": "回家", "relationships": "旅人", "costume": "原文未说明，视觉设定建议灰色外套", "consistencyNotes": "外套与发型保持一致"]]
    }
    private var sceneArguments: [String: Any] {
        ["id": "room", "name": "旧屋", "profile": ["roleInStory": "归途终点", "setting": "原文未说明，视觉设定建议当代旧屋", "spatialLayout": "门在北侧，窗在东侧", "lightingAndPalette": "暖色夕阳", "keyElements": "木桌", "atmosphere": "安静", "consistencyNotes": "所有镜头保持门窗方位一致"]]
    }
    private func characterProfile() -> StoryCharacterProfile {
        .init(isProtagonist: true, roleInStory: "主角", appearance: "短发", personality: "坚定", motivation: "回家",
              relationships: "独自行动", costume: "灰色外套", consistencyNotes: "保持发型与服装一致")
    }
    private func sceneProfile() -> StorySceneProfile {
        .init(roleInStory: "故事发生地", setting: "当代旧屋", spatialLayout: "门在北侧，窗在东侧",
              lightingAndPalette: "暖色夕阳", keyElements: "木桌", atmosphere: "安静", consistencyNotes: "保持门窗方位一致")
    }
    private var detail: [String: Any] {
        ["firstFramePrompt": "wide room", "shots": [["start": 0, "end": 15, "prompt": "tracking shot"]], "continuityIn": "enter", "continuityOut": "exit", "audio": "wind", "constraints": "same room"]
    }
    private func outlineCalls() -> [AgentToolCall] {
        [call("story_read_source", ["offset": 0, "limit": 4]), call("story_save_summary", ["summary": "完整剧情"]),
         call("story_save_character_profile", characterArguments), call("story_save_scene_profile", sceneArguments),
         call("story_append_segments", ["segments": [segment("s1", 0, 2, refs: ["hero", "room"]), segment("s2", 2, 4, refs: ["hero", "room"])]]),
         call("story_save_segment_relations", ["segmentID": "s1", "characterIDs": ["hero"], "sceneIDs": ["room"], "propIDs": [], "relations": [["relationID": "s1-hero-room", "characterID": "hero", "sceneID": "room", "action": "走进房间", "position": "北侧门口", "startSecond": 0, "endSecond": 15]]]),
         call("story_save_segment_relations", ["segmentID": "s2", "characterIDs": ["hero"], "sceneIDs": ["room"], "propIDs": [], "relations": [["relationID": "s2-hero-room", "characterID": "hero", "sceneID": "room", "action": "走向窗户", "position": "东侧窗边", "startSecond": 0, "endSecond": 15]]]),
         call("story_finish", [:])]
    }
    private func execute(_ run: StoryAgentRun, session: StoryAgentSession, model: ScriptedStoryModel) async throws -> StoryAgentRun {
        let checkpoint = try await AgentRuntime().run(checkpoint: run.checkpoint, scope: run.checkpoint.scope, policy: run.policy,
            model: model, tools: StoryAgentTools.definitions(stage: run.stage), execute: { try await session.execute($0) },
            record: { try await session.record($0, event: $1) })
        return try await session.finish(checkpoint)
    }
}

private actor ScriptedStoryModel: AgentModelClient {
    var calls: [AgentToolCall]
    let delay: Bool
    var hasStarted = false
    init(calls: [AgentToolCall], delay: Bool = false) { self.calls = calls; self.delay = delay }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        hasStarted = true
        if delay { try? await Task.sleep(for: .milliseconds(100)) }
        guard !calls.isEmpty else { throw AgentRuntimeError.invalidResponse }
        return .init(role: .assistant, toolCalls: [calls.removeFirst()])
    }
}

private actor StoryLoopServices: AgentServiceProviding, StoryPlanningServicing, MediaGenerationServicing {
    let model: ScriptedStoryModel
    let memory = StoryLoopMemory()
    var log: [String] = []
    var memoryScopes: [AgentMemoryScope] = []
    init(calls: [AgentToolCall], delay: Bool = false) { model = .init(calls: calls, delay: delay) }
    func events() -> [String] { log }
    func scopes() -> [AgentMemoryScope] { memoryScopes }
    func makeAgentModel(configID: String, policy: AgentRunPolicy) async throws -> any AgentModelClient {
        log.append("model:\(configID):\(policy.maximumModelCalls)"); return model
    }
    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        log.append("memory"); memoryScopes.append(scope); return memory
    }
    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func plan(_ request: StoryPlanningRequest) async throws -> Data { log.append("legacy"); throw StoryError.unavailable }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult { log.append("image"); throw StoryError.unavailable }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        log.append("video"); throw StoryError.unavailable
    }
}

private actor StoryLoopMemory: AgentMemoryServicing {
    var entries: [AgentMemoryEntry] = []
    func ensureThread() async throws {}
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws { self.entries += entries }
    func compose() async throws -> AgentMemoryContext { .init(summaries: [], recentRecordIDs: entries.map(\.id)) }
    func startSummary(reason: String) async throws -> AgentSummaryStatus { throw AgentContextError.summaryFailed(nil) }
    func summaryStatus(jobID: String?) async throws -> AgentSummaryStatus { throw AgentContextError.summaryFailed(nil) }
}
