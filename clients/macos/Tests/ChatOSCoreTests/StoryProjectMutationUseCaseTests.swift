import Foundation
import Testing
@testable import ChatOSCore

@Suite("Story project mutation application boundary")
struct StoryProjectMutationUseCaseTests {
    @Test("creation validates models, snapshots video capabilities, and only persists")
    func createsProject() async throws {
        let persistence = RecordingStoryProjectMutationPersistence()
        let useCase = StoryProjectMutationUseCase(persistence: persistence)
        let draft = Self.project()

        let saved = try await useCase.create(
            draft,
            ownerUserID: "  alice  ",
            availableModels: Self.models
        )

        #expect(saved.id == draft.id)
        #expect(saved.version == 2)
        #expect(saved.models.supportedVideoDurations == Array(4...15))
        let requests = await persistence.requests
        #expect(requests.count == 1)
        #expect(requests.first?.ownerUserID == "alice")
        #expect(requests.first?.preservingVideoSegmentIDs == [])
        #expect(requests.first?.project.models.supportedVideoDurations == Array(4...15))
    }

    @Test("missing credentials and blank owners fail before persistence")
    func rejectsInvalidAdmission() async {
        let persistence = RecordingStoryProjectMutationPersistence()
        let useCase = StoryProjectMutationUseCase(persistence: persistence)
        var unavailable = Self.models
        unavailable[1].hasAPIKey = false

        await #expect(throws: StoryError.self) {
            _ = try await useCase.create(
                Self.project(),
                ownerUserID: "alice",
                availableModels: unavailable
            )
        }
        await #expect(throws: StoryProjectMutationError.invalidOwner) {
            _ = try await useCase.create(
                Self.project(),
                ownerUserID: "   ",
                availableModels: Self.models
            )
        }
        #expect(await persistence.requests.isEmpty)
    }

    @Test("settings rebuild from the canonical project and preserve active video ownership")
    func updatesOnlySettings() async throws {
        let persistence = RecordingStoryProjectMutationPersistence()
        let useCase = StoryProjectMutationUseCase(persistence: persistence)
        var current = Self.project()
        current.source = "canonical source"
        current.style = "canonical style"
        current.summary = "canonical summary"
        let createdAt = Date(timeIntervalSince1970: 10)
        current.createdAt = createdAt
        let preserving: Set<String> = ["segment-2"]

        let saved = try await useCase.updateSettings(
            current: current,
            change: .init(
                projectID: current.id,
                title: "  Updated Story  ",
                description: "updated description",
                models: .init(
                    textModelID: "video",
                    imageModelID: "image",
                    videoModelID: "video"
                )
            ),
            ownerUserID: "alice",
            availableModels: Self.models,
            preservingVideoSegmentIDs: preserving
        )

        #expect(saved.title == "Updated Story")
        #expect(saved.description == "updated description")
        #expect(saved.source == "canonical source")
        #expect(saved.style == "canonical style")
        #expect(saved.summary == "canonical summary")
        #expect(saved.createdAt == createdAt)
        #expect(saved.id == current.id)
        #expect(saved.version == current.version)
        #expect(saved.models.textModelID == "video")
        #expect(saved.models.supportedVideoDurations == Array(4...15))
        #expect(await persistence.requests.first?.preservingVideoSegmentIDs == preserving)
    }

    @Test("unresolved provider work keeps the image and video model identities frozen")
    func freezesModelsForUnresolvedJobs() async {
        let persistence = RecordingStoryProjectMutationPersistence()
        let useCase = StoryProjectMutationUseCase(persistence: persistence)
        var current = Self.project()
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
        current.source = "x"
        current.segments = [segment]

        await #expect(throws: StoryError.self) {
            _ = try await useCase.updateSettings(
                current: current,
                change: .init(
                    projectID: current.id,
                    title: current.title,
                    description: current.description,
                    models: .init(
                        textModelID: "text",
                        imageModelID: "image",
                        videoModelID: "text"
                    )
                ),
                ownerUserID: "alice",
                availableModels: Self.models,
                preservingVideoSegmentIDs: []
            )
        }

        var imageCurrent = Self.project()
        var character = StoryCharacter(
            id: "character-1",
            name: "Character",
            profile: .init(
                isProtagonist: true,
                roleInStory: "Lead",
                appearance: "Short hair",
                personality: "Calm",
                motivation: "Finish the story",
                relationships: "Independent",
                costume: "Dark coat",
                consistencyNotes: "Keep the same appearance"
            ),
            imagePrompt: "Character portrait"
        )
        character.media.generationAttemptID = UUID()
        imageCurrent.characters = [character]
        await #expect(throws: StoryError.self) {
            _ = try await useCase.updateSettings(
                current: imageCurrent,
                change: .init(
                    projectID: imageCurrent.id,
                    title: imageCurrent.title,
                    description: imageCurrent.description,
                    models: .init(
                        textModelID: "text",
                        imageModelID: "text",
                        videoModelID: "video"
                    )
                ),
                ownerUserID: "alice",
                availableModels: Self.models,
                preservingVideoSegmentIDs: []
            )
        }
        #expect(await persistence.requests.isEmpty)
    }

    @Test("ordinary edits freeze the canonical identity and preserve active video ownership")
    func persistsOrdinaryEdit() async throws {
        let persistence = RecordingStoryProjectMutationPersistence()
        let useCase = StoryProjectMutationUseCase(persistence: persistence)
        var base = Self.project()
        base.source = "Original source"
        var edited = base
        edited.source = "Edited source"
        edited.style = "Edited style"
        let preserving: Set<String> = ["segment-1"]
        let command = try StoryProjectEditCommand(
            base: base,
            project: edited,
            preservingVideoSegmentIDs: preserving
        )

        let saved = try await useCase.persistEdit(command, ownerUserID: "  alice  ")

        #expect(saved.source == "Edited source")
        #expect(command.projectID == base.id)
        #expect(!command.baseFingerprint.isEmpty)
        let request = await persistence.requests.first
        #expect(request?.ownerUserID == "alice")
        #expect(request?.preservingVideoSegmentIDs == preserving)
        #expect(request?.project.models == base.models)
    }

    @Test("ordinary edits reject stale identity and settings changes before persistence")
    func rejectsInvalidOrdinaryEdit() async throws {
        let persistence = RecordingStoryProjectMutationPersistence()
        let useCase = StoryProjectMutationUseCase(persistence: persistence)
        let base = Self.project()
        var changedIdentity = base
        changedIdentity.id = UUID()
        let identityCommand = try StoryProjectEditCommand(
            base: base,
            project: changedIdentity,
            preservingVideoSegmentIDs: []
        )
        await #expect(throws: StoryProjectMutationError.projectMismatch) {
            _ = try await useCase.persistEdit(identityCommand, ownerUserID: "alice")
        }

        var changedSettings = base
        changedSettings.models.textModelID = "video"
        let settingsCommand = try StoryProjectEditCommand(
            base: base,
            project: changedSettings,
            preservingVideoSegmentIDs: []
        )
        await #expect(throws: StoryProjectMutationError.projectMismatch) {
            _ = try await useCase.persistEdit(settingsCommand, ownerUserID: "alice")
        }

        var changedRevision = base
        changedRevision.updatedAt = Date(timeIntervalSince1970: 1)
        let revisionCommand = try StoryProjectEditCommand(
            base: base,
            project: changedRevision,
            preservingVideoSegmentIDs: []
        )
        await #expect(throws: StoryProjectMutationError.projectMismatch) {
            _ = try await useCase.persistEdit(revisionCommand, ownerUserID: "alice")
        }
        #expect(await persistence.requests.isEmpty)
    }

    @Test("project identity and account lifecycle reject stale mutations")
    func validatesIdentityAndLifecycle() async {
        let persistence = RecordingStoryProjectMutationPersistence()
        let useCase = StoryProjectMutationUseCase(persistence: persistence)
        let current = Self.project()
        await #expect(throws: StoryProjectMutationError.projectMismatch) {
            _ = try await useCase.updateSettings(
                current: current,
                change: .init(
                    projectID: UUID(),
                    title: current.title,
                    description: current.description,
                    models: current.models
                ),
                ownerUserID: "alice",
                availableModels: Self.models,
                preservingVideoSegmentIDs: []
            )
        }

        var state = StoryProjectMutationState()
        let aliceSession = UUID()
        let request = state.begin(
            ownerUserID: "alice",
            accountSessionID: aliceSession,
            kind: .updateSettings(projectID: current.id)
        )
        #expect(request != nil)
        #expect(state.begin(
            ownerUserID: "alice",
            accountSessionID: aliceSession,
            kind: .create(projectID: UUID())
        ) == nil)
        if let request {
            #expect(state.accepts(request, ownerUserID: "alice", accountSessionID: aliceSession))
            #expect(!state.accepts(request, ownerUserID: "bob", accountSessionID: aliceSession))
            #expect(!state.accepts(request, ownerUserID: "alice", accountSessionID: UUID()))
            let didFinish = state.finish(request)
            #expect(didFinish)
        }
        #expect(state.activeToken == nil)
    }

    private static var models: [MediaGenerationModel] {
        [
            .init(
                id: "text",
                name: "Text",
                provider: "provider",
                modelName: "text-model",
                enabled: true,
                taskEnabled: false,
                hasAPIKey: true
            ),
            .init(
                id: "image",
                name: "Image",
                provider: "provider",
                modelName: "image-model",
                enabled: true,
                taskEnabled: false,
                hasAPIKey: true
            ),
            .init(
                id: "video",
                name: "Video",
                provider: "provider",
                modelName: "MiniMax-H3",
                enabled: true,
                taskEnabled: false,
                hasAPIKey: true
            ),
        ]
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
}

private actor RecordingStoryProjectMutationPersistence: StoryProjectMutationPersisting {
    struct Request: Equatable {
        var project: StoryProject
        var ownerUserID: String
        var preservingVideoSegmentIDs: Set<String>
    }

    var requests: [Request] = []

    func persistStoryProject(
        _ project: StoryProject,
        ownerUserID: String,
        preservingVideoSegmentIDs: Set<String>
    ) async throws -> StoryProject {
        requests.append(.init(
            project: project,
            ownerUserID: ownerUserID,
            preservingVideoSegmentIDs: preservingVideoSegmentIDs
        ))
        return project
    }
}
