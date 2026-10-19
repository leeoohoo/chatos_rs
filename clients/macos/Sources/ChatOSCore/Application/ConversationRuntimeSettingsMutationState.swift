import Foundation

public struct ConversationRuntimeSettingsMutationToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Latest-wins identity for optimistic Conversation runtime-setting changes.
/// The UI adapter owns the concrete Task and rollback snapshot; only the newest
/// mutation may apply a Host response, restore optimistic state, or end loading.
public struct ConversationRuntimeSettingsMutationState: Sendable, Equatable {
    public private(set) var activeToken: ConversationRuntimeSettingsMutationToken?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin() -> ConversationRuntimeSettingsMutationToken {
        lastGeneration &+= 1
        let token = ConversationRuntimeSettingsMutationToken(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(_ token: ConversationRuntimeSettingsMutationToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    public mutating func finish(
        _ token: ConversationRuntimeSettingsMutationToken
    ) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
