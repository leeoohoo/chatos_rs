import ChatOSConnector
import ChatOSCore

struct AgentMessageAttachmentThumbnail: Sendable, Equatable {
    let identity: String
    let decoded: AppDecodedThumbnail

    var memoryCost: Int { decoded.memoryCost }

    init(attachment: ProjectAgentMessageAttachment, decoded: AppDecodedThumbnail) {
        identity = "\(attachment.id)|\(attachment.size)|\(attachment.sha256 ?? "")"
        self.decoded = decoded
    }

    static func == (
        lhs: AgentMessageAttachmentThumbnail,
        rhs: AgentMessageAttachmentThumbnail
    ) -> Bool {
        lhs.identity == rhs.identity
            && lhs.decoded.image.width == rhs.decoded.image.width
            && lhs.decoded.image.height == rhs.decoded.image.height
    }
}

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

enum AgentMessageAttachmentThumbnailLoader {
    static let maximumDisplayPixelSize = 1_024

    static func load(
        requests: [AgentAttachmentThumbnailLoadRequest],
        roomID: String,
        ownerUserID: String,
        store: SQLiteAgentGroupChatStore,
        maximumMemoryCost: Int
    ) async throws -> [String: AgentMessageAttachmentThumbnail] {
        guard maximumMemoryCost > 0 else { return [:] }
        var result: [String: AgentMessageAttachmentThumbnail] = [:]
        result.reserveCapacity(requests.count)
        var remainingMemoryCost = maximumMemoryCost
        for request in requests {
            try Task.checkCancellation()
            guard let payload = try await store.messageAttachment(
                ownerUserID: ownerUserID,
                roomID: roomID,
                messageID: request.messageID,
                attachmentID: request.attachment.id
            ) else { continue }
            let decoded: AppDecodedThumbnail
            do {
                decoded = try await AgentMessageAttachmentImageLoader.load(
                    payload,
                    maximumDisplayPixelSize: maximumDisplayPixelSize
                )
            } catch is CancellationError {
                throw CancellationError()
            } catch is AgentAttachmentPresentationError {
                // A legacy or corrupt image remains available as an attachment chip; it must
                // not prevent proposal and task metadata from completing their supplementary load.
                continue
            }
            let thumbnail = AgentMessageAttachmentThumbnail(
                attachment: payload.attachment,
                decoded: decoded
            )
            guard thumbnail.memoryCost <= remainingMemoryCost else { continue }
            result[request.attachment.id] = thumbnail
            remainingMemoryCost -= thumbnail.memoryCost
        }
        return result
    }
}
