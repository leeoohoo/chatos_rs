import ChatOSCore

enum AgentMessageAttachmentImageLoader {
    static let maximumBytes = ConversationAttachmentImageLoader.maximumBytes

    static func load(
        _ payload: ProjectAgentMessageAttachmentPayload,
        maximumDisplayPixelSize: Int = 2_048
    ) async throws -> AppDecodedThumbnail {
        let attachment = payload.attachment
        guard attachment.kind == .image,
              attachment.size > 0,
              attachment.size <= maximumBytes,
              let sha256 = attachment.sha256,
              sha256.utf8.count == 64 else {
            throw AgentAttachmentPresentationError.invalidSize
        }
        do {
            return try await ConversationAttachmentImageLoader.load(
                url: payload.localFileURL,
                sha256: sha256,
                expectedBytes: attachment.size,
                maximumDisplayPixelSize: maximumDisplayPixelSize
            )
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw AgentAttachmentPresentationError.invalidSize
        }
    }
}
