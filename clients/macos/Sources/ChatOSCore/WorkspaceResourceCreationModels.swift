import Foundation

public struct LocalProjectCreationDraft: Sendable, Equatable {
    public var name: String
    public var deviceID: String
    public var workspaceID: String
    public var relativePath: String?

    public init(
        name: String,
        deviceID: String,
        workspaceID: String,
        relativePath: String?
    ) {
        self.name = name
        self.deviceID = deviceID
        self.workspaceID = workspaceID
        self.relativePath = relativePath
    }
}

public protocol WorkspaceResourceCreating: Sendable {
    func createLocalProject(_ draft: LocalProjectCreationDraft) async throws -> WorkspaceProject
    func bindContact(projectID: String, contactID: String) async throws
    func ensureConversation(
        project: WorkspaceProject,
        contact: WorkspaceContact
    ) async throws -> String
}
