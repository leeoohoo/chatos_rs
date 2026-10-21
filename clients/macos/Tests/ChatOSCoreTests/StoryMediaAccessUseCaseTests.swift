import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story media access application boundary")
struct StoryMediaAccessUseCaseTests {
    @Test("image projection trims owner and preserves only display identity")
    func resolvesImage() throws {
        let resolver = RecordingStoryMediaResolver()
        let useCase = StoryMediaAccessUseCase(resolver: resolver)
        let projectID = UUID()
        let image = StoryImage(filename: "frame.png", mimeType: "image/png")

        let asset = try useCase.imageAsset(
            ownerUserID: "  alice  ",
            projectID: projectID,
            image: image
        )

        #expect(asset.id == image.id.uuidString)
        #expect(asset.mimeType == "image/png")
        #expect(asset.base64Data == nil)
        #expect(asset.url == URL(fileURLWithPath: "/safe/frame.png"))
        #expect(resolver.requests == [
            .init(ownerUserID: "alice", projectID: projectID, filename: "frame.png"),
        ])
    }

    @Test("video projection resolves the frozen local filename")
    func resolvesVideo() throws {
        let resolver = RecordingStoryMediaResolver()
        let useCase = StoryMediaAccessUseCase(resolver: resolver)
        let projectID = UUID()
        let video = StoryVideo(
            filename: "cut.mp4",
            jobID: "job-1",
            modelName: "video-model"
        )

        let url = try useCase.videoURL(
            ownerUserID: "alice",
            projectID: projectID,
            video: video
        )

        #expect(url == URL(fileURLWithPath: "/safe/cut.mp4"))
        #expect(resolver.requests.first?.filename == "cut.mp4")
    }

    @Test("invalid owners and media fail before the resolver")
    func rejectsInvalidAdmission() {
        let resolver = RecordingStoryMediaResolver()
        let useCase = StoryMediaAccessUseCase(resolver: resolver)
        #expect(throws: StoryMediaAccessError.invalidOwner) {
            _ = try useCase.imageAsset(
                ownerUserID: "   ",
                projectID: UUID(),
                image: .init(filename: "frame.png", mimeType: "image/png")
            )
        }
        #expect(throws: StoryMediaAccessError.invalidMedia) {
            _ = try useCase.imageAsset(
                ownerUserID: "alice",
                projectID: UUID(),
                image: .init(filename: "frame.txt", mimeType: "text/plain")
            )
        }
        #expect(resolver.requests.isEmpty)
    }

    @Test("non-file adapter results are rejected")
    func rejectsRemoteResolution() {
        let resolver = RecordingStoryMediaResolver(
            result: URL(string: "https://example.com/frame.png")!
        )
        let useCase = StoryMediaAccessUseCase(resolver: resolver)

        #expect(throws: StoryMediaAccessError.invalidResolution) {
            _ = try useCase.imageAsset(
                ownerUserID: "alice",
                projectID: UUID(),
                image: .init(filename: "frame.png", mimeType: "image/png")
            )
        }
    }
}

private final class RecordingStoryMediaResolver: StoryMediaAccessResolving, @unchecked Sendable {
    struct Request: Equatable {
        var ownerUserID: String
        var projectID: UUID
        var filename: String
    }

    let result: URL?
    var requests: [Request] = []

    init(result: URL? = nil) {
        self.result = result
    }

    func resolveStoryMediaURL(
        ownerUserID: String,
        projectID: UUID,
        filename: String
    ) throws -> URL {
        requests.append(.init(
            ownerUserID: ownerUserID,
            projectID: projectID,
            filename: filename
        ))
        return result ?? URL(fileURLWithPath: "/safe/\(filename)")
    }
}
