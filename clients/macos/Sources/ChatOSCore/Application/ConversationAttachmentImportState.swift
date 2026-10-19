import Foundation

public struct ConversationAttachmentImportToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Identity for asynchronous attachment imports targeting the current composer
/// draft. Multiple imports may run concurrently, but clearing the composer for
/// a send invalidates every pending import so late files cannot enter the next
/// message.
public struct ConversationAttachmentImportState: Sendable, Equatable {
    private var generation: UInt64 = 0
    private var activeRequestIDs: Set<UUID> = []

    public init() {}

    public mutating func begin() -> ConversationAttachmentImportToken {
        let token = ConversationAttachmentImportToken(
            generation: generation,
            requestID: UUID()
        )
        activeRequestIDs.insert(token.requestID)
        return token
    }

    public func accepts(_ token: ConversationAttachmentImportToken) -> Bool {
        token.generation == generation
            && activeRequestIDs.contains(token.requestID)
    }

    @discardableResult
    public mutating func finish(_ token: ConversationAttachmentImportToken) -> Bool {
        guard accepts(token) else { return false }
        activeRequestIDs.remove(token.requestID)
        return true
    }

    public mutating func reset() {
        generation &+= 1
        activeRequestIDs.removeAll()
    }
}
