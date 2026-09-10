import Foundation

public protocol ProjectConversationPreparing: Sendable {
    func ensureConversation(project: WorkspaceProject, contact: WorkspaceContact) async throws -> String
}
