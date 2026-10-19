import Foundation

public struct ConversationRuntimeSettingsLoadToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Latest-wins identity for Conversation runtime-settings snapshots. A newer
/// load or an optimistic settings mutation invalidates older Host snapshots so
/// they cannot overwrite the current composer configuration.
public struct ConversationRuntimeSettingsLoadState: Sendable, Equatable {
    public private(set) var activeToken: ConversationRuntimeSettingsLoadToken?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin() -> ConversationRuntimeSettingsLoadToken {
        lastGeneration &+= 1
        let token = ConversationRuntimeSettingsLoadToken(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: ConversationRuntimeSettingsLoadToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(_ token: ConversationRuntimeSettingsLoadToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
