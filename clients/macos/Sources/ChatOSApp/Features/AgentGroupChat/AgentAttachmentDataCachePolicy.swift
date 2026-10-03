import ChatOSCore
import Foundation

struct AgentAttachmentDataLoadRequest: Equatable {
    let messageID: String
    let attachment: ProjectAgentMessageAttachment
}

enum AgentAttachmentDataCachePolicy {
    static let maximumCachedBytes = 48 * 1_024 * 1_024
    static let maximumCachedCount = 24

    /// Selects the newest image attachments that fit the compressed-data cache. Attachment
    /// records and files remain durable in SQLite/the attachment vault; this only bounds the
    /// eager in-memory bytes used to render recent thumbnails.
    static func loadPlan(
        messages: [ProjectAgentMessage],
        maximumBytes: Int = maximumCachedBytes,
        maximumCount: Int = maximumCachedCount
    ) -> [AgentAttachmentDataLoadRequest] {
        guard maximumBytes > 0, maximumCount > 0 else { return [] }

        var requests: [AgentAttachmentDataLoadRequest] = []
        var selectedIDs: Set<String> = []
        var remainingBytes = maximumBytes

        for message in messages.reversed() {
            for attachment in message.attachmentItems where attachment.kind == .image {
                guard requests.count < maximumCount else { return requests }
                guard attachment.size > 0,
                      attachment.size <= remainingBytes,
                      selectedIDs.insert(attachment.id).inserted else { continue }
                requests.append(.init(messageID: message.id, attachment: attachment))
                remainingBytes -= attachment.size
            }
        }
        return requests
    }

    static func retainedData(
        _ dataByID: [String: Data],
        for plan: [AgentAttachmentDataLoadRequest]
    ) -> [String: Data] {
        var retained: [String: Data] = [:]
        retained.reserveCapacity(min(dataByID.count, plan.count))
        for request in plan {
            if let data = dataByID[request.attachment.id] {
                retained[request.attachment.id] = data
            }
        }
        return retained
    }

    static func missingRequests(
        in plan: [AgentAttachmentDataLoadRequest],
        cachedDataByID: [String: Data]
    ) -> [AgentAttachmentDataLoadRequest] {
        plan.filter { cachedDataByID[$0.attachment.id] == nil }
    }
}
