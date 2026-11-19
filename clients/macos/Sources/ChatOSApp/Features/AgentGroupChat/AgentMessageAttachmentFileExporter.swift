import ChatOSCore
import Foundation

enum AgentMessageAttachmentFileExporter {
    static let maximumBytes = AgentMessageAttachmentDataLoader.maximumBytes

    static func export(
        _ payload: ProjectAgentMessageAttachmentPayload,
        to destinationURL: URL
    ) async throws {
        let attachment = payload.attachment
        guard payload.localFileURL.isFileURL,
              destinationURL.isFileURL,
              attachment.size > 0,
              attachment.size <= maximumBytes,
              let sha256 = attachment.sha256,
              sha256.utf8.count == 64 else {
            throw AgentAttachmentPresentationError.invalidSize
        }
        do {
            try await AppCancellableDetachedWork.run {
                try AppVerifiedFileExporter.export(
                    sourceURL: payload.localFileURL,
                    destinationURL: destinationURL,
                    expectedBytes: attachment.size,
                    expectedSHA256: sha256.lowercased(),
                    temporaryPrefix: ".chatos-agent-export-"
                )
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch AppVerifiedFileExporter.VerificationError.invalidSource {
            throw AgentAttachmentPresentationError.invalidSize
        }
    }
}
