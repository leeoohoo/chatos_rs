import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story video generation application boundary")
struct StoryVideoGenerationUseCaseTests {
    @Test("start preparation freezes model, segment, duration, and bounded guidance")
    func preparesStart() throws {
        let project = Self.readyProject()
        let generator = StubStoryVideoGenerator(outcome: .init(project: project, status: .completed))
        let command = try StoryVideoGenerationUseCase(generator: generator).prepareStart(
            ownerUserID: "  alice  ",
            project: project,
            segmentID: "s1",
            model: Self.model,
            size: "768P",
            userIdeas: "  " + String(repeating: "x", count: 1_010) + "  "
        )

        #expect(command.projectID == project.id)
        #expect(command.segmentID == "s1")
        #expect(command.attempt.modelConfigID == "video")
        #expect(command.attempt.seconds == 15)
        #expect(command.attempt.prompt == String(repeating: "x", count: 1_000))
        #expect(command.kind == .start(model: Self.model))
    }

    @Test("resume requires the exact durable attempt and provider job")
    func preparesResume() throws {
        var project = Self.readyProject()
        var attempt = StoryVideoAttempt(
            modelConfigID: "video",
            prompt: "prompt",
            size: "768P",
            ratio: "16:9"
        )
        attempt.jobID = "job"
        project.segments[0].attempt = attempt
        let useCase = StoryVideoGenerationUseCase(
            generator: StubStoryVideoGenerator(outcome: .init(project: project, status: .failedRetainingAttempt))
        )

        let command = try useCase.prepareResume(
            ownerUserID: "alice",
            project: project,
            segmentID: "s1"
        )
        #expect(command.attempt == attempt)
        #expect(command.kind == .resume(jobID: "job"))

        project.segments[0].attempt?.jobID = nil
        #expect(throws: StoryError.unresolvedSubmission) {
            _ = try useCase.prepareResume(
                ownerUserID: "alice",
                project: project,
                segmentID: "s1"
            )
        }
    }

    @Test("execution filters foreign progress and accepts its completed result")
    func filtersEvents() async throws {
        let project = Self.readyProject()
        let prepared = try StoryVideoGenerationUseCase(
            generator: StubStoryVideoGenerator(outcome: .init(project: project, status: .completed))
        ).prepareStart(
            ownerUserID: "alice",
            project: project,
            segmentID: "s1",
            model: Self.model,
            size: "768P",
            userIdeas: ""
        )
        var foreign = project
        foreign.segments[0].attempt = StoryVideoAttempt(
            modelConfigID: "video", prompt: "foreign", size: "768P", ratio: "16:9"
        )
        var active = project
        active.version = 3
        active.segments[0].videoJobRecordID = prepared.attempt.id
        var completed = active
        completed.segments[0].video = .init(filename: "video.mp4", jobID: "job", modelName: "MiniMax-H3")
        let generator = StubStoryVideoGenerator(
            events: [
                .init(project: foreign, progress: .init(status: "foreign")),
                .init(project: active, progress: .init(status: "queued", percent: 0.2)),
            ],
            outcome: .init(project: completed, status: .completed)
        )
        let collector = StoryVideoEventCollector()

        let outcome = try await StoryVideoGenerationUseCase(generator: generator).execute(
            ownerUserID: "alice",
            accountSessionID: UUID(),
            command: prepared,
            continuationCheck: {},
            onEvent: { await collector.append($0) }
        )

        #expect(outcome.project == completed)
        #expect(await collector.events.map(\.progress.status) == ["queued"])
    }

    @Test("terminal status must match the frozen attempt semantics")
    func rejectsForeignOutcome() async throws {
        let project = Self.readyProject()
        let prepared = try StoryVideoGenerationUseCase(
            generator: StubStoryVideoGenerator(outcome: .init(project: project, status: .completed))
        ).prepareStart(
            ownerUserID: "alice",
            project: project,
            segmentID: "s1",
            model: Self.model,
            size: "768P",
            userIdeas: ""
        )
        var foreign = project
        foreign.segments[0].error = "failed"
        let useCase = StoryVideoGenerationUseCase(
            generator: StubStoryVideoGenerator(
                outcome: .init(project: foreign, status: .failedRetainingAttempt)
            )
        )

        await #expect(throws: StoryVideoGenerationError.identityMismatch) {
            _ = try await useCase.execute(
                ownerUserID: "alice",
                accountSessionID: UUID(),
                command: prepared,
                continuationCheck: {},
                onEvent: { _ in }
            )
        }
    }

    @Test("lifecycle permits different segments and rejects replacements and stale cleanup")
    func validatesLifecycle() {
        let project = Self.readyProject()
        let first = Self.command(project: project, segmentID: "s1")
        let second = Self.command(project: project, segmentID: "s2")
        var state = StoryVideoGenerationState()
        let session = UUID()
        let firstToken = state.begin(ownerUserID: "alice", accountSessionID: session, command: first)
        let secondToken = state.begin(ownerUserID: "alice", accountSessionID: session, command: second)
        #expect(firstToken != nil)
        #expect(secondToken != nil)
        #expect(state.begin(ownerUserID: "alice", accountSessionID: session, command: first) == nil)
        guard let firstToken else { return }
        #expect(state.accepts(firstToken, ownerUserID: "alice", accountSessionID: session))
        #expect(!state.accepts(firstToken, ownerUserID: "bob", accountSessionID: session))
        let didFinish = state.finish(firstToken)
        #expect(didFinish)
        let replacement = state.begin(ownerUserID: "alice", accountSessionID: session, command: first)
        let staleFinish = state.finish(firstToken)
        #expect(!staleFinish)
        #expect(replacement != nil)
    }

    private static var model: MediaGenerationModel {
        .init(id: "video", name: "Video", provider: "test", modelName: "MiniMax-H3",
              enabled: true, taskEnabled: false, hasAPIKey: true)
    }

    private static func readyProject() -> StoryProject {
        var project = StoryProject(
            title: "Story", description: "",
            models: .init(textModelID: "text", imageModelID: "image", videoModelID: "video")
        )
        project.source = "ab"
        project.segments = ["s1", "s2"].enumerated().map { index, id in
            var segment = StorySegment(
                id: id, title: id, synopsis: id,
                sourceRange: .init(start: index, end: index + 1)
            )
            segment.detail = .init(
                firstFramePrompt: "frame",
                shots: [.init(start: 0, end: 15, prompt: "shot")],
                continuityIn: "in", continuityOut: "out", audio: "", constraints: ""
            )
            let frame = StoryImage(filename: "\(id).png", mimeType: "image/png")
            segment.firstFrames.images = [frame]
            segment.confirmedFrameID = frame.id
            return segment
        }
        return project
    }

    private static func command(project: StoryProject, segmentID: String) -> StoryVideoGenerationCommand {
        .init(
            project: project,
            segmentID: segmentID,
            attempt: .init(modelConfigID: "video", prompt: "prompt", size: "768P", ratio: "16:9"),
            kind: .start(model: model)
        )
    }
}

