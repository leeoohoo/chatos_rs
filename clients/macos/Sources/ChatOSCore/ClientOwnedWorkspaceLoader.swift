import Foundation

public struct WorkspaceRelationsSnapshot: Sendable, Equatable {
    public let contacts: [WorkspaceContact]
    public let conversations: [WorkspaceConversation]

    public init(contacts: [WorkspaceContact], conversations: [WorkspaceConversation]) {
        self.contacts = contacts
        self.conversations = conversations
    }
}

public protocol WorkspaceRelationsRemoteServicing: Sendable {
    /// Must not fetch or synthesize project entities.
    func fetchWorkspaceRelations() async throws -> WorkspaceRelationsSnapshot
}

public struct ClientOwnedWorkspaceLoadResult: Sendable {
    public let snapshot: WorkspaceSnapshot
    public let remoteError: String?
}

/// Account-bound composition for migrated clients. No implicit import, remote project merge,
/// or fallback to server project authority. UI should publish loadLocal() before awaiting refresh().
/// Callers must discard stale authentication/refresh generations before publishing either result.
public struct ClientOwnedWorkspaceLoader: Sendable {
    private let registry: any ProjectRegistry
    private let remote: any WorkspaceRelationsRemoteServicing
    private let ownerUserID: String

    public init(registry: any ProjectRegistry, remote: any WorkspaceRelationsRemoteServicing, ownerUserID: String) throws {
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        self.registry = registry
        self.remote = remote
        self.ownerUserID = ownerUserID
    }

    public func loadLocal(deviceID: String?) async throws -> WorkspaceSnapshot {
        try await compose(relations: .init(contacts: [], conversations: []))
    }

    public func refresh(deviceID: String?) async throws -> ClientOwnedWorkspaceLoadResult {
        let relations: WorkspaceRelationsSnapshot
        let remoteError: String?
        do {
            relations = try await remote.fetchWorkspaceRelations()
            remoteError = nil
        } catch {
            try Task.checkCancellation()
            if error is CancellationError { throw error }
            relations = .init(contacts: [], conversations: [])
            remoteError = error.localizedDescription
        }
        // Read after the network await: a pending remote response cannot resurrect local deletions.
        let snapshot = try await compose(relations: relations)
        return .init(snapshot: snapshot, remoteError: remoteError)
    }

    private func compose(relations: WorkspaceRelationsSnapshot) async throws -> WorkspaceSnapshot {
        try Task.checkCancellation()
        let records = try await registry.list(ownerUserID: ownerUserID, includeInactive: false)
        let conversations = relations.conversations.filter { !$0.isArchived }.sorted {
            $0.updatedAt == $1.updatedAt ? $0.id < $1.id : $0.updatedAt > $1.updatedAt
        }
        let projects = try records.map { record in
            return WorkspaceProject(
                id: record.id, name: record.draft.name, rootPath: record.draft.rootPath,
                latestConversationID: conversations.first { $0.projectID == record.id }?.id,
                projectContext: try ProjectContextSnapshot(record: record)
            )
        }
        return .init(projects: projects, contacts: relations.contacts, conversations: relations.conversations)
    }
}
