import Foundation
import Testing
@testable import ChatOSCore

@Suite("Media Studio workflow validation")
struct MediaStudioWorkflowModelsTests {
    @Test("accepts a linear text image and terminal video workflow")
    func acceptsValidWorkflow() throws {
        let workflow = MediaStudioWorkflow(
            name: "Campaign",
            nodes: [
                Self.node(.text),
                Self.node(.image, references: [Self.reference()]),
                Self.node(.video, references: [Self.reference()]),
            ]
        )

        try MediaStudioWorkflowValidator.validate(workflow)
    }

    @Test("rejects a video node before another node")
    func rejectsNonTerminalVideo() {
        let workflow = MediaStudioWorkflow(
            name: "Invalid",
            nodes: [Self.node(.video), Self.node(.text)]
        )

        #expect(throws: MediaStudioWorkflowValidationError.invalidVideoPosition) {
            try MediaStudioWorkflowValidator.validate(workflow)
        }
    }

    @Test("video nodes accept only one node reference")
    func limitsVideoReferences() {
        let workflow = MediaStudioWorkflow(
            name: "Invalid",
            nodes: [Self.node(.video, references: [Self.reference(), Self.reference()])]
        )

        #expect(throws: MediaStudioWorkflowValidationError.tooManyReferences) {
            try MediaStudioWorkflowValidator.validate(workflow)
        }
    }

    @Test("video nodes use and enforce the MiniMax-H3 protocol")
    func enforcesMiniMaxVideoOptions() throws {
        let node = Self.node(.video)
        #expect(node.videoSize == "768P")
        #expect(node.videoSeconds == 4)
        try MediaStudioWorkflowValidator.validate(.init(name: "Valid", nodes: [node]))

        var invalidSize = node
        invalidSize.videoSize = "1280x720"
        #expect(throws: MediaStudioWorkflowValidationError.invalidVideoOptions) {
            try MediaStudioWorkflowValidator.validate(.init(name: "Invalid", nodes: [invalidSize]))
        }

        var invalidDuration = node
        invalidDuration.videoSeconds = 16
        #expect(throws: MediaStudioWorkflowValidationError.invalidVideoOptions) {
            try MediaStudioWorkflowValidator.validate(.init(name: "Invalid", nodes: [invalidDuration]))
        }
    }

    @Test("legacy workflow video options migrate to the MiniMax-H3 protocol")
    func migratesLegacyVideoOptions() throws {
        var node = Self.node(.video)
        node.videoSize = "1280x720"
        node.videoSeconds = 120
        var workflow = MediaStudioWorkflow(name: "Legacy", nodes: [node])

        MediaStudioWorkflowValidator.migrateLegacyVideoOptions(in: &workflow)

        #expect(workflow.nodes[0].videoSize == "768P")
        #expect(workflow.nodes[0].videoSeconds == 15)
        #expect(workflow.nodes[0].videoRatio == "16:9")
        try MediaStudioWorkflowValidator.validate(workflow)
    }

    @Test("migration does not hide unknown video options")
    func rejectsUnknownOptionsAfterMigration() {
        var node = Self.node(.video)
        node.videoSize = "unknown"
        node.videoRatio = "unknown"
        node.videoSeconds = 121
        var workflow = MediaStudioWorkflow(name: "Invalid", nodes: [node])

        MediaStudioWorkflowValidator.migrateLegacyVideoOptions(in: &workflow)

        #expect(throws: MediaStudioWorkflowValidationError.invalidVideoOptions) {
            try MediaStudioWorkflowValidator.validate(workflow)
        }
    }

    private static func node(
        _ kind: MediaStudioWorkflowNodeKind,
        references: [MediaStudioWorkflowImageReference] = []
    ) -> MediaStudioWorkflowNode {
        .init(
            kind: kind,
            modelConfigID: "model",
            prompt: "Do the work",
            referenceImages: references
        )
    }

    private static func reference() -> MediaStudioWorkflowImageReference {
        .init(
            name: "reference.png",
            mimeType: "image/png",
            filename: "\(UUID().uuidString).png",
            byteCount: 128
        )
    }
}
