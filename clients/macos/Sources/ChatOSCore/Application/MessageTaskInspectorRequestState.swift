import Foundation

public enum MessageTaskInspectorRequestLane: Sendable, Equatable {
    case snapshot
    case olderEvents
}

public struct MessageTaskInspectorRequestToken: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID
    public let lane: MessageTaskInspectorRequestLane
    public let contextID: String

    public init(
        generation: UInt64,
        requestID: UUID,
        lane: MessageTaskInspectorRequestLane,
        contextID: String
    ) {
        self.generation = generation
        self.requestID = requestID
        self.lane = lane
        self.contextID = contextID
    }
}

/// Request identity for the selected Task Inspector. Snapshot requests are
/// latest-wins across task/detail/run views. Pagination is independent, but a
/// new snapshot invalidates it because its event cursor belongs to old data.
public struct MessageTaskInspectorRequestState: Sendable, Equatable {
    private var lastGeneration: UInt64 = 0
    private var activeSnapshot: MessageTaskInspectorRequestToken?
    private var activeOlderEvents: MessageTaskInspectorRequestToken?

    public init() {}

    public mutating func beginSnapshot(
        contextID: String
    ) -> MessageTaskInspectorRequestToken {
        activeOlderEvents = nil
        let token = makeToken(lane: .snapshot, contextID: contextID)
        activeSnapshot = token
        return token
    }

    public mutating func beginOlderEvents(
        contextID: String
    ) -> MessageTaskInspectorRequestToken {
        let token = makeToken(lane: .olderEvents, contextID: contextID)
        activeOlderEvents = token
        return token
    }

    public func accepts(_ token: MessageTaskInspectorRequestToken) -> Bool {
        switch token.lane {
        case .snapshot:
            activeSnapshot == token
        case .olderEvents:
            activeOlderEvents == token
        }
    }

    @discardableResult
    public mutating func finish(_ token: MessageTaskInspectorRequestToken) -> Bool {
        guard accepts(token) else { return false }
        switch token.lane {
        case .snapshot:
            activeSnapshot = nil
        case .olderEvents:
            activeOlderEvents = nil
        }
        return true
    }

    public mutating func reset() {
        activeSnapshot = nil
        activeOlderEvents = nil
    }

    private mutating func makeToken(
        lane: MessageTaskInspectorRequestLane,
        contextID: String
    ) -> MessageTaskInspectorRequestToken {
        lastGeneration &+= 1
        return .init(
            generation: lastGeneration,
            requestID: UUID(),
            lane: lane,
            contextID: contextID
        )
    }
}
