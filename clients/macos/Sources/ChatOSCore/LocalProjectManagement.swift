import Foundation

public protocol LocalProjectCreating: Sendable {
    func createProject(_ draft: LocalProjectDraft) async throws -> WorkspaceProject
}

public struct ProjectDirectoryBinding: Sendable, Equatable {
    public let absolutePath: String

    public init(absolutePath: String) {
        self.absolutePath = absolutePath
    }
}
