import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story planning application boundary")
struct StoryPlanningUseCaseTests {
    @Test("start trims owner and bounds user ideas before entering the executor")
    func normalizesStartCommand() async throws {
        let project = Self.project()
        let snapshot = Self.snapshot(project: project, owner: "alice")
        let executor = RecordingStoryPlanningExecutor(
            outcome: .init(run: snapshot)
        )
        let checks = StoryPlanningCheckCounter()
        let events = StoryPlanningEventRecorder()
        let useCase = StoryPlanningUseCase(executor: executor)
        let ideas = "  " + String(repeating: "想", count: 2_020) + "  "

        let outcome = try await useCase.execute(
            ownerUserID: "  alice  ",
            command: .start(
                project: project,
                stage: .outline,
                targetIDs: [],
                userIdeas: ideas
            ),
            continuationCheck: { await checks.record() },
            shouldPause: { false },
            onEvent: { await events.record($0) }
        )

        #expect(outcome.run == snapshot)
        #expect(await checks.count == 3)
        #expect(await events.values == [.run(snapshot)])
        let requests = await executor.requests
        #expect(requests.count == 1)
        #expect(requests.first?.ownerUserID == "alice")
        guard case let .start(_, stage, targetIDs, userIdeas) = requests.first?.command else {
            Issue.record("Expected a normalized start command")
            return
        }
        #expect(stage == .outline)
        #expect(targetIDs.isEmpty)
        #expect(userIdeas == String(repeating: "想", count: 2_000))
    }

    @Test("invalid owner and invalid project fail before the executor")
    func rejectsInvalidInput() async {
        let project = Self.project()
        let executor = RecordingStoryPlanningExecutor(
            outcome: .init(run: Self.snapshot(project: project, owner: "alice"))
        )
        let useCase = StoryPlanningUseCase(executor: executor)

        await #expect(throws: StoryPlanningError.invalidOwner) {
            _ = try await useCase.execute(
                ownerUserID: "   ",
                command: .resume(project: project, runID: UUID()),
                continuationCheck: {},
                shouldPause: { false },
                onEvent: { _ in }
            )
        }
        var invalid = project
        invalid.title = ""
        await #expect(throws: StoryError.self) {
            _ = try await useCase.execute(
                ownerUserID: "alice",
                command: .start(
                    project: invalid,
                    stage: .outline,
                    targetIDs: [],
                    userIdeas: ""
                ),
                continuationCheck: {},
                shouldPause: { false },
                onEvent: { _ in }
            )
        }
        #expect(await executor.requests.isEmpty)
    }

    @Test("executor results must match the frozen owner and project")
    func rejectsMismatchedOutcome() async {
        let project = Self.project()
        let executor = RecordingStoryPlanningExecutor(
            outcome: .init(run: Self.snapshot(project: project, owner: "mallory"))
        )
        let events = StoryPlanningEventRecorder()
        let useCase = StoryPlanningUseCase(executor: executor)

        await #expect(throws: StoryPlanningError.identityMismatch) {
            _ = try await useCase.execute(
                ownerUserID: "alice",
                command: .resume(project: project, runID: UUID()),
                continuationCheck: {},
                shouldPause: { false },
                onEvent: { await events.record($0) }
            )
        }
        #expect(await events.values.isEmpty)
    }

    @Test("project fingerprint ignores timestamps and capability snapshots but reads legacy values")
    func fingerprintsProject() throws {
        var original = Self.project()
        original.models.supportedVideoDurations = [4, 8, 12]
        var refreshed = original
        refreshed.updatedAt = Date(timeIntervalSince1970: 9_999)
        refreshed.models.supportedVideoDurations = Array(4...15)

        let current = try StoryProjectFingerprint.current(original)
        let refreshedCurrent = try StoryProjectFingerprint.current(refreshed)
        #expect(current == refreshedCurrent)
        let legacy = try StoryProjectFingerprint.legacyIncludingCapabilities(original)
        #expect(legacy != current)
        let currentMatches = try StoryProjectFingerprint.matches(current, project: refreshed)
        let legacyMatches = try StoryProjectFingerprint.matches(legacy, project: original)
        #expect(currentMatches)
        #expect(legacyMatches)
    }

    @Test("typed lifecycle rejects replaced accounts and concurrent planning operations")
    func tracksExecutionLifecycle() {
        var state = StoryPlanningExecutionState()
        let session = UUID()
        let projectID = UUID()
        let runID = UUID()
        let request = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .resume(projectID: projectID, runID: runID)
        )
        #expect(request != nil)
        #expect(state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            kind: .abandon(projectID: projectID, runID: runID)
        ) == nil)
        if let request {
            #expect(state.accepts(request, ownerUserID: "alice", accountSessionID: session))
            #expect(!state.accepts(request, ownerUserID: "bob", accountSessionID: session))
            #expect(!state.accepts(request, ownerUserID: "alice", accountSessionID: UUID()))
            let didFinish = state.finish(request)
            #expect(didFinish)
        }
        #expect(state.activeToken == nil)
    }

    private static func project() -> StoryProject {
        StoryProject(
            title: "Story",
            description: "Description",
            models: .init(
                textModelID: "text",
                imageModelID: "image",
                videoModelID: "video"
            )
        )
    }

    private static func snapshot(
        project: StoryProject,
        owner: String
    ) -> StoryPlanningRunSnapshot {
        StoryPlanningRunSnapshot(
            id: UUID(),
            ownerUserID: owner,
            projectID: project.id,
            stage: .outline,
            targetIDs: [],
            baseProjectFingerprint: (try? StoryProjectFingerprint.current(project)) ?? "",
            draft: project,
            events: [],
            status: .paused,
            stopReason: nil,
            modelCalls: 0,
            usesCloudMemory: true,
            applied: false,
            abandonedAt: nil,
            updatedAt: Date(timeIntervalSince1970: 100)
        )
    }
}

private actor RecordingStoryPlanningExecutor: StoryPlanningExecuting {
    struct Request: Equatable {
        var ownerUserID: String
        var command: StoryPlanningCommand
    }

    var requests: [Request] = []
    let outcome: StoryPlanningExecutionOutcome

    init(outcome: StoryPlanningExecutionOutcome) {
        self.outcome = outcome
    }

    nonisolated func validateStoryPlanningAvailability() throws {}

    func executeStoryPlanning(
        ownerUserID: String,
        command: StoryPlanningCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        shouldPause: @escaping @Sendable () async -> Bool,
        onEvent: @escaping @Sendable (StoryPlanningExecutionEvent) async -> Void
    ) async throws -> StoryPlanningExecutionOutcome {
        requests.append(.init(ownerUserID: ownerUserID, command: command))
        try await continuationCheck()
        _ = await shouldPause()
        await onEvent(.run(outcome.run))
        return outcome
    }
}

private actor StoryPlanningCheckCounter {
    var count = 0

    func record() {
        count += 1
    }
}

private actor StoryPlanningEventRecorder {
    var values: [StoryPlanningExecutionEvent] = []

    func record(_ event: StoryPlanningExecutionEvent) {
        values.append(event)
    }
}
