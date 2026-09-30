import ChatOSCore
import Foundation

public actor NativeLocalAgentWorkspaceService: WorkspaceRelationsRemoteServicing {
    public static let mainContact = WorkspaceContact(
        id: "jiguli",
        agentID: "jiguli",
        name: "叽咕狸",
        status: "active"
    )

    private let client: NativeLocalAgentConversationClient
    private var ownerUserID: String?

    public init(host: any LocalAgentHostClientServicing) {
        client = NativeLocalAgentConversationClient(host: host)
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
    }

    public func reset() {
        ownerUserID = nil
    }

    public func fetchWorkspaceRelations() async throws -> WorkspaceRelationsSnapshot {
        let ownerUserID = try requireOwner()
        var conversations = try await listAll(ownerUserID: ownerUserID)
        if !conversations.contains(where: { $0.resource == .init(kind: .contact, resourceID: Self.mainContact.id) }) {
            do {
                let created = try await client.create(
                    ownerUserID: ownerUserID,
                    conversationID: "conversation_\(UUID().uuidString.lowercased())",
                    title: Self.mainContact.name,
                    resource: .init(kind: .contact, resourceID: Self.mainContact.id)
                )
                conversations.append(created.conversation)
            } catch {
                let refreshed = try await listAll(ownerUserID: ownerUserID)
                guard refreshed.contains(where: {
                    $0.resource == .init(kind: .contact, resourceID: Self.mainContact.id)
                }) else { throw error }
                conversations = refreshed
            }
        }
        return WorkspaceRelationsSnapshot(
            contacts: [Self.mainContact],
            conversations: conversations.compactMap(Self.mapConversation)
        )
    }

    func listAll(ownerUserID: String) async throws -> [LocalAgentConversationRecord] {
        var records: [LocalAgentConversationRecord] = []
        var beforeUpdatedAtUnixMs: Int64?
        var beforeConversationID: String?
        repeat {
            let page = try await client.list(
                ownerUserID: ownerUserID,
                beforeUpdatedAtUnixMs: beforeUpdatedAtUnixMs,
                beforeConversationID: beforeConversationID,
                limit: 200
            )
            records.append(contentsOf: page.conversations)
            beforeUpdatedAtUnixMs = page.nextBeforeUpdatedAtUnixMs
            beforeConversationID = page.nextBeforeConversationID
        } while beforeUpdatedAtUnixMs != nil && beforeConversationID != nil
        return records
    }

    private static func mapConversation(
        _ record: LocalAgentConversationRecord
    ) -> WorkspaceConversation? {
        guard let resource = record.resource else { return nil }
        return WorkspaceConversation(
            id: record.conversationID,
            title: record.title,
            projectID: resource.kind == .project ? resource.resourceID : nil,
            contactID: resource.kind == .contact ? resource.resourceID : mainContact.id,
            contactAgentID: mainContact.agentID,
            messageCount: 0,
            updatedAt: Date(timeIntervalSince1970: Double(record.updatedAtUnixMs) / 1_000),
            isArchived: false
        )
    }

    private func requireOwner() throws -> String {
        guard let ownerUserID else { throw NativeLocalAgentWorkspaceError.notConfigured }
        return ownerUserID
    }
}

public actor NativeLocalAgentProjectConversationService: ProjectConversationPreparing {
    private let client: NativeLocalAgentConversationClient
    private let workspace: NativeLocalAgentWorkspaceService
    private var ownerUserID: String?

    public init(
        host: any LocalAgentHostClientServicing,
        workspace: NativeLocalAgentWorkspaceService
    ) {
        client = NativeLocalAgentConversationClient(host: host)
        self.workspace = workspace
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
    }

    public func reset() {
        ownerUserID = nil
    }

    public func ensureConversation(
        project: WorkspaceProject,
        contact: WorkspaceContact
    ) async throws -> String {
        guard let ownerUserID else { throw NativeLocalAgentWorkspaceError.notConfigured }
        guard project.projectContext?.projectId == project.id else {
            throw ProjectRegistryError.invalidField("projectContext")
        }
        let binding = LocalAgentConversationResourceBinding(kind: .project, resourceID: project.id)
        if let existing = try await workspace.listAll(ownerUserID: ownerUserID).first(where: {
            $0.resource == binding
        }) {
            return existing.conversationID
        }
        do {
            return try await client.create(
                ownerUserID: ownerUserID,
                conversationID: "conversation_\(UUID().uuidString.lowercased())",
                title: project.name,
                resource: binding
            ).conversation.conversationID
        } catch {
            if let existing = try await workspace.listAll(ownerUserID: ownerUserID).first(where: {
                $0.resource == binding
            }) {
                return existing.conversationID
            }
            throw error
        }
    }
}

enum NativeLocalAgentWorkspaceError: LocalizedError {
    case notConfigured

    var errorDescription: String? {
        "Local Agent workspace is not configured for an authenticated account."
    }
}
