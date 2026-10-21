import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story frame image generation application boundary")
struct StoryFrameImageGenerationUseCaseTests {
    @Test("preparation freezes story reference order, role, attempt, and bounded ideas")
    func preparesGeneration() throws {
        let project = Self.project()
        let attemptID = UUID()
        let useCase = StoryFrameImageGenerationUseCase(
            generator: StubStoryFrameImageGenerator(outcome: .init(project: project))
        )

        let command = try useCase.prepare(
            ownerUserID: "  alice  ",
            project: project,
            segmentID: "s1",
            role: .last,
            referenceResourceIDs: ["map", "key"],
            userIdeas: "  " + String(repeating: "想", count: 2_010) + "  ",
            attemptID: attemptID
        )

        #expect(command.projectID == project.id)
        #expect(command.segmentID == "s1")
        #expect(command.role == .last)
        #expect(command.referenceResourceIDs == ["key", "map"])
        #expect(command.attemptID == attemptID)
        #expect(command.userIdeas == String(repeating: "想", count: 2_000))
    }

    @Test("preparation rejects foreign references and unresolved role attempts")
    func rejectsInvalidPreparation() {
        let project = Self.project()
        let useCase = StoryFrameImageGenerationUseCase(
            generator: StubStoryFrameImageGenerator(outcome: .init(project: project))
        )
        #expect(throws: StoryFrameImageGenerationError.invalidReferences) {
            _ = try useCase.prepare(
                ownerUserID: "alice",
                project: project,
                segmentID: "s1",
                role: .first,
                referenceResourceIDs: ["foreign"],
                userIdeas: ""
            )
        }

