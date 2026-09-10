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

    func testForeignDeviceNeverFallsBackToDisplayPathAndUnknownSchemesFail() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let candidates = await context.service.preview(ownerUserID: "alice", projects: [
            .init(id: "foreign", name: "Foreign", rootPath: "local://connector/another/ws/repo", displayRootPath: context.root.appendingPathComponent("repo").path, latestConversationID: nil),
            .init(id: "remote", name: "Remote", rootPath: "harness://repo", latestConversationID: nil),
        ])
        XCTAssertTrue(candidates.allSatisfy { $0.record == nil && $0.error != nil })
    }

    func testRepeatedFilePreviewAndImportPreserveIDAndReceipt() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let legacy = WorkspaceProject(id: "historical-id", name: "Imported", rootPath: "local://connector/device/ws/repo", latestConversationID: nil)
        let first = await context.service.preview(ownerUserID: "alice", projects: [legacy])
        let imported = try await context.service.importConfirmed(ownerUserID: "alice", sourceID: "export-1", candidates: first)
        let second = await context.service.preview(ownerUserID: "alice", projects: [legacy])
        let replay = try await context.service.importConfirmed(ownerUserID: "alice", sourceID: "export-1", candidates: second)
        XCTAssertEqual(imported, replay)
        XCTAssertEqual(imported.insertedIDs, [legacy.id])
    }

    func testChangedDirectoryBetweenPreviewAndConfirmationFails() async throws {
        let context = try context()
        defer { try? FileManager.default.removeItem(at: context.root) }
        let alias = context.root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: context.root.appendingPathComponent("repo"))
        let preview = await context.service.preview(ownerUserID: "alice", projects: [
            .init(id: "legacy", name: "Imported", rootPath: "local://connector/device/ws/alias", latestConversationID: nil),
        ])
        try FileManager.default.removeItem(at: alias)
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: context.root)
        do {
            _ = try await context.service.importConfirmed(ownerUserID: "alice", sourceID: "export", candidates: preview)
            XCTFail("Silently rebound a changed directory")
        } catch {}
        let registry = try await context.service.registry()
        let records = try await registry.list(ownerUserID: "alice")
        XCTAssertTrue(records.isEmpty)
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
        let record = LocalProjectRecord(
            id: "project",
            ownerUserID: "alice",
            draft: .init(name: "Rebound", workspaceID: "retired-workspace", relativeRoot: relativeRoot),
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
        _ = try await registry.importRecords(ownerUserID: "alice", sourceID: "stale-binding", records: [record])

        let resolved = try await context.connector.resolveProjectPath(
            "local://connector/device/retired-workspace/\(relativeRoot)"
        )
        XCTAssertEqual(resolved.workspace.id, "root-ws")
        XCTAssertEqual(resolved.absoluteURL.path, context.root.appendingPathComponent("repo").path)

        try await context.service.repairRootWorkspaceBindings(ownerUserID: "alice")
        let snapshot = try await context.service.projectContext(ownerUserID: "alice", projectID: "project")
        XCTAssertEqual(snapshot.executionTarget.workspaceId, "root-ws")
        XCTAssertEqual(snapshot.executionTarget.relativeRoot, relativeRoot)
        XCTAssertEqual(snapshot.projectRevision, 2)
        let repaired = try await registry.get(ownerUserID: "alice", id: "project")
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

    func testImportEnvelopeRejectsDifferentOwnerVersionAndDuplicateIDs() throws {
        for json in [
            #"{"schemaVersion":2,"ownerUserId":"alice","sourceId":"export","projects":[]}"#,
            #"{"schemaVersion":1,"ownerUserId":"bob","sourceId":"export","projects":[]}"#,
            #"{"schemaVersion":1,"ownerUserId":"alice","sourceId":"export","projects":[{"id":"a","name":"a","rootPath":"/repo"},{"id":"a","name":"a","rootPath":"/repo"}]}"#,
        ] {
            let document = try JSONDecoder().decode(LocalProjectImportDocument.self, from: Data(json.utf8))
            XCTAssertThrowsError(try document.validate(ownerUserID: "alice"))
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
