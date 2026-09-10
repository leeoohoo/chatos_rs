import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class ClientOwnedWorkspaceLoaderTests: XCTestCase {
    private func databaseURL() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("workspace-loader-\(UUID().uuidString)/projects.db")
    }

    func testOfflineRelationsDoNotHideLocalProjectsAndUnpairedProjectsRemainVisible() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let project = try await registry.create(ownerUserID: "alice", draft: .init(name: "Local", workspaceID: "ws"))
        let loader = try ClientOwnedWorkspaceLoader(registry: registry, remote: OfflineRelations(), ownerUserID: "alice")
        let local = try await loader.loadLocal(deviceID: nil)
        XCTAssertEqual(local.projects.map(\.id), [project.id])
        XCTAssertNil(local.projects.first?.rootPath)
        let refreshed = try await loader.refresh(deviceID: "device")
        XCTAssertEqual(refreshed.snapshot.projects.first?.rootPath, "local://connector/device/ws")
        XCTAssertNotNil(refreshed.remoteError)
        XCTAssertEqual(refreshed.snapshot.projects.map(\.id), [project.id])
    }

    func testRelationsCannotCreateOrRenameProjectsAndChooseLatestActiveConversation() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let project = try await registry.create(ownerUserID: "alice", draft: .init(name: "Local", workspaceID: "ws", relativeRoot: "目录/a%20b #x"))
        let remote = FixedRelations(snapshot: .init(contacts: [], conversations: [
            conversation("archived", project: project.id, time: 30, archived: true),
            conversation("new", project: project.id, time: 20),
            conversation("old", project: project.id, time: 10),
            conversation("orphan", project: "remote-only", time: 40),
        ]))
        let result = try await ClientOwnedWorkspaceLoader(registry: registry, remote: remote, ownerUserID: "alice").refresh(deviceID: "device")
        XCTAssertNil(result.remoteError)
        XCTAssertEqual(result.snapshot.projects.count, 1)
        XCTAssertEqual(result.snapshot.projects.first?.name, "Local")
        XCTAssertEqual(result.snapshot.projects.first?.latestConversationID, "new")
        let path = try XCTUnwrap(result.snapshot.projects.first?.rootPath)
        XCTAssertEqual(URLComponents(string: path)?.path, "/device/ws/目录/a%20b #x")
        XCTAssertEqual(result.snapshot.conversations.count, 4, "Historical/orphan conversations remain readable")
    }

    func testCancelledRefreshIsNotReportedAsOfflineSuccess() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let loader = try ClientOwnedWorkspaceLoader(registry: registry, remote: CancelledRelations(), ownerUserID: "alice")
        do {
            _ = try await loader.refresh(deviceID: nil)
            XCTFail("Cancellation swallowed")
        } catch { XCTAssertTrue(error is CancellationError) }
    }

    func testDeletionDuringRemoteRefreshCannotResurrectProject() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let registry = try SQLiteProjectRegistry(databaseURL: url)
        let project = try await registry.create(ownerUserID: "alice", draft: .init(name: "Local", workspaceID: "ws"))
        let remote = DeletingRelations(registry: registry, project: project)
        let loader = try ClientOwnedWorkspaceLoader(registry: registry, remote: remote, ownerUserID: "alice")
        let result = try await loader.refresh(deviceID: "device")
        XCTAssertTrue(result.snapshot.projects.isEmpty)
    }

    private func conversation(_ id: String, project: String, time: Double, archived: Bool = false) -> WorkspaceConversation {
        .init(id: id, title: id, projectID: project, contactID: nil, contactAgentID: nil, messageCount: 1,
              updatedAt: Date(timeIntervalSince1970: time), isArchived: archived)
    }
}

private struct OfflineRelations: WorkspaceRelationsRemoteServicing {
    func fetchWorkspaceRelations() async throws -> WorkspaceRelationsSnapshot { throw URLError(.notConnectedToInternet) }
}

private struct CancelledRelations: WorkspaceRelationsRemoteServicing {
    func fetchWorkspaceRelations() async throws -> WorkspaceRelationsSnapshot { throw CancellationError() }
}

private struct FixedRelations: WorkspaceRelationsRemoteServicing {
    let snapshot: WorkspaceRelationsSnapshot
    func fetchWorkspaceRelations() async throws -> WorkspaceRelationsSnapshot { snapshot }
}

private struct DeletingRelations: WorkspaceRelationsRemoteServicing {
    let registry: SQLiteProjectRegistry
    let project: LocalProjectRecord

    func fetchWorkspaceRelations() async throws -> WorkspaceRelationsSnapshot {
        _ = try await registry.update(ownerUserID: project.ownerUserID, id: project.id,
                                      expectedRevision: project.revision, draft: project.draft, status: .removed)
        return .init(contacts: [], conversations: [])
    }
}