        var unresolved = project
        unresolved.segments[0].lastFrames.generationAttemptID = UUID()
        #expect(throws: StoryError.unresolvedSubmission) {
            _ = try useCase.prepare(
                ownerUserID: "alice",
                project: unresolved,
                segmentID: "s1",
                role: .last,
                referenceResourceIDs: ["key"],
                userIdeas: ""
            )
        }
    }

    @Test("execution filters another role's intent and verifies the completed frame")
    func filtersEventsAndValidatesCompletion() async throws {
        let project = Self.project()
        let attemptID = UUID()
        let command = try StoryFrameImageGenerationUseCase(
            generator: StubStoryFrameImageGenerator(outcome: .init(project: project))
        ).prepare(
            ownerUserID: "alice",
            project: project,
            segmentID: "s1",
            role: .last,
            referenceResourceIDs: ["key"],
            userIdeas: "",
            attemptID: attemptID
        )
        var foreign = project
        foreign.segments[0].firstFrames.generationAttemptID = attemptID
        var intent = project
        intent.segments[0].lastFrames.generationAttemptID = attemptID
        let completed = Self.completedProject(project, role: .last, attemptID: attemptID)
        let generator = StubStoryFrameImageGenerator(
            events: [.init(project: foreign), .init(project: intent)],
            outcome: .init(project: completed)
        )
        let collector = StoryFrameImageEventCollector()

        let outcome = try await StoryFrameImageGenerationUseCase(generator: generator).generate(
            ownerUserID: "  alice  ",
            command: command,
            continuationCheck: {},
            onEvent: { await collector.append($0) }
        )

        #expect(outcome.project == completed)
        #expect(await collector.events.map(\.project) == [intent])
        let requests = await generator.requests
        #expect(requests.first?.ownerUserID == "alice")
    }

    @Test("execution rejects a completed image from another role or attempt")
    func rejectsForeignOutcome() async throws {
        let project = Self.project()
        let attemptID = UUID()
        let command = try StoryFrameImageGenerationUseCase(
            generator: StubStoryFrameImageGenerator(outcome: .init(project: project))
        ).prepare(
            ownerUserID: "alice",
            project: project,
            segmentID: "s1",
            role: .first,
            referenceResourceIDs: ["key"],
            userIdeas: "",
            attemptID: attemptID
        )
        let foreign = Self.completedProject(project, role: .last, attemptID: attemptID)
        let useCase = StoryFrameImageGenerationUseCase(
            generator: StubStoryFrameImageGenerator(outcome: .init(project: foreign))
        )

        await #expect(throws: StoryFrameImageGenerationError.identityMismatch) {
            _ = try await useCase.generate(
                ownerUserID: "alice",
                command: command,
                continuationCheck: {},
                onEvent: { _ in }
            )
        }
    }

    @Test("lifecycle preserves global single-flight and rejects stale account/session cleanup")
    func validatesLifecycle() throws {
        let project = Self.project()
        let useCase = StoryFrameImageGenerationUseCase(
            generator: StubStoryFrameImageGenerator(outcome: .init(project: project))
        )
        let firstCommand = try useCase.prepare(
            ownerUserID: "alice",
            project: project,
            segmentID: "s1",
            role: .first,
            referenceResourceIDs: ["key"],
            userIdeas: ""
        )
        let lastCommand = try useCase.prepare(
            ownerUserID: "alice",
            project: project,
            segmentID: "s1",
            role: .last,
            referenceResourceIDs: ["key"],
            userIdeas: ""
        )
        var state = StoryFrameImageGenerationState()
        let session = UUID()
        let first = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            command: firstCommand
        )
        #expect(first != nil)
        #expect(state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            command: lastCommand
        ) == nil)
        guard let first else { return }
        #expect(state.accepts(first, ownerUserID: "alice", accountSessionID: session))
        #expect(!state.accepts(first, ownerUserID: "bob", accountSessionID: session))
        #expect(!state.accepts(first, ownerUserID: "alice", accountSessionID: UUID()))
        let didFinish = state.finish(first)
        #expect(didFinish)
        let replacement = state.begin(
            ownerUserID: "alice",
            accountSessionID: session,
            command: lastCommand
        )
        #expect(replacement != nil)
        let staleFinish = state.finish(first)
        #expect(!staleFinish)
        #expect(state.activeToken == replacement)
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
        project.source = "a"
        var key = StoryProp(id: "key", name: "Key", description: "An old key")
        let keyImage = StoryImage(filename: "key.png", mimeType: "image/png")
        key.media.images = [keyImage]
        key.media.confirmedImageID = keyImage.id
        var map = StoryProp(id: "map", name: "Map", description: "A map")
        let mapImage = StoryImage(filename: "map.png", mimeType: "image/png")
        map.media.images = [mapImage]
        map.media.confirmedImageID = mapImage.id
        project.props = [key, map]
        var segment = StorySegment(
            id: "s1",
            title: "Opening",
            synopsis: "Opening",
            sourceRange: .init(start: 0, end: 1),
            propIDs: ["key", "map"]
        )
        segment.detail = .init(
            firstFramePrompt: "frame",
            shots: [.init(start: 0, end: 15, prompt: "shot")],
            continuityIn: "in",
            continuityOut: "out",
            audio: "",
            constraints: ""
        )
        project.segments = [segment]
        return project
    }

    private static func completedProject(
        _ project: StoryProject,
        role: StoryFrameRole,
        attemptID: UUID
    ) -> StoryProject {
        var completed = project
        let image = StoryImage(
            filename: "\(role.rawValue).png",
            mimeType: "image/png",
            sourceResourceID: "s1",
            generationAttemptID: attemptID
        )
        switch role {
        case .first:
            completed.segments[0].firstFrames.images.append(image)
            completed.segments[0].firstFrames.generationAttemptID = nil
        case .last:
            completed.segments[0].lastFrames.images.append(image)
            completed.segments[0].lastFrames.generationAttemptID = nil
        }
        return completed
    }
}

private actor StubStoryFrameImageGenerator: StoryFrameImageGenerating {
    struct Request: Equatable {
        let ownerUserID: String
        let command: StoryFrameImageGenerationCommand
    }

    let events: [StoryFrameImageGenerationEvent]
    let outcome: StoryFrameImageGenerationOutcome
    private(set) var requests: [Request] = []

    init(
        events: [StoryFrameImageGenerationEvent] = [],
        outcome: StoryFrameImageGenerationOutcome
    ) {
        self.events = events
        self.outcome = outcome
    }

    func generateStoryFrameImage(
        ownerUserID: String,
        command: StoryFrameImageGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryFrameImageGenerationEvent) async -> Void
    ) async throws -> StoryFrameImageGenerationOutcome {
        requests.append(.init(ownerUserID: ownerUserID, command: command))
        try await continuationCheck()
        for event in events { await onEvent(event) }
        return outcome
    }
}

private actor StoryFrameImageEventCollector {
    private(set) var events: [StoryFrameImageGenerationEvent] = []

    func append(_ event: StoryFrameImageGenerationEvent) {
        events.append(event)
    }
}
