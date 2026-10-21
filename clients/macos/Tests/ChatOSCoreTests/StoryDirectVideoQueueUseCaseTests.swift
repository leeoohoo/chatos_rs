import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story direct video queue application boundary")
struct StoryDirectVideoQueueUseCaseTests {
    @Test("preparation freezes story order, model capabilities, and bounded guidance")
    func preparesQueue() throws {
        let project = Self.readyProject()
        let preparation = try Self.preparation(project: project, targets: ["s2", "s1"])

        let plan = try StoryDirectVideoQueueUseCase().prepare(
            ownerUserID: "  alice  ",
            project: project,
            preparation: preparation,
            availableModels: Self.models,
            userIdeas: "  " + String(repeating: "x", count: 2_010) + "  "
        )

        #expect(plan.queueID == preparation.batchID)
        #expect(plan.targetIDs == ["s1", "s2"])
        #expect(plan.model.id == "video")
        #expect(plan.size == "768P")
        #expect(plan.userIdeas.count == 2_000)
    }

    @Test("preparation rejects changed projects and unavailable reference videos")
    func validatesPreparation() throws {
        var project = Self.readyProject()
        let preparation = try Self.preparation(project: project, targets: ["s2"])
        project.style = "changed"
        #expect(throws: StoryDirectVideoQueueError.invalidPreparation) {
            _ = try StoryDirectVideoQueueUseCase().prepare(
                ownerUserID: "alice",
                project: project,
                preparation: preparation,
                availableModels: Self.models,
                userIdeas: ""
            )
        }

        project = Self.readyProject()
        project.segments[1].videoGuidanceMode = .previousVideo
        let missingReference = try Self.preparation(project: project, targets: ["s2"])
        #expect(throws: StoryDirectVideoQueueError.missingPreviousVideos(["Second"])) {
            _ = try StoryDirectVideoQueueUseCase().prepare(
                ownerUserID: "alice",
                project: project,
                preparation: missingReference,
                availableModels: Self.models,
                userIdeas: ""
            )
        }
    }

    @Test("execution starts every target strictly in frozen story order")
    @MainActor
    func executesSequentially() async throws {
        let current = Self.readyProject()
        let preparation = try Self.preparation(project: current, targets: ["s2", "s1"])
        let useCase = StoryDirectVideoQueueUseCase()
        let plan = try useCase.prepare(
            ownerUserID: "alice",
            project: current,
            preparation: preparation,
            availableModels: Self.models,
            userIdeas: "camera guidance"
        )
        var observed: [String] = []
        var checks = 0

        try await useCase.execute(
            plan,
            currentProject: { current },
            continuationCheck: { checks += 1 },
            executeStep: { _, step in
                observed.append(step.segmentID)
                #expect(step.index == observed.count - 1)
                #expect(step.totalCount == 2)
                try await Task.sleep(for: .milliseconds(1))
            }
        )

        #expect(observed == ["s1", "s2"])
        #expect(checks == 5)
    }

    @Test("execution rejects a project replacement before the first billable step")
    @MainActor
    func rejectsChangedProjectAtExecution() async throws {
        let original = Self.readyProject()
        let preparation = try Self.preparation(project: original, targets: ["s1"])
        let useCase = StoryDirectVideoQueueUseCase()
        let plan = try useCase.prepare(
            ownerUserID: "alice",
            project: original,
            preparation: preparation,
            availableModels: Self.models,
            userIdeas: ""
        )
        var changed = original
        changed.style = "changed after preview"
        var executed = false

        await #expect(throws: StoryDirectVideoQueueError.projectChanged) {
            try await useCase.execute(
                plan,
                currentProject: { changed },
                continuationCheck: {},
                executeStep: { _, _ in executed = true }
            )
        }
        #expect(!executed)
    }

    @Test("queue lifecycle rejects replaced account and session identities")
    func validatesLifecycle() {
        var state = StoryDirectVideoQueueState()
        let session = UUID()
        let projectID = UUID()
        let queueID = UUID()
        let token = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID,
            queueID: queueID
        )
        #expect(token != nil)
        #expect(state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID,
            queueID: UUID()
        ) == nil)
        guard let token else { return }
        #expect(state.accepts(
            token,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(!state.accepts(
            token,
            ownerUserID: "bob",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(!state.accepts(
            token,
            ownerUserID: "alice",
            accountSessionID: UUID(),
            projectID: projectID
        ))
        let didFinish = state.finish(token)
        #expect(didFinish)
        #expect(state.activeToken == nil)
    }

    private static var models: [MediaGenerationModel] {
        [
            .init(
                id: "video",
                name: "Video",
                provider: "gpt",
                modelName: "MiniMax-H3",
                enabled: true,
                taskEnabled: false,
                hasAPIKey: true
            ),
        ]
    }

    private static func readyProject() -> StoryProject {
        var project = StoryProject(
            title: "Story",
            description: "",
            models: .init(
                textModelID: "text",
                imageModelID: "image",
                videoModelID: "video"
            )
        )
        project.source = "ab"
        project.segments = [
            readySegment(id: "s1", title: "First", range: .init(start: 0, end: 1)),
            readySegment(id: "s2", title: "Second", range: .init(start: 1, end: 2)),
        ]
        return project
    }

    private static func readySegment(
        id: String,
        title: String,
        range: StorySourceRange
    ) -> StorySegment {
        var segment = StorySegment(
            id: id,
            title: title,
            synopsis: title,
            sourceRange: range
        )
        segment.detail = .init(
            firstFramePrompt: "frame",
            shots: [.init(start: 0, end: 15, prompt: "shot")],
            continuityIn: "in",
            continuityOut: "out",
            audio: "",
            constraints: ""
        )
        let frame = StoryImage(filename: "\(id).png", mimeType: "image/png")
        segment.firstFrames.images = [frame]
        segment.confirmedFrameID = frame.id
        return segment
    }

    private static func preparation(
        project: StoryProject,
        targets: [String]
    ) throws -> StoryMediaBatchPreparation {
        StoryMediaBatchPreparation(
            batchID: UUID(),
            ownerUserID: "alice",
            projectID: project.id,
            projectFingerprint: try StoryProjectFingerprint.current(project),
            kind: .videos,
            requestedTargetIDs: targets,
            steps: targets.map {
                .init(
                    id: "videos:\($0)",
                    kind: .videos,
                    targetID: $0,
                    completed: false
                )
            },
            executionMode: .directVideos
        )
    }
}
