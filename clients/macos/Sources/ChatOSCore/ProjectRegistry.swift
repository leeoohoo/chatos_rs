import Foundation

public enum ProjectRegistryError: Error, Equatable, LocalizedError {
    case invalidField(String)
    case notFound
    case revisionConflict
    case removed
    case storage(String)

    public var errorDescription: String? {
        switch self {
        case let .invalidField(field): "无效的项目字段：\(field)"
        case .notFound: "本地项目不存在。"
        case .revisionConflict: "项目已被修改，请刷新后重试。"
        case .removed: "项目已删除，不能更新或自动恢复。"
        case let .storage(message): "项目注册表不可用：\(message)"
        }
    }
}

public enum LocalProjectStatus: String, Codable, Sendable {
    case active, archived, removed
}

/// Only host-owned metadata. Git facts, conversations and plugin business data are not stored here.
public struct LocalProjectDraft: Codable, Sendable, Equatable {
    public let name: String
    public let description: String
    public let rootPath: String

    public init(name: String, description: String = "", rootPath: String) {
        self.name = name
        self.description = description
        self.rootPath = rootPath
    }

    public func validate() throws {
        try ProjectRegistryValidation.identifier(name, field: "name")
        try ProjectRegistryValidation.absoluteRootPath(rootPath)
        guard !description.contains("\0") else { throw ProjectRegistryError.invalidField("description") }
    }
}

public struct LocalProjectRecord: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let draft: LocalProjectDraft
    public let revision: Int64
    public let status: LocalProjectStatus
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public init(
        id: String, ownerUserID: String, draft: LocalProjectDraft, revision: Int64 = 1,
        status: LocalProjectStatus = .active, createdAtUnixMs: Int64, updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public func validate() throws {
        try ProjectRegistryValidation.identifier(id, field: "id")
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        try draft.validate()
        guard revision > 0, revision < Int64.max else { throw ProjectRegistryError.invalidField("revision") }
        guard createdAtUnixMs >= 0, updatedAtUnixMs >= createdAtUnixMs else {
            throw ProjectRegistryError.invalidField("timestamps")
        }
    }
}

public protocol ProjectRegistry: Sendable {
    func list(ownerUserID: String, includeInactive: Bool) async throws -> [LocalProjectRecord]
    func get(ownerUserID: String, id: String) async throws -> LocalProjectRecord?
    func create(ownerUserID: String, draft: LocalProjectDraft) async throws -> LocalProjectRecord
    func update(
        ownerUserID: String, id: String, expectedRevision: Int64,
        draft: LocalProjectDraft, status: LocalProjectStatus
    ) async throws -> LocalProjectRecord
}

public enum ProjectRegistryValidation {
    public static func routeIdentifier(_ value: String, field: String) throws {
        try identifier(value, field: field)
        guard !value.contains("/"), !value.contains("\\"), value != ".", value != ".." else {
            throw ProjectRegistryError.invalidField(field)
        }
    }

    public static func identifier(_ value: String, field: String) throws {
        guard !value.isEmpty, value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              value.rangeOfCharacter(from: .controlCharacters) == nil else {
            throw ProjectRegistryError.invalidField(field)
        }
    }

    public static func absoluteRootPath(_ value: String) throws {
        guard !value.isEmpty,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              value.hasPrefix("/"),
              value.rangeOfCharacter(from: .controlCharacters) == nil,
              URL(fileURLWithPath: value).standardizedFileURL.path == value else {
            throw ProjectRegistryError.invalidField("rootPath")
        }
    }
}

/// An untrusted request DTO until the server authenticates the device/workspace and freezes it.
/// Intentionally excludes owner identity and absolute paths. Changing a registry record cannot
/// mutate a previously-created value. All clients use these explicit camelCase wire keys.
public struct ProjectContextSnapshot: Codable, Sendable, Equatable {
    public struct ExecutionTarget: Codable, Sendable, Equatable {
        public let rootPath: String
    }

    public let schemaVersion: Int
    public let projectId: String
    public let projectName: String
    public let projectRevision: Int64
    public let executionTarget: ExecutionTarget

    public init(record: LocalProjectRecord) throws {
        try record.validate()
        guard record.status == .active else { throw ProjectRegistryError.invalidField("status") }
        schemaVersion = 1
        projectId = record.id
        projectName = record.draft.name
        projectRevision = record.revision
        executionTarget = ExecutionTarget(rootPath: record.draft.rootPath)
    }
}
