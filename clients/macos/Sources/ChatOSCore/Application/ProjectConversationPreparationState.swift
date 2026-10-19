import Foundation

public struct ProjectConversationPreparationToken: Sendable, Equatable {
    public let projectID: String
    public let generation: UInt64
    public let requestID: UUID

    public init(projectID: String, generation: UInt64, requestID: UUID) {
        self.projectID = projectID
        self.generation = generation
        self.requestID = requestID
    }
}

/// Per-project latest-wins identity for preparing the durable project
/// Conversation. Different projects may prepare concurrently; replacing,
/// deleting, or signing out invalidates only the appropriate work.
public struct ProjectConversationPreparationState: Sendable, Equatable {
    public private(set) var activeTokens: [String: ProjectConversationPreparationToken] = [:]
    private var lastGenerations: [String: UInt64] = [:]

    public init() {}

    public mutating func begin(projectID: String) -> ProjectConversationPreparationToken {
        let generation = (lastGenerations[projectID] ?? 0) &+ 1
        lastGenerations[projectID] = generation
        let token = ProjectConversationPreparationToken(
            projectID: projectID,
            generation: generation,
            requestID: UUID()
        )
        activeTokens[projectID] = token
        return token
    }

    public func accepts(_ token: ProjectConversationPreparationToken) -> Bool {
        activeTokens[token.projectID] == token
    }

    @discardableResult
    public mutating func finish(_ token: ProjectConversationPreparationToken) -> Bool {
        guard accepts(token) else { return false }
        activeTokens[token.projectID] = nil
        return true
    }

    public mutating func cancel(projectID: String) {
        activeTokens[projectID] = nil
    }

    public mutating func reset() {
        activeTokens.removeAll()
    }
}
