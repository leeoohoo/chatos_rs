import ChatOSCore
import Foundation

/// Host entrypoint for local CRUD and explicit migration. Never creates/updates a remote project.
public actor NativeLocalProjectsService {
    private let connector: NativeLocalConnectorService
    private let databaseURL: URL
    private var store: SQLiteProjectRegistry?

    public init(connector: NativeLocalConnectorService, databaseURL: URL) {
        self.connector = connector
        self.databaseURL = databaseURL
    }

    public func registry() throws -> SQLiteProjectRegistry {
        if let store { return store }
        let opened = try SQLiteProjectRegistry(databaseURL: databaseURL)
        store = opened
        return opened
    }

    public func deviceID(ownerUserID: String) async throws -> String? {
        try await connector.localProjectDeviceID(ownerUserID: ownerUserID)
    }

    public func pluginContext(ownerUserID: String, projectID: String) async throws -> LocalConnectorPluginApplicationContext {
        let registry = try registry()
        let record = try await activeRecordWithCurrentBinding(
            registry: registry,
            ownerUserID: ownerUserID,
            projectID: projectID
        )
        guard let deviceID = try await connector.localProjectDeviceID(ownerUserID: ownerUserID) else {
            throw NativeConnectorError.workspaceUnavailable
        }
        guard try await registry.get(ownerUserID: ownerUserID, id: projectID) == record else {
            throw ProjectRegistryError.revisionConflict
        }
        return .init(projectID: record.id, projectName: record.draft.name, projectRoot: try record.localRootURI(deviceID: deviceID))
    }

    public func projectContext(ownerUserID: String, projectID: String) async throws -> ProjectContextSnapshot {
        let registry = try registry()
        let record = try await activeRecordWithCurrentBinding(
            registry: registry,
            ownerUserID: ownerUserID,
            projectID: projectID
        )
        guard let deviceID = try await connector.localProjectDeviceID(ownerUserID: ownerUserID) else {
            throw NativeConnectorError.workspaceUnavailable
        }
        guard try await registry.get(ownerUserID: ownerUserID, id: projectID) == record else {
            throw ProjectRegistryError.revisionConflict
        }
        return try ProjectContextSnapshot(record: record, deviceID: deviceID)
    }

    /// Re-pairing creates a new device/workspace grant while the local project registry
    /// intentionally survives. Repair only projects that can be proven to use the current
    /// root-filesystem grant; project-specific workspace changes still require explicit input.
    public func repairRootWorkspaceBindings(ownerUserID: String) async throws {
        let registry = try registry()
        let records = try await registry.list(ownerUserID: ownerUserID, includeInactive: false)
        for record in records {
            do {
                _ = try await activeRecordWithCurrentBinding(
                    registry: registry,
                    ownerUserID: ownerUserID,
                    projectID: record.id
                )
            } catch NativeConnectorError.workspaceUnavailable {
                continue
            } catch ProjectRegistryError.revisionConflict {
                continue
            }
        }
    }

    public func create(ownerUserID: String, draft: LocalProjectDraft) async throws -> WorkspaceProject {
        let binding = try await connector.validateLocalProjectDirectory(ownerUserID: ownerUserID, draft: draft)
        try Task.checkCancellation()
        let record = try await registry().create(ownerUserID: ownerUserID, draft: draft)
        // Do not turn a successful write into an apparent failure if pairing changes after commit.
        let deviceID = try? await connector.localProjectDeviceID(ownerUserID: ownerUserID)
        return WorkspaceProject(id: record.id, name: draft.name,
                                rootPath: try record.localRootURI(deviceID: deviceID),
                                displayRootPath: binding.absolutePath, latestConversationID: nil,
                                projectContext: try deviceID.map { try ProjectContextSnapshot(record: record, deviceID: $0) })
    }

    public func preview(ownerUserID: String, projects: [WorkspaceProject]) async -> [LocalProjectImportCandidate] {
        var candidates: [LocalProjectImportCandidate] = []
        for project in projects {
            do {
                try Task.checkCancellation()
                let binding = try await connector.resolveLegacyProjectDirectory(ownerUserID: ownerUserID, project: project)
                let record = LocalProjectRecord(
                    id: project.id, ownerUserID: ownerUserID,
                    draft: .init(name: project.name, workspaceID: binding.workspaceID, relativeRoot: binding.relativeRoot),
                    createdAtUnixMs: 0, updatedAtUnixMs: 0
                )
                try record.validate()
                candidates.append(.init(project: project, record: record, binding: binding, error: nil))
            } catch {
                candidates.append(.init(project: project, record: nil, binding: nil, error: error.localizedDescription))
            }
        }
        return candidates
    }

    public func importConfirmed(
        ownerUserID: String, sourceID: String, candidates: [LocalProjectImportCandidate]
    ) async throws -> ProjectRegistryImportResult {
        // Import is an explicit data operation, never an authority-mode switch.
        try Task.checkCancellation()
        var records: [LocalProjectRecord] = []
        for candidate in candidates {
            guard let record = candidate.record, let binding = candidate.binding,
                  record.ownerUserID == ownerUserID else { throw ProjectRegistryError.invalidField("import selection") }
            let current = try await connector.resolveLegacyProjectDirectory(ownerUserID: ownerUserID, project: candidate.project)
            guard current == binding,
                  record.id == candidate.project.id,
                  record.draft.workspaceID == current.workspaceID,
                  record.draft.relativeRoot == current.relativeRoot else {
                throw ProjectRegistryError.storage("项目目录或工作区已变化，请重新预览后确认。")
            }
            records.append(record)
        }
        try Task.checkCancellation()
        // Records and receipt commit together. There is no legacy authority or activation flag.
        return try await registry().importRecords(ownerUserID: ownerUserID, sourceID: sourceID, records: records)
    }

    public func rename(ownerUserID: String, id: String, name: String, expectedRevision: Int64) async throws {
        guard let old = try await registry().get(ownerUserID: ownerUserID, id: id) else { throw ProjectRegistryError.notFound }
        let draft = LocalProjectDraft(name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                                      description: old.draft.description, workspaceID: old.draft.workspaceID,
                                      relativeRoot: old.draft.relativeRoot)
        _ = try await registry().update(ownerUserID: ownerUserID, id: id, expectedRevision: expectedRevision,
                                       draft: draft, status: old.status)
    }

    public func remove(ownerUserID: String, id: String, expectedRevision: Int64) async throws {
        guard let old = try await registry().get(ownerUserID: ownerUserID, id: id) else { throw ProjectRegistryError.notFound }
        _ = try await registry().update(ownerUserID: ownerUserID, id: id, expectedRevision: expectedRevision,
                                       draft: old.draft, status: .removed)
    }

    private func activeRecordWithCurrentBinding(
        registry: SQLiteProjectRegistry,
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalProjectRecord {
        guard let record = try await registry.get(ownerUserID: ownerUserID, id: projectID),
              record.status == .active else {
            throw ProjectRegistryError.notFound
        }
        do {
            _ = try await connector.validateLocalProjectDirectory(
                ownerUserID: ownerUserID,
                draft: record.draft
            )
            return record
        } catch NativeConnectorError.workspaceUnavailable {
            _ = try await connector.localProjectDeviceID(ownerUserID: ownerUserID)
            let resolved = try await connector.resolveReauthorizedProjectPath(
                relativePath: record.draft.relativeRoot
            )
            let draft = LocalProjectDraft(
                name: record.draft.name,
                description: record.draft.description,
                workspaceID: resolved.workspace.id,
                relativeRoot: resolved.relativePath == "." ? "" : resolved.relativePath
            )
            return try await registry.update(
                ownerUserID: ownerUserID,
                id: projectID,
                expectedRevision: record.revision,
                draft: draft,
                status: record.status
            )
        }
    }
}

public struct AccountLocalProjectCreator: LocalProjectCreating {
    private let ownerUserID: String
    private let service: NativeLocalProjectsService

    public init(ownerUserID: String, service: NativeLocalProjectsService) {
        self.ownerUserID = ownerUserID
        self.service = service
    }

    public func createProject(_ draft: LocalProjectDraft) async throws -> WorkspaceProject {
        try await service.create(ownerUserID: ownerUserID, draft: draft)
    }
}

extension NativeLocalConnectorService {
    public func localProjectDeviceID(ownerUserID: String) throws -> String? {
        guard state.user?.id == ownerUserID else { throw ProjectRegistryError.storage("本机工作区不属于当前账户，请重新配对。") }
        if let deviceID = state.deviceID { try ProjectRegistryValidation.routeIdentifier(deviceID, field: "deviceID") }
        return state.deviceID
    }

    func validateLocalProjectDirectory(ownerUserID: String, draft: LocalProjectDraft) throws -> ProjectDirectoryBinding {
        try draft.validate()
        _ = try localProjectDeviceID(ownerUserID: ownerUserID)
        guard let workspace = state.workspaces.first(where: { $0.id == draft.workspaceID }) else {
            throw NativeConnectorError.workspaceUnavailable
        }
        let url = try NativeWorkspaceFilesystem(workspace: workspace).resolveExistingURL(draft.relativeRoot.isEmpty ? "." : draft.relativeRoot)
        guard try url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true else {
            throw NativeWorkspaceRelayError.notDirectory
        }
        return .init(workspaceID: workspace.id, relativeRoot: draft.relativeRoot,
                     absolutePath: url.path, workspaceFingerprint: workspace.fingerprint)
    }

    func resolveLegacyProjectDirectory(ownerUserID: String, project: WorkspaceProject) throws -> ProjectDirectoryBinding {
        _ = try localProjectDeviceID(ownerUserID: ownerUserID)
        guard let path = project.rootPath, !path.isEmpty else { throw NativeConnectorError.workspaceUnavailable }
        guard path == path.trimmingCharacters(in: .whitespacesAndNewlines) else {
            throw ProjectRegistryError.invalidField("legacy project root")
        }
        // Foreign-device local:// URIs must fail; never fall back to displayRootPath on this machine.
        if let components = URLComponents(string: path), components.scheme != nil {
            guard components.scheme == "local", components.host == "connector",
                  components.query == nil, components.fragment == nil else {
                throw ProjectRegistryError.invalidField("legacy project root")
            }
        } else if !path.hasPrefix("/") {
            throw ProjectRegistryError.invalidField("legacy project root")
        }
        let resolved = try resolveProjectPath(path)
        let relative = resolved.relativePath == "." ? "" : resolved.relativePath
        return try validateLocalProjectDirectory(ownerUserID: ownerUserID,
            draft: .init(name: project.name, workspaceID: resolved.workspace.id, relativeRoot: relative))
    }
}
