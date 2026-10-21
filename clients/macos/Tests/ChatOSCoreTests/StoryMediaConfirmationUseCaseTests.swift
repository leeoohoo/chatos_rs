import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story media confirmation application boundary")
struct StoryMediaConfirmationUseCaseTests {
    private let useCase = StoryMediaConfirmationUseCase()

    @Test("resource attachment and confirmation invalidate only editable dependent frames")
    func attachesResourceImage() throws {
        var project = Self.project()
        let oldResource = StoryImage(filename: "old-resource.png", mimeType: "image/png")
        let first = StoryImage(filename: "first.png", mimeType: "image/png")
        let last = StoryImage(filename: "last.png", mimeType: "image/png")
        var resource = project.resource(id: "key")!
        resource.media.images = [oldResource]
        resource.media.confirmedImageID = oldResource.id
        try project.replaceResource(resource)
        project.segments[0].firstFrames.images = [first]
        project.segments[0].firstFrames.confirmedImageID = first.id
        project.segments[0].lastFrames.images = [last]
        project.segments[0].lastFrames.confirmedImageID = last.id
        let imported = StoryImage(filename: "imported.png", mimeType: "image/png")

        let result = try useCase.attachingImportedImage(
            imported,
            target: .resource(id: "key"),
            confirmsImportedImage: true,
            to: project
        )

        #expect(result.resource(id: "key")?.images == [oldResource, imported])
        #expect(result.resource(id: "key")?.confirmedImageID == imported.id)
        #expect(result.segments[0].confirmedFrameID == nil)
        #expect(result.segments[0].confirmedLastFrameID == nil)
    }

    @Test("first-frame attachment records an explicit user choice")
    func attachesFirstFrame() throws {
        var project = Self.project()
        project.segments[0].inheritedFirstFrameSourceSegmentID = "previous"
        let imported = StoryImage(filename: "first.png", mimeType: "image/png")

        let result = try useCase.attachingImportedImage(
            imported,
            target: .frame(segmentID: "s1", role: .first),
            confirmsImportedImage: true,
            to: project
        )

        #expect(result.segments[0].firstFrames.images == [imported])
        #expect(result.segments[0].confirmedFrameID == imported.id)
        #expect(result.segments[0].userSelectedFirstFrameID == imported.id)
        #expect(result.segments[0].inheritedFirstFrameSourceSegmentID == nil)
    }

