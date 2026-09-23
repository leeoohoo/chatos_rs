import Foundation

public protocol LocalProjectCreating: Sendable {
    func createProject(_ draft: LocalProjectDraft) async throws -> WorkspaceProject
}

/// Local-only preview evidence. Never sent to a server or used as a permission grant.
public struct ProjectDirectoryBinding: Sendable, Equatable {
    public let workspaceID: String
    public let relativeRoot: String
    public let absolutePath: String
    public let workspaceFingerprint: String

    public init(workspaceID: String, relativeRoot: String, absolutePath: String, workspaceFingerprint: String) {
        self.workspaceID = workspaceID
        self.relativeRoot = relativeRoot
        self.absolutePath = absolutePath
        self.workspaceFingerprint = workspaceFingerprint
    }
}
