import Foundation

public struct MessageTaskInspectorSession: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Presentation identity for an inline Task Inspector. Concrete Tasks remain
/// in the platform adapter; callbacks from a stopped or replaced presentation
/// cannot update state or restart realtime/polling work.
public struct MessageTaskInspectorLifecycleState: Sendable, Equatable {
    public private(set) var activeSession: MessageTaskInspectorSession?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func start() -> MessageTaskInspectorSession {
        if let activeSession {
            return activeSession
        }
        lastGeneration &+= 1
        let session = MessageTaskInspectorSession(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeSession = session
        return session
    }

    public func accepts(_ session: MessageTaskInspectorSession) -> Bool {
        activeSession == session
    }

    public mutating func stop() {
        activeSession = nil
    }
}