    @Test("manual resource confirmation rejects an unresolved dependent video")
    func rejectsResourceConfirmationDuringSubmission() throws {
        var project = Self.project()
        let image = StoryImage(filename: "resource.png", mimeType: "image/png")
        var resource = project.resource(id: "key")!
        resource.media.images = [image]
        try project.replaceResource(resource)
        project.segments[0].attempt = .init(
            modelConfigID: "video",
            prompt: "prompt",
            size: "768P",
            ratio: "16:9"
        )

        #expect(throws: StoryError.unresolvedSubmission) {
            _ = try useCase.confirmingImage(
                image.id,
                target: .resource(id: "key"),
                in: project
            )
        }
    }

    @Test("latest frame confirmation preserves existing choices and enables tail guidance")
    func confirmsLatestFrames() throws {
        var project = Self.project()
        let selectedFirst = StoryImage(filename: "selected.png", mimeType: "image/png")
        let newerFirst = StoryImage(filename: "newer.png", mimeType: "image/png")
        let tail = StoryImage(filename: "tail.png", mimeType: "image/png")
        project.segments[0].firstFrames.images = [selectedFirst, newerFirst]
        project.segments[0].firstFrames.confirmedImageID = selectedFirst.id
        project.segments[0].userSelectedFirstFrameID = selectedFirst.id
        project.segments[0].lastFrames.images = [tail]

        let mutation = try useCase.confirmingLatestFrames(
            segmentID: "s1",
            useConfirmedLastFrameForVideo: true,
            in: project
        )
        let result = try #require(mutation)

        #expect(result.segments[0].confirmedFrameID == selectedFirst.id)
        #expect(result.segments[0].confirmedLastFrameID == tail.id)
        #expect(result.segments[0].useLastFrameForVideo)
    }

    @Test("clearing the confirmed tail also resets tail guidance")
    func clearsConfirmedTail() throws {
        var project = Self.project()
        let tail = StoryImage(filename: "tail.png", mimeType: "image/png")
        project.segments[0].lastFrames.images = [tail]
        project.segments[0].lastFrames.confirmedImageID = tail.id
        project.segments[0].useLastFrameForVideo = true

        let mutation = try useCase.clearingConfirmedLastFrame(
            segmentID: "s1",
            in: project
        )
        let result = try #require(mutation)

        #expect(result.segments[0].confirmedLastFrameID == nil)
        #expect(!result.segments[0].useLastFrameForVideo)
    }

    @Test("refined detail invalidates frame choices and tail guidance")
    func appliesRefinedDetail() throws {
        var project = Self.project()
        Self.addConfirmedFrames(to: &project.segments[0])
        project.segments[0].useLastFrameForVideo = true
        let detail = Self.detail(continuityIn: "new entrance")

        let result = try useCase.applyingRefinedDetail(
            detail,
            segmentID: "s1",
            in: project
        )

        #expect(result.segments[0].detail == detail)
        #expect(result.segments[0].confirmedFrameID == nil)
        #expect(result.segments[0].confirmedLastFrameID == nil)
        #expect(!result.segments[0].useLastFrameForVideo)
    }

    @Test("segment continuity edits invalidate current and adjacent plans")
    func appliesSegmentEdit() throws {
        var project = Self.threeSegmentProject()
        for index in project.segments.indices {
            project.segments[index].detail = Self.detail(continuityIn: "in-\(index)")
            Self.addConfirmedFrames(to: &project.segments[index])
        }
        var edited = project.segments[1]
        edited.synopsis = "Changed continuity"

        let mutation = try useCase.applyingSegmentEdit(
            edited,
            relations: [],
            to: project
        )
        let result = try #require(mutation)

        #expect(result.segments[1].synopsis == "Changed continuity")
        #expect(result.segments.allSatisfy { $0.confirmedFrameID == nil })
        #expect(result.segments.allSatisfy { $0.confirmedLastFrameID == nil })
        #expect(result.segments[0].detail == nil)
        #expect(result.segments[2].detail == nil)
    }

    @Test("structural segment changes invalidate every plan through one policy")
    func invalidatesAllSegmentPlans() {
        var project = Self.threeSegmentProject()
        for index in project.segments.indices {
            project.segments[index].detail = Self.detail(continuityIn: "in-\(index)")
            Self.addConfirmedFrames(to: &project.segments[index])
        }

        let result = useCase.invalidatingAllSegmentPlans(in: project)

        #expect(result.segments.allSatisfy { $0.detail == nil })
        #expect(result.segments.allSatisfy { $0.confirmedFrameID == nil })
        #expect(result.segments.allSatisfy { $0.confirmedLastFrameID == nil })
    }

    private static func project() -> StoryProject {
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
        project.props = [
            .init(
                id: "key",
                name: "Key",
                description: "An old key",
                imagePrompt: "An old brass key"
            ),
        ]
        project.segments = [
            .init(
                id: "s1",
                title: "Segment",
                synopsis: "Segment",
                sourceRange: .init(start: 0, end: 1),
                propIDs: ["key"]
            ),
        ]
        return project
    }

    private static func threeSegmentProject() -> StoryProject {
        var project = project()
        project.source = "abc"
        project.segments = (0..<3).map { index in
            .init(
                id: "s\(index + 1)",
                title: "Segment \(index + 1)",
                synopsis: "Segment \(index + 1)",
                sourceRange: .init(start: index, end: index + 1),
                propIDs: ["key"]
            )
        }
        return project
    }

    private static func detail(continuityIn: String) -> StorySegmentDetail {
        .init(
            firstFramePrompt: "First frame",
            shots: [.init(start: 0, end: 15, prompt: "Shot")],
            continuityIn: continuityIn,
            continuityOut: "Exit",
            audio: "Audio",
            constraints: "Constraints",
            lastFramePrompt: "Last frame"
        )
    }

    private static func addConfirmedFrames(to segment: inout StorySegment) {
        let first = StoryImage(filename: "\(segment.id)-first.png", mimeType: "image/png")
        let last = StoryImage(filename: "\(segment.id)-last.png", mimeType: "image/png")
        segment.firstFrames.images = [first]
        segment.firstFrames.confirmedImageID = first.id
        segment.userSelectedFirstFrameID = first.id
        segment.lastFrames.images = [last]
        segment.lastFrames.confirmedImageID = last.id
    }
}