private struct StubStoryVideoGenerator: StoryVideoGenerating {
    var events: [StoryVideoGenerationEvent] = []
    let outcome: StoryVideoGenerationOutcome

    func prepareStoryVideoGeneration(
        ownerUserID: String,
        project: StoryProject,
        segmentID: String,
        model: MediaGenerationModel,
        size: String,
        userIdeas: String
    ) throws -> StoryVideoGenerationCommand {
        guard let segment = project.segments.first(where: { $0.id == segmentID }) else {
            throw StoryError.invalidPlan
        }
        return .init(
            project: project,
            segmentID: segmentID,
            attempt: .init(
                modelConfigID: project.models.videoModelID,
                prompt: userIdeas.isEmpty ? "prompt" : userIdeas,
                size: size,
                ratio: project.ratio,
                seconds: segment.seconds
            ),
            kind: .start(model: model)
        )
    }

    func executeStoryVideoGeneration(
        ownerUserID: String,
        accountSessionID: UUID,
        command: StoryVideoGenerationCommand,
        continuationCheck: @escaping @Sendable () async throws -> Void,
        onEvent: @escaping @Sendable (StoryVideoGenerationEvent) async -> Void
    ) async throws -> StoryVideoGenerationOutcome {
        try await continuationCheck()
        for event in events { await onEvent(event) }
        return outcome
    }
}

private actor StoryVideoEventCollector {
    private(set) var events: [StoryVideoGenerationEvent] = []
    func append(_ event: StoryVideoGenerationEvent) { events.append(event) }
}
