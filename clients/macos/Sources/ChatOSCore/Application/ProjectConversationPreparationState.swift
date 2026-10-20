import Foundation

public struct ProjectConversationPreparationToken: Sendable, Equatable {
    public let projectID: String
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let generation: UInt64
    public let requestID: UUID

    public init(
        projectID: String,
        ownerUserID: String,
        accountGeneration: UInt64,
        generation: UInt64,
        requestID: UUID
    ) {
        self.projectID = projectID
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.generation = generation
        self.requestID = requestID
    }
}

public enum ProjectConversationPreparationPhase: Sendable, Equatable {
    case idle
    case preparing
    case failed(message: String)
}

/// Per-project latest-wins identity for preparing the durable project
/// Conversation. Different projects may prepare concurrently; replacing,
/// deleting, or signing out invalidates only the appropriate work.
public struct ProjectConversationPreparationState: Sendable, Equatable {
    public private(set) var activeTokens: [String: ProjectConversationPreparationToken] = [:]
    public private(set) var phases: [String: ProjectConversationPreparationPhase] = [:]
    private var lastGenerations: [String: UInt64] = [:]

    public init() {}

    public mutating func begin(
        projectID: String,
        ownerUserID: String,
        accountGeneration: UInt64
    ) -> ProjectConversationPreparationToken {
        let generation = (lastGenerations[projectID] ?? 0) &+ 1
        lastGenerations[projectID] = generation
        let token = ProjectConversationPreparationToken(
            projectID: projectID,
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            generation: generation,
            requestID: UUID()
        )
        activeTokens[projectID] = token
        phases[projectID] = .preparing
        return token
    }

    public func phase(for projectID: String) -> ProjectConversationPreparationPhase {
        phases[projectID] ?? .idle
    }

    public func isPreparing(projectID: String) -> Bool {
        activeTokens[projectID] != nil
    }

    public func accepts(
        _ token: ProjectConversationPreparationToken,
        ownerUserID: String?,
        accountGeneration: UInt64
    ) -> Bool {
        activeTokens[token.projectID] == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
    }

    @discardableResult
    public mutating func finish(
        _ token: ProjectConversationPreparationToken,
        errorMessage: String? = nil
    ) -> Bool {
        guard activeTokens[token.projectID] == token else { return false }
        activeTokens[token.projectID] = nil
        if let errorMessage {
            phases[token.projectID] = .failed(message: errorMessage)
        } else {
            phases[token.projectID] = nil
        }
        return true
    }

    public mutating func clearFailure(projectID: String) {
        guard case .failed = phase(for: projectID) else { return }
        phases[projectID] = nil
    }

    public mutating func cancel(projectID: String) {
        activeTokens[projectID] = nil
        phases[projectID] = nil
    }

    public mutating func reset() {
        activeTokens.removeAll()
        phases.removeAll()
    }
}
