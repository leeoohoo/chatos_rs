import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story image import application boundary")
struct StoryImageImportUseCaseTests {
    @Test("generated imports freeze owner, target, confirmation and active video preservation")
    func importsGeneratedAsset() async throws {
        let importer = RecordingStoryImageImporter()
        let useCase = StoryImageImportUseCase(importer: importer)
        let project = Self.project()
        let preserving: Set<String> = ["segment-1"]
        let command = try useCase.prepare(
            ownerUserID: "  alice  ",
            project: project,
            target: .resource(id: "prop-1"),
            preservingVideoSegmentIDs: preserving,
            confirmsImportedImage: false
        )
        let asset = GeneratedMediaAsset(
            id: "generated-1",
            mimeType: "image/png",
            base64Data: "cG5n"
        )

        let result = try await useCase.importGeneratedAsset(asset, command: command)

        #expect(command.ownerUserID == "alice")
        #expect(command.preservingVideoSegmentIDs == preserving)
        #expect(command.confirmsImportedImage == false)
        #expect(result.props.first?.media.images.count == 1)
        #expect(await importer.calls == [.generated(asset.id)])
    }

    @Test("local imports dispatch the selected file only through the adapter")
    func importsLocalFile() async throws {
        let importer = RecordingStoryImageImporter()
        let useCase = StoryImageImportUseCase(importer: importer)
        let command = try useCase.prepare(
            ownerUserID: "alice",
            project: Self.project(),
            target: .resource(id: "prop-1"),
            preservingVideoSegmentIDs: []
        )
        let url = URL(fileURLWithPath: "/tmp/story-input.png")

        let result = try await useCase.importLocalFile(url, command: command)

        #expect(result.props.first?.media.images.count == 1)
        #expect(await importer.calls == [.local(url)])
    }

    @Test("blank owners and missing or provider-owned targets fail before import")
    func rejectsInvalidAdmission() throws {
        let importer = RecordingStoryImageImporter()
        let useCase = StoryImageImportUseCase(importer: importer)
        let project = Self.project()
        #expect(throws: StoryImageImportError.invalidOwner) {
            _ = try useCase.prepare(
                ownerUserID: "   ",
                project: project,
                target: .resource(id: "prop-1"),
                preservingVideoSegmentIDs: []
            )
        }
        #expect(throws: StoryImageImportError.invalidTarget) {
            _ = try useCase.prepare(
                ownerUserID: "alice",
                project: project,
                target: .resource(id: "missing"),
                preservingVideoSegmentIDs: []
            )
        }

        var pending = project
        var segment = StorySegment(
            id: "segment-1",
            title: "Segment",
            synopsis: "Segment",
            sourceRange: .init(start: 0, end: 1)
        )
        segment.attempt = .init(
            modelConfigID: "video",
            prompt: "prompt",
            size: "768P",
            ratio: "16:9"
        )
        pending.source = "x"
        pending.segments = [segment]
        #expect(throws: StoryImageImportError.invalidTarget) {
            _ = try useCase.prepare(
                ownerUserID: "alice",
                project: pending,
                target: .frame(segmentID: "segment-1", role: .first),
                preservingVideoSegmentIDs: []
            )
        }
    }

    @Test("adapter results must append exactly one image to the frozen target")
    func rejectsMismatchedResult() async throws {
        let importer = RecordingStoryImageImporter(appendImage: false)
        let useCase = StoryImageImportUseCase(importer: importer)
        let command = try useCase.prepare(
            ownerUserID: "alice",
            project: Self.project(),
            target: .resource(id: "prop-1"),
            preservingVideoSegmentIDs: []
        )

        await #expect(throws: StoryImageImportError.resultMismatch) {
            _ = try await useCase.importGeneratedAsset(
                .init(id: "generated-1", mimeType: "image/png"),
                command: command
            )
        }
    }

    @Test("import lifecycle rejects replacement accounts and projects")
    func validatesLifecycle() throws {
        let useCase = StoryImageImportUseCase(importer: RecordingStoryImageImporter())
        let command = try useCase.prepare(
            ownerUserID: "alice",
            project: Self.project(),
            target: .resource(id: "prop-1"),
            preservingVideoSegmentIDs: []
        )
        var state = StoryImageImportState()
        let session = UUID()
        let token = state.begin(
            command: command,
            accountSessionID: session,
            source: .generatedAsset
        )
        #expect(token != nil)
        #expect(state.begin(
            command: command,
            accountSessionID: session,
            source: .localFile
        ) == nil)
        if let token {
            #expect(state.accepts(
                token,
                ownerUserID: "alice",
                accountSessionID: session,
                projectID: command.project.id
            ))
            #expect(!state.accepts(
                token,
                ownerUserID: "bob",
                accountSessionID: session,
                projectID: command.project.id
            ))
            #expect(!state.accepts(
                token,
                ownerUserID: "alice",
                accountSessionID: session,
                projectID: UUID()
            ))
            let didFinish = state.finish(token)
            #expect(didFinish)
        }
        #expect(state.activeToken == nil)
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
        project.props = [
            .init(
                id: "prop-1",
                name: "Key",
                description: "An old key",
                imagePrompt: "An old brass key"
            ),
        ]
        return project
    }
}

private actor RecordingStoryImageImporter: StoryImageImporting {
    enum Call: Equatable {
        case generated(String)
        case local(URL)
    }

    let appendImage: Bool
    var calls: [Call] = []

    init(appendImage: Bool = true) {
        self.appendImage = appendImage
    }

    func importGeneratedStoryImage(
        command: StoryImageImportCommand,
        asset: GeneratedMediaAsset
    ) async throws -> StoryProject {
        calls.append(.generated(asset.id))
        return try result(for: command)
    }

    func importLocalStoryImage(
        command: StoryImageImportCommand,
        fileURL: URL
    ) async throws -> StoryProject {
        calls.append(.local(fileURL))
        return try result(for: command)
    }

    private func result(for command: StoryImageImportCommand) throws -> StoryProject {
        guard appendImage else { return command.project }
        var project = command.project
        switch command.target {
        case .resource(let id):
            guard var resource = project.resource(id: id) else {
                throw StoryImageImportError.invalidTarget
            }
            resource.media.images.append(.init(filename: "imported.png", mimeType: "image/png"))
            try project.replaceResource(resource)
        case .frame(let segmentID, let role):
            guard let index = project.segments.firstIndex(where: { $0.id == segmentID }) else {
                throw StoryImageImportError.invalidTarget
            }
            let image = StoryImage(filename: "imported.png", mimeType: "image/png")
            switch role {
            case .first:
                project.segments[index].firstFrames.images.append(image)
            case .last:
                project.segments[index].lastFrames.images.append(image)
            }
        }
        return project
    }
}
