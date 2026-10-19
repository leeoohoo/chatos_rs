import Foundation

public enum ConversationViewportMutationKind: Sendable, Equatable, Hashable {
    case timelineAnchor
    case markNewerContentRead
}

public struct ConversationViewportMutationToken: Sendable, Equatable {
    public let kind: ConversationViewportMutationKind
    public let generation: UInt64
    public let requestID: UUID

    public init(
        kind: ConversationViewportMutationKind,
        generation: UInt64,
        requestID: UUID
    ) {
        self.kind = kind
        self.generation = generation
        self.requestID = requestID
    }
}

/// Independent latest-wins identities for the two persisted viewport lanes.
/// The platform adapter owns concrete Tasks and rejects presentation updates
/// after the Conversation has been deactivated or a lane has been replaced.
public struct ConversationViewportMutationState: Sendable, Equatable {
    public private(set) var activeTokens: [
        ConversationViewportMutationKind: ConversationViewportMutationToken
    ] = [:]
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func begin(
        _ kind: ConversationViewportMutationKind
    ) -> ConversationViewportMutationToken {
        lastGeneration &+= 1
        let token = ConversationViewportMutationToken(
            kind: kind,
            generation: lastGeneration,
            requestID: UUID()
        )
        activeTokens[kind] = token
        return token
    }

    public func accepts(_ token: ConversationViewportMutationToken) -> Bool {
        activeTokens[token.kind] == token
    }

    @discardableResult
    public mutating func finish(_ token: ConversationViewportMutationToken) -> Bool {
        guard accepts(token) else { return false }
        activeTokens[token.kind] = nil
        return true
    }

    public mutating func reset() {
        activeTokens.removeAll()
    }
}
