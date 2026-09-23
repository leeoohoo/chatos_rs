@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalProjectsServiceTests: XCTestCase {
    private func context(rootWorkspaceFirst: Bool = false) throws -> Context {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("local-projects-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root.appendingPathComponent("repo"), withIntermediateDirectories: true)
        let stateURL = root.appendingPathComponent("connector.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "alice", username: "alice", displayName: nil, role: "user")
        state.deviceID = "device"
        let projectWorkspace = LocalConnectorWorkspace(
            id: "ws", alias: "workspace", absoluteRoot: root.path, fingerprint: "fingerprint"
        )
        let rootWorkspace = LocalConnectorWorkspace(
            id: "root-ws", alias: "root", absoluteRoot: "/", fingerprint: "root-fingerprint"
        )
        state.workspaces = rootWorkspaceFirst
            ? [rootWorkspace, projectWorkspace]
            : [projectWorkspace, rootWorkspace]
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let connector = NativeLocalConnectorService(
            configuration: .init(gatewayBaseURL: URL(string: "http://127.0.0.1:1")!, stateURL: stateURL),
            ticketProvider: NoNetworkTicketProvider())
        return Context(root: root, connector: connector,
                       service: NativeLocalProjectsService(connector: connector, databaseURL: root.appendingPathComponent("projects.db")))
    }

    func testCreatesOfflineWithoutGitContactOrActivationAndPersists() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let project = try await context.service.create(ownerUserID: "alice", draft: .init(name: "Local", workspaceID: "ws", relativeRoot: "repo"))
        XCTAssertEqual(project.rootPath, "local://connector/device/ws/repo")
        XCTAssertNil(project.latestConversationID)
        let registry = try await context.service.registry()
        let records = try await registry.list(ownerUserID: "alice")
        XCTAssertEqual(records.map(\.id), [project.id])
        XCTAssertFalse(FileManager.default.fileExists(atPath: context.root.appendingPathComponent("repo/.git").path))
    }

    func testCreatesAgentRequestedProjectInDefaultWorkspaceWithoutAcceptingAPath() async throws {
        let context = try context(rootWorkspaceFirst: true)
        defer { try? FileManager.default.removeItem(at: context.root) }

        let first = try await context.service.createInDefaultWorkspace(
            ownerUserID: "alice",
            name: "设计 / 系统",
            description: "由 Human 确认的 Agent 提案"
        )
        let second = try await context.service.createInDefaultWorkspace(
            ownerUserID: "alice",
            name: "设计 / 系统"
        )

        XCTAssertEqual(first.name, "设计 / 系统")
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: context.root.appendingPathComponent("Projects/设计---系统").path
        ))
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: context.root.appendingPathComponent("Projects/设计---系统-2").path
        ))
        XCTAssertNotEqual(first.id, second.id)
        let registry = try await context.service.registry()
        let records = try await registry.list(ownerUserID: "alice")
        XCTAssertEqual(Set(records.map(\.draft.workspaceID)), ["ws"])
        XCTAssertEqual(
            Set(records.map(\.draft.relativeRoot)),
            ["Projects/设计---系统", "Projects/设计---系统-2"]
        )
    }

    func testImportsExistingAbsoluteDirectoryWithoutChangingOrLinkingIt() async throws {
        let context = try context(rootWorkspaceFirst: true)
        defer { try? FileManager.default.removeItem(at: context.root) }
        let directory = context.root.appendingPathComponent("import-me", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        let sentinel = directory.appendingPathComponent("keep.txt")
        try Data("unchanged".utf8).write(to: sentinel)
        let before = try FileManager.default.contentsOfDirectory(atPath: directory.path)

        let prepared = try await context.service.prepareExistingDirectoryImport(
            ownerUserID: "alice",
            absolutePath: directory.path,
            name: nil,
            description: "Existing source"
        )
        XCTAssertEqual(prepared.absolutePath, directory.path)
        XCTAssertEqual(prepared.draft.name, "import-me")
        XCTAssertEqual(prepared.draft.workspaceID, "ws")
        XCTAssertEqual(prepared.draft.relativeRoot, "import-me")

        let project = try await context.service.createFromExistingDirectory(
            ownerUserID: "alice",
            draft: prepared.draft,
            absolutePath: prepared.absolutePath
        )
        XCTAssertEqual(project.displayRootPath, directory.path)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: directory.path), before)
        XCTAssertEqual(try String(contentsOf: sentinel, encoding: .utf8), "unchanged")
        XCTAssertFalse(try directory.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink ?? true)
    }

    func testExistingDirectoryImportRejectsSymbolicLinkPath() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let directory = context.root.appendingPathComponent("real-import", isDirectory: true)
        let link = context.root.appendingPathComponent("linked-import", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: directory)

        do {
            _ = try await context.service.prepareExistingDirectoryImport(
                ownerUserID: "alice",
                absolutePath: link.path,
                name: nil
            )
            XCTFail("A symbolic-link project path must not be imported")
        } catch {
            XCTAssertEqual(error as? ProjectRegistryError, .invalidField("absolutePath.symbolicLink"))
        }
    }

    func testSignedOutSuspensionPreservesPersistentProjectAccessState() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let project = try await context.service.create(
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
        let registry = try await context.service.registry()
        let record = try await registry.get(ownerUserID: "alice", id: project.id)
        XCTAssertEqual(record?.draft.relativeRoot, "repo")
    }

    func testDisconnectOnlyBlocksServerAccessAndPreservesAllLocalState() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let stateURL = context.root.appendingPathComponent("connector.json")
        let before = try NativeConnectorStateStore(stateURL: stateURL).load()

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
                _ = try await context.service.create(ownerUserID: owner, draft: .init(name: "bad", workspaceID: "ws", relativeRoot: path))
                XCTFail("Invalid directory/account accepted")
            } catch {}
        }
        let registry = try await context.service.registry()
        let records = try await registry.list(ownerUserID: "alice")
        XCTAssertTrue(records.isEmpty)
    }

    func testSymlinkEscapeIsRejectedBeforePersistence() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        try FileManager.default.createSymbolicLink(at: context.root.appendingPathComponent("escape"), withDestinationURL: context.root.deletingLastPathComponent())
        do {
            _ = try await context.service.create(ownerUserID: "alice", draft: .init(name: "bad", workspaceID: "ws", relativeRoot: "escape"))
            XCTFail("Escaped workspace")
        } catch {}
    }

    func testPluginContextUsesLocalNameAndBindingAndDeletionCannotFallback() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let project = try await context.service.create(ownerUserID: "alice", draft: .init(name: "Local", workspaceID: "ws", relativeRoot: "repo"))
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
        let registry = try await context.service.registry()
        let relativeRoot = context.root.appendingPathComponent("repo").path.dropFirst().description
        let record = try await registry.create(
            ownerUserID: "alice",
            draft: .init(name: "Rebound", workspaceID: "retired-workspace", relativeRoot: relativeRoot)
        )

        let resolved = try await context.connector.resolveProjectPath(
            "local://connector/device/retired-workspace/\(relativeRoot)"
        )
        XCTAssertEqual(resolved.workspace.id, "root-ws")
        XCTAssertEqual(resolved.absoluteURL.path, context.root.appendingPathComponent("repo").path)

        try await context.service.repairRootWorkspaceBindings(ownerUserID: "alice")
        let snapshot = try await context.service.projectContext(ownerUserID: "alice", projectID: record.id)
        XCTAssertEqual(snapshot.executionTarget.workspaceId, "root-ws")
        XCTAssertEqual(snapshot.executionTarget.relativeRoot, relativeRoot)
        XCTAssertEqual(snapshot.projectRevision, 2)
        let repaired = try await registry.get(ownerUserID: "alice", id: record.id)
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
            ticketProvider: NoNetworkTicketProvider()
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

    private struct Context { let root: URL; let connector: NativeLocalConnectorService; let service: NativeLocalProjectsService }
    private struct NoNetworkTicketProvider: LocalConnectorPairingTicketProviding {
        func issueLocalConnectorPairingTicket() async throws -> String { throw URLError(.notConnectedToInternet) }
    }
}
