@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalProjectsServiceTests: XCTestCase {
    private func context() throws -> Context {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("local-projects-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root.appendingPathComponent("repo"), withIntermediateDirectories: true)
        let stateURL = root.appendingPathComponent("connector.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "alice", username: "alice", displayName: nil, role: "user")
        state.deviceID = "device"
        state.workspaces = [
            .init(id: "ws", alias: "workspace", absoluteRoot: root.path, fingerprint: "fingerprint"),
            .init(id: "root-ws", alias: "root", absoluteRoot: "/", fingerprint: "root-fingerprint"),
        ]
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let settingsClient = try NativeLocalAgentIPCClient(
            ownerUserID: "alice",
            transport: ProjectRunPreferencesTransport()
        )
        let connector = NativeLocalConnectorService(
            configuration: .init(gatewayBaseURL: URL(string: "http://127.0.0.1:1")!, stateURL: stateURL),
            ticketProvider: NoNetworkTicketProvider(),
            accountSession: ProjectRunPreferencesAccountSession(client: settingsClient),
            agentRuntimeSettings: AgentRuntimePreferencesTestProvider()
        )
        let client = ProjectClient(ownerUserID: "alice")
        return Context(
            root: root,
            connector: connector,
            service: NativeLocalProjectsService(
                connector: connector,
                clientProvider: { ownerUserID in
                    guard ownerUserID == "alice" else {
                        throw NativeLocalAgentAccountSessionError.accountMismatch
                    }
                    return client
                }
            ),
            client: client
        )
    }

    func testCreatesOfflineWithoutGitContactOrActivationAndPersists() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let project = try await context.service.createWorkspaceProject(ownerUserID: "alice", draft: .init(name: "Local", workspaceID: "ws", relativeRoot: "repo"))
        XCTAssertEqual(project.rootPath, "local://connector/device/ws/repo")
        XCTAssertNil(project.latestConversationID)
        let records = try await context.service.list(ownerUserID: "alice")
        XCTAssertEqual(records.map(\.id), [project.id])
        XCTAssertFalse(FileManager.default.fileExists(atPath: context.root.appendingPathComponent("repo/.git").path))
    }

    func testSignedOutSuspensionPreservesPersistentProjectAccessState() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let project = try await context.service.createWorkspaceProject(
            ownerUserID: "alice",
            draft: .init(name: "Local", workspaceID: "ws", relativeRoot: "repo")
        )

        await context.connector.suspendForSignedOut()

        let persisted = try NativeConnectorStateStore(
            stateURL: context.root.appendingPathComponent("connector.json")
        ).load()
        XCTAssertEqual(persisted.user?.id, "alice")
        XCTAssertEqual(persisted.deviceID, "device")
        XCTAssertEqual(persisted.workspaces.map(\.id), ["ws", "root-ws"])
        let deviceID = try await context.service.deviceID(ownerUserID: "alice")
        XCTAssertEqual(deviceID, "device")
        let record = try await context.service.get(ownerUserID: "alice", id: project.id)
        XCTAssertEqual(record?.draft.relativeRoot, "repo")
    }

    func testDisconnectOnlyBlocksServerAccessAndPreservesAllLocalState() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let stateURL = context.root.appendingPathComponent("connector.json")
        let before = try NativeConnectorStateStore(stateURL: stateURL).load()
        try await context.connector.activateClientStorage(ownerUserID: "alice")

        let status = try await context.connector.disconnect()

        let after = try NativeConnectorStateStore(stateURL: stateURL).load()
        XCTAssertEqual(after.user, before.user)
        XCTAssertEqual(after.deviceID, before.deviceID)
        XCTAssertEqual(after.deviceName, before.deviceName)
        XCTAssertEqual(after.workspaces, before.workspaces)
        XCTAssertEqual(after.gatewayConnectionEnabled, false)
        XCTAssertFalse(status.connectorRunning)
        let deviceID = try await context.service.deviceID(ownerUserID: "alice")
        XCTAssertEqual(deviceID, "device")
    }

    func testWrongAccountMissingDirectoryAndFileRootCannotCreateProjects() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        for (owner, path) in [("bob", "repo"), ("alice", "missing"), ("alice", "connector.json")] {
            do {
                _ = try await context.service.createWorkspaceProject(ownerUserID: owner, draft: .init(name: "bad", workspaceID: "ws", relativeRoot: path))
                XCTFail("Invalid directory/account accepted")
            } catch {}
        }
        let records = try await context.service.list(ownerUserID: "alice")
        XCTAssertTrue(records.isEmpty)
    }

    func testSymlinkEscapeIsRejectedBeforePersistence() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        try FileManager.default.createSymbolicLink(at: context.root.appendingPathComponent("escape"), withDestinationURL: context.root.deletingLastPathComponent())
        do {
            _ = try await context.service.createWorkspaceProject(ownerUserID: "alice", draft: .init(name: "bad", workspaceID: "ws", relativeRoot: "escape"))
            XCTFail("Escaped workspace")
        } catch {}
    }

    func testPluginContextUsesLocalNameAndBindingAndDeletionCannotFallback() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let project = try await context.service.createWorkspaceProject(ownerUserID: "alice", draft: .init(name: "Local", workspaceID: "ws", relativeRoot: "repo"))
        try await context.service.rename(ownerUserID: "alice", id: project.id, name: "Renamed", expectedRevision: 1)
        let plugin = try await context.service.pluginContext(ownerUserID: "alice", projectID: project.id)
        XCTAssertEqual(plugin.projectName, "Renamed")
        XCTAssertEqual(plugin.projectID, project.id)
        XCTAssertEqual(plugin.projectRoot, project.rootPath)
        try await context.service.remove(ownerUserID: "alice", id: project.id, expectedRevision: 2)
        do {
            _ = try await context.service.pluginContext(ownerUserID: "alice", projectID: project.id)
            XCTFail("Deleted project fell back to device scope")
        } catch { XCTAssertEqual(error as? ProjectRegistryError, .notFound) }
        XCTAssertTrue(FileManager.default.fileExists(atPath: context.root.appendingPathComponent("repo").path))
    }

    func testRetiredWorkspaceIDRebindsThroughCurrentAuthorizationAndUpdatesRegistry() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let relativeRoot = context.root.appendingPathComponent("repo").path.dropFirst().description
        let record = await context.client.seed(
            projectID: "project-rebound",
            draft: .init(
                name: "Rebound",
                description: "",
                workspaceID: "retired-workspace",
                relativeRoot: relativeRoot
            )
        )

        let resolved = try await context.connector.resolveProjectPath(
            "local://connector/device/retired-workspace/\(relativeRoot)"
        )
        XCTAssertEqual(resolved.workspace.id, "root-ws")
        XCTAssertEqual(resolved.absoluteURL.path, context.root.appendingPathComponent("repo").path)

        try await context.service.repairRootWorkspaceBindings(ownerUserID: "alice")
        let snapshot = try await context.service.projectContext(
            ownerUserID: "alice",
            projectID: record.projectID
        )
        XCTAssertEqual(snapshot.executionTarget.workspaceId, "root-ws")
        XCTAssertEqual(snapshot.executionTarget.relativeRoot, relativeRoot)
        XCTAssertEqual(snapshot.projectRevision, 2)
        let repaired = try await context.service.get(ownerUserID: "alice", id: record.projectID)
        XCTAssertEqual(repaired?.draft.workspaceID, "root-ws")
        XCTAssertEqual(repaired?.revision, 2)
    }

    func testRetiredWorkspaceIDDoesNotGuessThroughProjectSpecificWorkspaces() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("ambiguous-project-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root.appendingPathComponent("repo"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: root.appendingPathComponent("other/repo"), withIntermediateDirectories: true)
        let stateURL = root.appendingPathComponent("connector.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "alice", username: "alice", displayName: nil, role: "user")
        state.deviceID = "device"
        state.workspaces = [
            .init(id: "root", alias: "root", absoluteRoot: root.path, fingerprint: "root-fingerprint"),
            .init(id: "other", alias: "other", absoluteRoot: root.appendingPathComponent("other").path, fingerprint: "other-fingerprint"),
        ]
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let connector = NativeLocalConnectorService(
            configuration: .init(gatewayBaseURL: URL(string: "http://127.0.0.1:1")!, stateURL: stateURL),
            ticketProvider: NoNetworkTicketProvider(),
            accountSession: UnavailableLocalAgentAccountSession(),
            agentRuntimeSettings: AgentRuntimePreferencesTestProvider()
        )

        do {
            _ = try await connector.resolveProjectPath("local://connector/device/retired/repo")
            XCTFail("Project-specific workspace replacement was accepted")
        } catch let error as NativeConnectorError {
            guard case .workspaceUnavailable = error else {
                return XCTFail("Unexpected connector error: \(error)")
            }
        }
    }

    func testPluginLaunchRejectsAccountChangeBeforePreparingRuntime() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        do {
            _ = try await context.connector.launchPluginApplication(pluginID: "not-installed", componentKey: "main",
                context: .device, expectedOwnerUserID: "bob")
            XCTFail("Mismatched account accepted")
        } catch { XCTAssertTrue(error is CancellationError) }
    }

    private struct Context {
        let root: URL
        let connector: NativeLocalConnectorService
        let service: NativeLocalProjectsService
        let client: ProjectClient
    }
    private struct NoNetworkTicketProvider: LocalConnectorPairingTicketProviding {
        func issueLocalConnectorPairingTicket() async throws -> String { throw URLError(.notConnectedToInternet) }
    }
}

