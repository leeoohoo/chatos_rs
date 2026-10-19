import Foundation

public protocol ProjectContextLoading: Sendable {
    func projectContext(
        ownerUserID: String,
        projectID: String
    ) async throws -> ProjectContextSnapshot
}

public protocol ProjectConversationPort: Sendable {
    func ensureConversation(project: WorkspaceProject, contact: WorkspaceContact) async throws -> String
}
