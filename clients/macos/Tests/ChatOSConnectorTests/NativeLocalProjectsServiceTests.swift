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