private actor ProjectClient: NativeLocalProjectIPCClient {
    let ownerUserID: String
    private var records: [String: LocalAgentProjectSnapshot] = [:]

    init(ownerUserID: String) {
        self.ownerUserID = ownerUserID
    }

    func project(id: String) throws -> LocalAgentProjectSnapshot {
        guard let record = records[id] else { throw notFound() }
        return record
    }

    func projects(includeInactive: Bool) -> [LocalAgentProjectSnapshot] {
        records.values
            .filter { includeInactive || $0.status == .active }
            .sorted {
                $0.draft.name == $1.draft.name
                    ? $0.projectID < $1.projectID
                    : $0.draft.name < $1.draft.name
            }
    }

    func createProject(
        projectID: String,
        draft: LocalAgentProjectDraft
    ) throws -> LocalAgentProjectSnapshot {
        guard records[projectID] == nil else { throw conflict() }
        let record = snapshot(projectID: projectID, draft: draft, revision: 1, status: .active)
        records[projectID] = record
        return record
    }

    func updateProject(
        projectID: String,
        expectedRevision: UInt64,
        draft: LocalAgentProjectDraft,
        status: LocalAgentProjectStatus
    ) throws -> LocalAgentProjectSnapshot {
        guard let previous = records[projectID] else { throw notFound() }
        guard previous.status != .removed else { throw removed() }
        guard previous.revision == expectedRevision else { throw conflict() }
        let record = snapshot(
            projectID: projectID,
            draft: draft,
            revision: expectedRevision + 1,
            status: status
        )
        records[projectID] = record
        return record
    }

    func seed(
        projectID: String,
        draft: LocalAgentProjectDraft
    ) -> LocalAgentProjectSnapshot {
        let record = snapshot(projectID: projectID, draft: draft, revision: 1, status: .active)
        records[projectID] = record
        return record
    }

    private func snapshot(
        projectID: String,
        draft: LocalAgentProjectDraft,
        revision: UInt64,
        status: LocalAgentProjectStatus
    ) -> LocalAgentProjectSnapshot {
        .init(
            projectID: projectID,
            ownerUserID: ownerUserID,
            draft: draft,
            revision: revision,
            status: status,
            createdAt: "2026-09-13T01:00:00Z",
            updatedAt: "2026-09-13T01:00:00Z"
        )
    }

    private func notFound() -> NativeLocalAgentIPCError {
        .rejected(.init(code: "project_not_found", message: "missing", retryable: false))
    }

    private func conflict() -> NativeLocalAgentIPCError {
        .rejected(.init(code: "project_revision_conflict", message: "conflict", retryable: false))
    }

    private func removed() -> NativeLocalAgentIPCError {
        .rejected(.init(
            code: "project_invalid",
            message: "removed projects cannot be updated",
            retryable: false
        ))
    }
}
