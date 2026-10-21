import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story media batch application boundary")
struct StoryMediaBatchUseCaseTests {
    @Test("preparation normalizes owner and ideas while preserving the frozen batch identity")
    func preparesBatch() throws {
        let project = Self.project()
        let batchID = UUID()
        let step = Self.step()
        let preparation = StoryMediaBatchPreparation(
            batchID: batchID,
            ownerUserID: "alice",
            projectID: project.id,
            projectFingerprint: try StoryProjectFingerprint.current(project),
            kind: .assets,
            requestedTargetIDs: ["hero"],
            steps: [step],
            executionMode: .persistentBatch
        )
        let executor = StubStoryMediaBatchExecutor(
            preparation: preparation,
            expectedOwner: "alice",
            expectedIdeas: String(repeating: "x", count: 2_000)
        )
        let useCase = StoryMediaBatchUseCase(executor: executor)

        let result = try useCase.prepare(
            ownerUserID: "  alice  ",
            project: project,
            kind: .assets,
            targetIDs: ["hero"],
            availableModels: [],
            userIdeas: "  " + String(repeating: "x", count: 2_010) + "  "
        )

        #expect(result.batchID == batchID)
        #expect(result.steps == [step])
        #expect(result.executionMode == .persistentBatch)
    }

    @Test("candidate selection keeps existing Story eligibility rules in Core")
    func selectsCandidates() {
        var project = Self.project()
        project.props = [
            .init(id: "hero", name: "Hero", description: "Hero"),
            .init(id: "scene", name: "Scene", description: "Scene"),
        ]
        project.props[1].media.generationAttemptID = UUID()
        var ready = StorySegment(
            id: "ready",
            title: "Ready",
            synopsis: "Ready",
            sourceRange: .init(start: 0, end: 1)
        )
        ready.detail = .init(
            firstFramePrompt: "frame",
            shots: [.init(start: 0, end: 15, prompt: "shot")],
            continuityIn: "in",
            continuityOut: "out",
            audio: "",
            constraints: ""
        )
        project.segments = [ready]
        let useCase = StoryMediaBatchUseCase(
            executor: StubStoryMediaBatchExecutor(preparation: nil)
        )

        #expect(useCase.eligibleTargetIDs(project: project, kind: .assets) == ["hero"])
        #expect(useCase.eligibleTargetIDs(project: project, kind: .pipeline) == ["ready"])
        #expect(useCase.eligibleTargetIDs(project: project, kind: .videos).isEmpty)
    }

    @Test("execution drops foreign progress and verifies the terminal identity")
    func validatesExecutionIdentity() async throws {
        let project = Self.project()
        let batchID = UUID()
        let preparation = StoryMediaBatchPreparation(
            batchID: batchID,
            ownerUserID: "alice",
            projectID: project.id,
            projectFingerprint: try StoryProjectFingerprint.current(project),
            kind: .assets,
            requestedTargetIDs: ["hero"],
            steps: [Self.step()],
            executionMode: .persistentBatch
        )
        let valid = Self.snapshot(
            id: batchID,
            owner: "alice",
            projectID: project.id
        )
        let foreign = Self.snapshot(
            id: batchID,
            owner: "bob",
            projectID: project.id
        )
        let executor = StubStoryMediaBatchExecutor(
            preparation: preparation,
            events: [
                .init(batch: foreign, project: project),
                .init(batch: valid, project: project),
            ],
            outcome: .init(batch: valid, project: project)
        )
        let collector = BatchEventCollector()
        let outcome = try await StoryMediaBatchUseCase(executor: executor).execute(
            ownerUserID: "alice",
            command: .start(
                project: project,
                preparation: preparation,
                availableModels: [],
                userIdeas: ""
            ),
            continuationCheck: {},
            shouldPause: { false },
            onEvent: { await collector.append($0) }
        )

        #expect(outcome.batch == valid)
        #expect(await collector.events.map(\.batch) == [valid])
    }

