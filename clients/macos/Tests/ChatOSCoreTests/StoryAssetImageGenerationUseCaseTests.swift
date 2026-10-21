import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story asset image generation application boundary")
struct StoryAssetImageGenerationUseCaseTests {
    @Test("preparation trims owner and ideas while freezing the attempt identity")
    func preparesGeneration() throws {
        let project = Self.project()
        let attemptID = UUID()
        let useCase = StoryAssetImageGenerationUseCase(
            generator: StubStoryAssetImageGenerator(outcome: .init(project: project))
        )

        let command = try useCase.prepare(
            ownerUserID: "  alice  ",
            project: project,
            resourceID: "key",
            userIdeas: "  " + String(repeating: "想", count: 2_010) + "  ",
            attemptID: attemptID
        )

        #expect(command.projectID == project.id)
        #expect(command.resourceID == "key")
        #expect(command.attemptID == attemptID)
        #expect(command.userIdeas == String(repeating: "想", count: 2_000))
    }

    @Test("preparation refuses a resource with an unresolved generation attempt")
    func rejectsUnresolvedAttempt() {
        var project = Self.project()
        project.props[0].media.generationAttemptID = UUID()
        let useCase = StoryAssetImageGenerationUseCase(
            generator: StubStoryAssetImageGenerator(outcome: .init(project: project))
        )

        #expect(throws: StoryError.unresolvedSubmission) {
            _ = try useCase.prepare(
                ownerUserID: "alice",
                project: project,
                resourceID: "key",
                userIdeas: ""
            )
        }
    }

    @Test("execution filters foreign intent events and accepts only its completed image")
    func filtersEventsAndValidatesCompletion() async throws {
        let project = Self.project()
        let attemptID = UUID()
        let command = StoryAssetImageGenerationCommand(
            projectID: project.id,
            resourceID: "key",
            attemptID: attemptID,
            userIdeas: "  " + String(repeating: "x", count: 2_010) + "  "
        )
        var foreign = project
        foreign.props[0].media.generationAttemptID = UUID()
        var intent = project
        intent.props[0].media.generationAttemptID = attemptID
        let completed = Self.completedProject(project, attemptID: attemptID)
        let generator = StubStoryAssetImageGenerator(
            events: [.init(project: foreign), .init(project: intent)],
            outcome: .init(project: completed)
        )
        let collector = StoryAssetImageEventCollector()

        let outcome = try await StoryAssetImageGenerationUseCase(generator: generator).generate(
            ownerUserID: "  alice  ",
            command: command,
            continuationCheck: {},
            onEvent: { await collector.append($0) }
        )

        #expect(outcome.project == completed)
        #expect(await collector.events.map(\.project) == [intent])
        let requests = await generator.requests
        #expect(requests.count == 1)
        #expect(requests.first?.ownerUserID == "alice")
        #expect(requests.first?.command.userIdeas == String(repeating: "x", count: 2_000))
    }

    @Test("execution rejects a terminal project belonging to another attempt")
    func rejectsForeignOutcome() async {
        let project = Self.project()
        let command = StoryAssetImageGenerationCommand(
            projectID: project.id,
            resourceID: "key",
            attemptID: UUID(),
            userIdeas: ""
        )
        let foreign = Self.completedProject(project, attemptID: UUID())
        let useCase = StoryAssetImageGenerationUseCase(
            generator: StubStoryAssetImageGenerator(outcome: .init(project: foreign))
        )

        await #expect(throws: StoryAssetImageGenerationError.identityMismatch) {
            _ = try await useCase.generate(
                ownerUserID: "alice",
                command: command,
                continuationCheck: {},
                onEvent: { _ in }
            )
        }
    }

    @Test("per-resource lifecycle permits concurrency but rejects replacements and stale cleanup")
    func validatesLifecycle() {
        var state = StoryAssetImageGenerationState()
        let session = UUID()
        let projectID = UUID()
        let keyCommand = Self.command(projectID: projectID, resourceID: "key")
        let mapCommand = Self.command(projectID: projectID, resourceID: "map")
        let key = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            command: keyCommand
        )
        let map = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            command: mapCommand
        )

        #expect(key != nil)
        #expect(map != nil)
        #expect(state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            command: keyCommand
        ) == nil)
        guard let key, let map else { return }
        #expect(state.accepts(key, ownerUserID: "alice", accountSessionID: session))
        #expect(!state.accepts(key, ownerUserID: "bob", accountSessionID: session))
        #expect(!state.accepts(key, ownerUserID: "alice", accountSessionID: UUID()))
        #expect(state.accepts(map, ownerUserID: "alice", accountSessionID: session))

        let didFinishKey = state.finish(key)
        #expect(didFinishKey)
        let replacement = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            command: keyCommand
        )
        #expect(replacement != nil)
        let staleFinish = state.finish(key)
        #expect(!staleFinish)
        if let replacement {
            #expect(state.activeTokens[replacement.target] == replacement)
        }
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
        project.props = [
            .init(id: "key", name: "Key", description: "An old key"),
            .init(id: "map", name: "Map", description: "A hand-drawn map"),
        ]
        return project
    }

    private static func completedProject(
        _ project: StoryProject,
        attemptID: UUID
    ) -> StoryProject {
        var completed = project
        completed.props[0].media.generationAttemptID = nil
        completed.props[0].media.images = [
            StoryImage(
                filename: "key.png",
                mimeType: "image/png",
                sourceResourceID: "key",
                generationAttemptID: attemptID
            ),
        ]
        return completed
    }

    private static func command(
        projectID: UUID,
        resourceID: String
    ) -> StoryAssetImageGenerationCommand {
        .init(
            projectID: projectID,
            resourceID: resourceID,
            attemptID: UUID(),
            userIdeas: ""
        )
    }
}

private actor StubStoryAssetImageGenerator: StoryAssetImageGenerating {
    struct Request: Equatable {
        let ownerUserID: String
        let command: StoryAssetImageGenerationCommand
    }

    let events: [StoryAssetImageGenerationEvent]
    let outcome: StoryAssetImageGenerationOutcome
    private(set) var requests: [Request] = []

    init(
        events: [StoryAssetImageGenerationEvent] = [],
        outcome: StoryAssetImageGenerationOutcome
    ) {
        self.events = events
        self.outcome = outcome
    }

    func generateStoryAssetImage(
        ownerUserID: String,
        command: StoryAssetImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryAssetImageGenerationEvent) async -> Void
    ) async throws -> StoryAssetImageGenerationOutcome {
        requests.append(.init(ownerUserID: ownerUserID, command: command))
        try await continuationCheck()
        for event in events { await onEvent(event) }
        return outcome
    }
}

private actor StoryAssetImageEventCollector {
    private(set) var events: [StoryAssetImageGenerationEvent] = []

    func append(_ event: StoryAssetImageGenerationEvent) {
        events.append(event)
    }
}
