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
