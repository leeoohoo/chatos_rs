import Foundation

public protocol LocalProjectCreating: Sendable {
    func createProject(_ draft: LocalProjectDraft) async throws -> WorkspaceProject
}

/// One-shot file import, not a live compatibility API. Unknown historical timestamps use zero.
public struct LocalProjectImportDocument: Codable, Sendable {
    public struct Entry: Codable, Sendable {
        public let id: String
        public let name: String
        public let rootPath: String
    }
    public let schemaVersion: Int
    public let ownerUserId: String
    public let sourceId: String
    public let projects: [Entry]

    public func validate(ownerUserID: String) throws {
        guard schemaVersion == 1, projects.count <= 1_000 else { throw ProjectRegistryError.invalidField("import schema/size") }
        guard ownerUserId == ownerUserID else { throw ProjectRegistryError.invalidField("ownerUserId") }
        try ProjectRegistryValidation.identifier(sourceId, field: "sourceId")
        guard Set(projects.map(\.id)).count == projects.count else { throw ProjectRegistryError.invalidField("duplicate id") }
        for entry in projects {
            try ProjectRegistryValidation.identifier(entry.id, field: "id")
            try ProjectRegistryValidation.identifier(entry.name, field: "name")
        }
    }
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

public struct LocalProjectImportCandidate: Identifiable, Sendable {
    public var id: String { project.id }
    public let project: WorkspaceProject
    public let record: LocalProjectRecord?
    public let binding: ProjectDirectoryBinding?
    public let error: String?

    public init(project: WorkspaceProject, record: LocalProjectRecord?, binding: ProjectDirectoryBinding?, error: String?) {
        self.project = project
        self.record = record
        self.binding = binding
        self.error = error
    }
}