    @Test("direct video preparations cannot enter the persisted batch executor")
    func rejectsDirectVideoExecution() async {
        let project = Self.project()
        let preparation = StoryMediaBatchPreparation(
            batchID: UUID(),
            ownerUserID: "alice",
            projectID: project.id,
            projectFingerprint: (try? StoryProjectFingerprint.current(project)) ?? "",
            kind: .videos,
            requestedTargetIDs: ["segment"],
            steps: [
                .init(
                    id: "videos:segment",
                    kind: .videos,
                    targetID: "segment",
                    completed: false
                ),
            ],
            executionMode: .directVideos
        )
        let useCase = StoryMediaBatchUseCase(
            executor: StubStoryMediaBatchExecutor(preparation: preparation)
        )

        await #expect(throws: StoryMediaBatchUseCaseError.directVideosRequireSeparateExecution) {
            _ = try await useCase.execute(
                ownerUserID: "alice",
                command: .start(
                    project: project,
                    preparation: preparation,
                    availableModels: [],
                    userIdeas: ""
                ),
                continuationCheck: {},
                shouldPause: { false },
                onEvent: { _ in }
            )
        }
    }

    @Test("operation lifecycle rejects replaced account, session, and project identities")
    func validatesLifecycle() {
        var state = StoryMediaBatchExecutionState()
        let session = UUID()
        let projectID = UUID()
        let first = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .resume(projectID: projectID, batchID: UUID())
        )
        #expect(first != nil)
        #expect(state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .abandon(projectID: projectID, batchID: UUID())
        ) == nil)
        guard let first else { return }
        #expect(state.accepts(
            first,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(!state.accepts(
            first,
            ownerUserID: "bob",
            accountSessionID: session,
            projectID: projectID
        ))
        #expect(!state.accepts(
            first,
            ownerUserID: "alice",
            accountSessionID: UUID(),
            projectID: projectID
        ))
        #expect(!state.accepts(
            first,
            ownerUserID: "alice",
            accountSessionID: session,
            projectID: UUID()
        ))
        let didFinish = state.finish(first)
        #expect(didFinish)
        #expect(state.activeToken == nil)
    }

    private static func project() -> StoryProject {
        var project = StoryProject(
            title: "Story",
            description: "",
            models: .init(
                textModelID: "text",
                imageModelID: "image",
                videoModelID: "video"
            )
        )
        project.source = "Story"
        return project
    }

    private static func step() -> StoryMediaBatchStepSnapshot {
        .init(
            id: "assets:hero",
            kind: .assets,
            targetID: "hero",
            completed: false
        )
    }

    private static func snapshot(
        id: UUID,
        owner: String,
        projectID: UUID
    ) -> StoryMediaBatchSnapshot {
        .init(
            id: id,
            ownerUserID: owner,
            projectID: projectID,
            kind: .assets,
            status: .running,
            steps: [step()],
            events: [],
            consentAt: .now,
            error: nil,
            updatedAt: .now
        )
    }
}

private struct StubStoryMediaBatchExecutor: StoryMediaBatchExecuting {
    let preparation: StoryMediaBatchPreparation?
    var events: [StoryMediaBatchExecutionEvent] = []
    var outcome: StoryMediaBatchExecutionOutcome?
    var expectedOwner: String?
    var expectedIdeas: String?

    func prepareStoryMediaBatch(
        ownerUserID: String,
        project: StoryProject,
        kind: StoryMediaBatchKind,
        targetIDs: [String],
        availableModels: [MediaGenerationModel],
        userIdeas: String
    ) throws -> StoryMediaBatchPreparation {
        if let expectedOwner, ownerUserID != expectedOwner {
            throw StubStoryMediaBatchError.unexpectedRequest
        }
        if let expectedIdeas, userIdeas != expectedIdeas {
            throw StubStoryMediaBatchError.unexpectedRequest
        }
        guard let preparation else { throw StubStoryMediaBatchError.unexpectedRequest }
        return preparation
    }

    func executeStoryMediaBatch(
        ownerUserID: String,
        command: StoryMediaBatchCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryMediaBatchExecutionEvent) async -> Void
    ) async throws -> StoryMediaBatchExecutionOutcome {
        for event in events { await onEvent(event) }
        guard let outcome else { throw StubStoryMediaBatchError.unexpectedRequest }
        return outcome
    }
}

private actor BatchEventCollector {
    var events: [StoryMediaBatchExecutionEvent] = []

    func append(_ event: StoryMediaBatchExecutionEvent) {
        events.append(event)
    }
}

private enum StubStoryMediaBatchError: Error {
    case unexpectedRequest
}
