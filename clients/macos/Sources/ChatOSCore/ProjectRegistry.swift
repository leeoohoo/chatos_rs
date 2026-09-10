import Foundation

public enum ProjectRegistryError: Error, Equatable, LocalizedError {
    case invalidField(String)
    case notFound
    case revisionConflict
    case removed
    case importSourceConflict
    case storage(String)

    public var errorDescription: String? {
        switch self {
        case let .invalidField(field): "无效的项目字段：\(field)"
        case .notFound: "本地项目不存在。"
        case .revisionConflict: "项目已被修改，请刷新后重试。"
        case .removed: "项目已删除，不能更新或自动恢复。"
        case .importSourceConflict: "迁移来源已使用，但导入内容不一致。"
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
    public let workspaceID: String
    public let relativeRoot: String

    public init(name: String, description: String = "", workspaceID: String, relativeRoot: String = "") {
        self.name = name
        self.description = description
        self.workspaceID = workspaceID
        self.relativeRoot = relativeRoot
    }

    public func validate() throws {
        try ProjectRegistryValidation.identifier(name, field: "name")
        try ProjectRegistryValidation.routeIdentifier(workspaceID, field: "workspaceID")
        try ProjectRegistryValidation.relativeRoot(relativeRoot)
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

public struct ProjectRegistryImportResult: Codable, Sendable, Equatable {
    public let insertedIDs: [String]
    /// Existing records, including tombstones, are never overwritten by an import.
    public let skippedIDs: [String]

    public init(insertedIDs: [String], skippedIDs: [String]) {
        self.insertedIDs = insertedIDs
        self.skippedIDs = skippedIDs
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
    /// Explicit, atomic, idempotent migration; not a remote refresh/merge operation.
    func importRecords(
        ownerUserID: String, sourceID: String, records: [LocalProjectRecord]
    ) async throws -> ProjectRegistryImportResult
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

    /// Canonical portable relative path. This is NOT a filesystem authorization check.
    /// The connector must still resolve symlinks and enforce the granted workspace at use time.
    public static func relativeRoot(_ value: String) throws {
        guard value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.contains("\\"), !value.contains(":"),
              value.rangeOfCharacter(from: .controlCharacters) == nil else {
            throw ProjectRegistryError.invalidField("relativeRoot")
        }
        if value.isEmpty { return }
        guard value.split(separator: "/", omittingEmptySubsequences: false).allSatisfy({
            !$0.isEmpty && $0 != "." && $0 != ".."
        }) else { throw ProjectRegistryError.invalidField("relativeRoot") }
    }
}

/// An untrusted request DTO until the server authenticates the device/workspace and freezes it.
/// Intentionally excludes owner identity and absolute paths. Changing a registry record cannot
/// mutate a previously-created value. All clients use these explicit camelCase wire keys.
public struct ProjectContextSnapshot: Codable, Sendable, Equatable {
    public struct ExecutionTarget: Codable, Sendable, Equatable {
        public let deviceId: String
        public let workspaceId: String
        public let relativeRoot: String
    }

    public let schemaVersion: Int
    public let projectId: String
    public let projectName: String
    public let projectRevision: Int64
    public let executionTarget: ExecutionTarget

    public init(record: LocalProjectRecord, deviceID: String) throws {
        try record.validate()
        try ProjectRegistryValidation.routeIdentifier(deviceID, field: "deviceID")
        guard record.status == .active else { throw ProjectRegistryError.invalidField("status") }
        schemaVersion = 1
        projectId = record.id
        projectName = record.draft.name
        projectRevision = record.revision
        executionTarget = ExecutionTarget(
            deviceId: deviceID, workspaceId: record.draft.workspaceID, relativeRoot: record.draft.relativeRoot
        )
    }
}
