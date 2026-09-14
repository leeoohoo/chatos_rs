// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

protocol NativeLocalProjectIPCClient: Sendable {
    func project(id: String) async throws -> LocalAgentProjectSnapshot
    func projects(includeInactive: Bool) async throws -> [LocalAgentProjectSnapshot]
    func createProject(
        projectID: String,
        draft: LocalAgentProjectDraft
    ) async throws -> LocalAgentProjectSnapshot
    func updateProject(
        projectID: String,
        expectedRevision: UInt64,
        draft: LocalAgentProjectDraft,
        status: LocalAgentProjectStatus
    ) async throws -> LocalAgentProjectSnapshot
}

extension NativeLocalAgentIPCClient: NativeLocalProjectIPCClient {}

/// Account-scoped project authority backed exclusively by the selected Rust
/// Client Storage provider. Native code validates filesystem grants, but it
/// never opens a project database or keeps a second project record.
public actor NativeLocalProjectsService: ProjectRegistry {
    typealias ClientProvider = @Sendable (String) async throws -> any NativeLocalProjectIPCClient

    private let clientProvider: ClientProvider

    public init(
        accountSession: any NativeLocalAgentAccountSessionAccess
    ) {
        self.clientProvider = { ownerUserID in
            try await accountSession.client(accountID: ownerUserID)
        }
    }

    init(clientProvider: @escaping ClientProvider) {
        self.clientProvider = clientProvider
    }

    public func list(
        ownerUserID: String,
        includeInactive: Bool = false
    ) async throws -> [LocalProjectRecord] {
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        do {
            let snapshots = try await clientProvider(ownerUserID).projects(
                includeInactive: includeInactive
            )
            return try snapshots.map { try localRecord($0, expectedOwnerUserID: ownerUserID) }
        } catch {
            throw projectError(error)
        }
    }

    public func get(ownerUserID: String, id: String) async throws -> LocalProjectRecord? {
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        try ProjectRegistryValidation.identifier(id, field: "id")
        do {
            return try localRecord(
                try await clientProvider(ownerUserID).project(id: id),
                expectedOwnerUserID: ownerUserID
            )
        } catch NativeLocalAgentIPCError.rejected(let rejection)
            where rejection.code == "project_not_found"
        {
            return nil
        } catch {
            throw projectError(error)
        }
    }

    public func create(
        ownerUserID: String,
        draft: LocalProjectDraft
    ) async throws -> LocalProjectRecord {
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        try draft.validate()
        let projectID = UUID().uuidString.lowercased()
        do {
            let snapshot = try await clientProvider(ownerUserID).createProject(
                projectID: projectID,
                draft: wireDraft(draft)
            )
            guard snapshot.projectID == projectID else {
                throw ProjectRegistryError.storage("Host 返回了不一致的项目身份")
            }
            return try localRecord(snapshot, expectedOwnerUserID: ownerUserID)
        } catch {
            throw projectError(error)
        }
    }

    public func update(
        ownerUserID: String,
        id: String,
        expectedRevision: Int64,
        draft: LocalProjectDraft,
        status: LocalProjectStatus
    ) async throws -> LocalProjectRecord {
        try ProjectRegistryValidation.identifier(ownerUserID, field: "ownerUserID")
        try ProjectRegistryValidation.identifier(id, field: "id")
        try draft.validate()
        guard let revision = UInt64(exactly: expectedRevision), revision > 0 else {
            throw ProjectRegistryError.invalidField("revision")
        }
        do {
            let snapshot = try await clientProvider(ownerUserID).updateProject(
                projectID: id,
                expectedRevision: revision,
                draft: wireDraft(draft),
                status: wireStatus(status)
            )
            guard snapshot.projectID == id else {
                throw ProjectRegistryError.storage("Host 返回了不一致的项目身份")
            }
            return try localRecord(snapshot, expectedOwnerUserID: ownerUserID)
        } catch {
            throw projectError(error)
        }
    }

    public func localAgentProjectRecord(
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalProjectRecord {
        guard let record = try await get(ownerUserID: ownerUserID, id: projectID),
              record.status == .active,
              record.ownerUserID == ownerUserID,
              record.id == projectID
        else {
            throw ProjectRegistryError.notFound
        }
        try record.validate()
        return record
    }

    public func pluginContext(
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalConnectorPluginApplicationContext {
        let record = try await activeRecord(
            ownerUserID: ownerUserID,
            projectID: projectID
        )
        guard try await get(ownerUserID: ownerUserID, id: projectID) == record else {
            throw ProjectRegistryError.revisionConflict
        }
        return .init(
            projectID: record.id,
            projectName: record.draft.name,
            projectRoot: record.draft.rootPath
        )
    }

    public func projectContext(
        ownerUserID: String,
        projectID: String
    ) async throws -> ProjectContextSnapshot {
        let record = try await activeRecord(
            ownerUserID: ownerUserID,
            projectID: projectID
        )
        guard try await get(ownerUserID: ownerUserID, id: projectID) == record else {
            throw ProjectRegistryError.revisionConflict
        }
        return try ProjectContextSnapshot(record: record)
    }

    public func createWorkspaceProject(
        ownerUserID: String,
        draft: LocalProjectDraft
    ) async throws -> WorkspaceProject {
        let binding = try validateLocalProjectDirectory(draft)
        try Task.checkCancellation()
        let record: LocalProjectRecord = try await create(ownerUserID: ownerUserID, draft: draft)
        return WorkspaceProject(
            id: record.id,
            name: draft.name,
            rootPath: record.draft.rootPath,
            displayRootPath: binding.absolutePath,
            latestConversationID: nil,
            projectContext: try ProjectContextSnapshot(record: record)
        )
    }

    public func rename(
        ownerUserID: String,
        id: String,
        name: String,
        expectedRevision: Int64
    ) async throws {
        guard let old = try await get(ownerUserID: ownerUserID, id: id) else {
            throw ProjectRegistryError.notFound
        }
        let draft = LocalProjectDraft(
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            description: old.draft.description,
            rootPath: old.draft.rootPath
        )
        _ = try await update(
            ownerUserID: ownerUserID,
            id: id,
            expectedRevision: expectedRevision,
            draft: draft,
            status: old.status
        )
    }

    public func remove(
        ownerUserID: String,
        id: String,
        expectedRevision: Int64
    ) async throws {
        guard let old = try await get(ownerUserID: ownerUserID, id: id) else {
            throw ProjectRegistryError.notFound
        }
        _ = try await update(
            ownerUserID: ownerUserID,
            id: id,
            expectedRevision: expectedRevision,
            draft: old.draft,
            status: .removed
        )
    }

    private func activeRecord(
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalProjectRecord {
        guard let record = try await get(ownerUserID: ownerUserID, id: projectID),
              record.status == .active
        else {
            throw ProjectRegistryError.notFound
        }
        _ = try validateLocalProjectDirectory(record.draft)
        return record
    }

    private func wireDraft(_ draft: LocalProjectDraft) -> LocalAgentProjectDraft {
        .init(
            name: draft.name,
            description: draft.description,
            rootPath: draft.rootPath
        )
    }

    private func wireStatus(_ status: LocalProjectStatus) -> LocalAgentProjectStatus {
        switch status {
        case .active: .active
        case .archived: .archived
        case .removed: .removed
        }
    }

    private func localRecord(
        _ snapshot: LocalAgentProjectSnapshot,
        expectedOwnerUserID: String
    ) throws -> LocalProjectRecord {
        guard snapshot.ownerUserID == expectedOwnerUserID,
              let revision = Int64(exactly: snapshot.revision),
              let createdAt = Self.unixMilliseconds(snapshot.createdAt),
              let updatedAt = Self.unixMilliseconds(snapshot.updatedAt)
        else {
            throw ProjectRegistryError.storage("Host 返回了无效的项目作用域或元数据")
        }
        let status: LocalProjectStatus = switch snapshot.status {
        case .active: .active
        case .archived: .archived
        case .removed: .removed
        }
        let record = LocalProjectRecord(
            id: snapshot.projectID,
            ownerUserID: snapshot.ownerUserID,
            draft: .init(
                name: snapshot.draft.name,
                description: snapshot.draft.description,
                rootPath: snapshot.draft.rootPath
            ),
            revision: revision,
            status: status,
            createdAtUnixMs: createdAt,
            updatedAtUnixMs: updatedAt
        )
        try record.validate()
        return record
    }

    private static func unixMilliseconds(_ value: String) -> Int64? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let date = fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
        guard let date else { return nil }
        return Int64((date.timeIntervalSince1970 * 1_000).rounded())
    }

    private func projectError(_ error: Error) -> Error {
        guard case let NativeLocalAgentIPCError.rejected(rejection) = error else {
            if error is ProjectRegistryError { return error }
            return ProjectRegistryError.storage(error.localizedDescription)
        }
        switch rejection.code {
        case "project_not_found": return ProjectRegistryError.notFound
        case "project_revision_conflict": return ProjectRegistryError.revisionConflict
        case "project_invalid" where rejection.message.contains("removed projects"):
            return ProjectRegistryError.removed
        case "project_invalid": return ProjectRegistryError.invalidField(rejection.message)
        default: return ProjectRegistryError.storage(rejection.message)
        }
    }
}

extension NativeLocalProjectsService: NativeLocalAgentProjectRecordLoading {}

public struct AccountLocalProjectCreator: LocalProjectCreating {
    private let ownerUserID: String
    private let service: NativeLocalProjectsService

    public init(ownerUserID: String, service: NativeLocalProjectsService) {
        self.ownerUserID = ownerUserID
        self.service = service
    }

    public func createProject(_ draft: LocalProjectDraft) async throws -> WorkspaceProject {
        try await service.createWorkspaceProject(ownerUserID: ownerUserID, draft: draft)
    }
}

private func validateLocalProjectDirectory(_ draft: LocalProjectDraft) throws -> ProjectDirectoryBinding {
    try draft.validate()
    let url = URL(fileURLWithPath: draft.rootPath).standardizedFileURL.resolvingSymlinksInPath()
    guard url.path == draft.rootPath,
          try url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true else {
        throw NativeWorkspaceRelayError.notDirectory
    }
    return .init(absolutePath: url.path)
}
