import ChatOSCore

struct AgentAttachmentThumbnailLoadRequest: Equatable {
    let messageID: String
    let attachment: ProjectAgentMessageAttachment
}

enum AgentAttachmentThumbnailCachePolicy {
    static let maximumPlannedSourceBytes = 48 * 1_024 * 1_024
    static let maximumCachedMemoryCost = 64 * 1_024 * 1_024
    static let maximumCachedCount = 16

    /// Selects the newest image attachments whose source files fit the bounded eager-I/O plan.
    /// The cache retains only downsampled pixels; compressed attachment bytes stay on disk.
    static func loadPlan(
        messages: [ProjectAgentMessage],
        maximumSourceBytes: Int = maximumPlannedSourceBytes,
        maximumCount: Int = maximumCachedCount
    ) -> [AgentAttachmentThumbnailLoadRequest] {
        guard maximumSourceBytes > 0, maximumCount > 0 else { return [] }

        var requests: [AgentAttachmentThumbnailLoadRequest] = []
        var selectedIDs: Set<String> = []
        var remainingSourceBytes = maximumSourceBytes

        for message in messages.reversed() {
            for attachment in message.attachmentItems where attachment.kind == .image {
                guard requests.count < maximumCount else { return requests }
                guard attachment.size > 0,
                      attachment.size <= remainingSourceBytes,
                      selectedIDs.insert(attachment.id).inserted else { continue }
                requests.append(.init(messageID: message.id, attachment: attachment))
                remainingSourceBytes -= attachment.size
            }
        }
        return requests
    }

    static func retainedThumbnails(
        _ thumbnailsByID: [String: AgentMessageAttachmentThumbnail],
        for plan: [AgentAttachmentThumbnailLoadRequest],
        maximumMemoryCost: Int = maximumCachedMemoryCost
    ) -> [String: AgentMessageAttachmentThumbnail] {
        guard maximumMemoryCost > 0 else { return [:] }
        var retained: [String: AgentMessageAttachmentThumbnail] = [:]
        retained.reserveCapacity(min(thumbnailsByID.count, plan.count))
        var remainingMemoryCost = maximumMemoryCost
        for request in plan {
            guard let thumbnail = thumbnailsByID[request.attachment.id],
                  thumbnail.memoryCost <= remainingMemoryCost else { continue }
            retained[request.attachment.id] = thumbnail
            remainingMemoryCost -= thumbnail.memoryCost
        }
        return retained
    }

    static func remainingMemoryCost(
        after thumbnailsByID: [String: AgentMessageAttachmentThumbnail],
        maximumMemoryCost: Int = maximumCachedMemoryCost
    ) -> Int {
        guard maximumMemoryCost > 0 else { return 0 }
        return max(0, maximumMemoryCost - thumbnailsByID.values.reduce(0) { partial, thumbnail in
            let (sum, overflow) = partial.addingReportingOverflow(thumbnail.memoryCost)
            return overflow ? Int.max : sum
        })
    }

    static func missingRequests(
        in plan: [AgentAttachmentThumbnailLoadRequest],
        cachedThumbnailsByID: [String: AgentMessageAttachmentThumbnail]
    ) -> [AgentAttachmentThumbnailLoadRequest] {
        plan.filter { cachedThumbnailsByID[$0.attachment.id] == nil }
    }
}
