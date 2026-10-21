import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story video tail extraction application boundary")
struct StoryVideoTailExtractionUseCaseTests {
    @Test("preparation freezes the completed video identity and extraction returns its actual tail")
    func extractsTail() async throws {
        let extractor = RecordingStoryVideoTailExtractor()
        let useCase = StoryVideoTailExtractionUseCase(extractor: extractor)
        let project = Self.project()
        let command = try useCase.prepare(
            ownerUserID: "  alice  ",
            project: project,
            segmentID: "segment-1"
        )

        let result = try await useCase.extract(command)

        #expect(command.ownerUserID == "alice")
        #expect(command.projectID == project.id)
        #expect(command.videoJobID == "job-1")
        #expect(command.videoFilename == "video.mp4")
        #expect(result.segments[0].actualVideoLastFrame?.derivedFromVideoJobID == "job-1")
        #expect(await extractor.commands == [command])
    }

    @Test("blank owners and segments without a completed video fail before extraction")
    func rejectsInvalidAdmission() {
        let useCase = StoryVideoTailExtractionUseCase(
            extractor: RecordingStoryVideoTailExtractor()
        )
        let project = Self.project()
        #expect(throws: StoryVideoTailExtractionError.invalidOwner) {
            _ = try useCase.prepare(
                ownerUserID: "   ",
                project: project,
                segmentID: "segment-1"
            )
        }
        var incomplete = project
        incomplete.segments[0].video = nil
        #expect(throws: StoryVideoTailExtractionError.invalidTarget) {
            _ = try useCase.prepare(
                ownerUserID: "alice",
                project: incomplete,
                segmentID: "segment-1"
            )
        }
    }

    @Test("a tail from another provider job is rejected")
    func rejectsMismatchedTail() async throws {
        let useCase = StoryVideoTailExtractionUseCase(
            extractor: RecordingStoryVideoTailExtractor(derivedJobID: "other-job")
        )
        let command = try useCase.prepare(
            ownerUserID: "alice",
            project: Self.project(),
            segmentID: "segment-1"
        )

        await #expect(throws: StoryVideoTailExtractionError.resultMismatch) {
            _ = try await useCase.extract(command)
        }
    }

    @Test("a result for another project is rejected")
    func rejectsMismatchedProject() async throws {
        let useCase = StoryVideoTailExtractionUseCase(
            extractor: RecordingStoryVideoTailExtractor(replaceProjectID: true)
        )
        let command = try useCase.prepare(
            ownerUserID: "alice",
            project: Self.project(),
            segmentID: "segment-1"
        )

        await #expect(throws: StoryVideoTailExtractionError.resultMismatch) {
            _ = try await useCase.extract(command)
        }
    }

    @Test("extraction lifecycle is single-flight per segment and account-bound")
    func validatesLifecycle() throws {
        let useCase = StoryVideoTailExtractionUseCase(
            extractor: RecordingStoryVideoTailExtractor()
        )
        let command = try useCase.prepare(
            ownerUserID: "alice",
            project: Self.project(),
            segmentID: "segment-1"
        )
        let other = StoryVideoTailExtractionCommand(
            ownerUserID: "alice",
            projectID: command.projectID,
            segmentID: "segment-2",
            videoJobID: "job-2",
            videoFilename: "video-2.mp4"
        )
        var state = StoryVideoTailExtractionState()
        let session = UUID()
        let token = state.begin(command: command, accountSessionID: session)
        #expect(token != nil)
        #expect(state.begin(command: command, accountSessionID: session) == nil)
        let otherToken = state.begin(command: other, accountSessionID: session)
        #expect(otherToken != nil)
        if let token {
            #expect(state.accepts(
                token,
                ownerUserID: "alice",
                accountSessionID: session
            ))
            #expect(!state.accepts(
                token,
                ownerUserID: "bob",
                accountSessionID: session
            ))
            let didFinish = state.finish(token)
            #expect(didFinish)
        }
        #expect(state.activeTokens.count == 1)
        state.reset()
        #expect(state.activeTokens.isEmpty)
    }

    fileprivate static func project() -> StoryProject {
        var project = StoryProject(
            title: "Story",
            description: "Description",
            models: .init(
                textModelID: "text",
                imageModelID: "image",
                videoModelID: "video"
            )
        )
        project.source = "x"
        var segment = StorySegment(
            id: "segment-1",
            title: "Segment",
            synopsis: "Segment",
            sourceRange: .init(start: 0, end: 1)
        )
        segment.video = .init(
            filename: "video.mp4",
            jobID: "job-1",
            modelName: "video-model"
        )
        project.segments = [segment]
        return project
    }
}

private actor RecordingStoryVideoTailExtractor: StoryVideoTailExtracting {
    let derivedJobID: String
    let replaceProjectID: Bool
    var commands: [StoryVideoTailExtractionCommand] = []

    init(derivedJobID: String = "job-1", replaceProjectID: Bool = false) {
        self.derivedJobID = derivedJobID
        self.replaceProjectID = replaceProjectID
    }

    func extractStoryVideoTail(
        command: StoryVideoTailExtractionCommand
    ) async throws -> StoryProject {
        commands.append(command)
        var project = StoryVideoTailExtractionUseCaseTests.project()
        if replaceProjectID {
            project.id = UUID()
        } else {
            project.id = command.projectID
        }
        let image = StoryImage(
            filename: "tail.png",
            mimeType: "image/png",
            derivedFromVideoJobID: derivedJobID
        )
        project.segments[0].lastFrames.images.append(image)
        project.segments[0].actualVideoLastFrameID = image.id
        return project
    }
}
