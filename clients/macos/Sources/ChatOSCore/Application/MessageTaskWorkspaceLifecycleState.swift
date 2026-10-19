import Foundation

public struct MessageTaskWorkspaceSession: Sendable, Equatable {
    public let generation: UInt64
    public let requestID: UUID

    public init(generation: UInt64, requestID: UUID) {
        self.generation = generation
        self.requestID = requestID
    }
}

/// Pure presentation lifecycle for the Task Graph workspace.
///
/// The macOS adapter owns concrete Tasks. This state supplies a session identity
/// so initial loading, manual refresh, realtime, and polling cannot write back or
/// restart background work after the workspace has disappeared.
public struct MessageTaskWorkspaceLifecycleState: Sendable, Equatable {
    public private(set) var activeSession: MessageTaskWorkspaceSession?
    private var lastGeneration: UInt64 = 0

    public init() {}

    public mutating func start() -> MessageTaskWorkspaceSession {
        if let activeSession {
            return activeSession
        }
        lastGeneration &+= 1
        let session = MessageTaskWorkspaceSession(
            generation: lastGeneration,
            requestID: UUID()
        )
        activeSession = session
        return session
    }

    public func accepts(_ session: MessageTaskWorkspaceSession) -> Bool {
        activeSession == session
    }

    public mutating func stop() {
        activeSession = nil
    }
}
