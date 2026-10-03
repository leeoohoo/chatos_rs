@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeProjectRunServiceTests: XCTestCase {
    func testPublishesStartAndTerminationChangesWithoutPolling() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("project-run-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("import time\ntime.sleep(1)\n".utf8)
            .write(to: root.appendingPathComponent("main.py"))

        let stateURL = root.appendingPathComponent("connector-state.json")
        var connectorState = NativeConnectorPersistentState.empty
        connectorState.deviceID = "device"
        connectorState.workspaces = [
            .init(
                id: "workspace",
                alias: "test",
                absoluteRoot: root.path,
                fingerprint: "test"
            ),
        ]
        try NativeConnectorStateStore(stateURL: stateURL).save(connectorState)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: ProjectRunTicketProvider()
        )
        let service = NativeProjectRunService(
            connector: connector,
            preferencesURL: root.appendingPathComponent("run-preferences.json"),
            maximumRetainedExitedInstances: 2
        )
        let projectID = "project"
        await service.updateProjects([.init(
            id: projectID,
            name: "Project",
            rootPath: "local://connector/device/workspace",
            latestConversationID: nil
        )])
        let catalog = try await service.fetchCatalog(projectID: projectID)
        let target = try XCTUnwrap(catalog.targets.first)

        let startChanges = await service.changes(projectID: projectID)
        try await service.start(projectID: projectID, targetID: target.id)
        let receivedStart = await receivesUpdate(from: startChanges, timeout: .seconds(1))
        XCTAssertTrue(receivedStart)

        let terminationChanges = await service.changes(projectID: projectID)
        let receivedTermination = await receivesUpdate(
            from: terminationChanges,
            timeout: .seconds(3)
        )
        XCTAssertTrue(receivedTermination)

        let state = try await service.fetchState(projectID: projectID)
        XCTAssertEqual(state.instances.count, 1)
        XCTAssertEqual(state.instances.first?.status, "exited")
        XCTAssertFalse(state.isRunning)

        for _ in 0..<2 {
            let startChanges = await service.changes(projectID: projectID)
            try await service.start(projectID: projectID, targetID: target.id)
            let receivedStart = await receivesUpdate(from: startChanges, timeout: .seconds(1))
            XCTAssertTrue(receivedStart)

            let terminationChanges = await service.changes(projectID: projectID)
            let receivedTermination = await receivesUpdate(
                from: terminationChanges,
                timeout: .seconds(3)
            )
            XCTAssertTrue(receivedTermination)
        }
        let boundedState = try await service.fetchState(projectID: projectID)
        XCTAssertEqual(boundedState.instances.count, 2)
    }

    private func receivesUpdate(
        from stream: AsyncStream<Void>,
        timeout: Duration
    ) async -> Bool {
        await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                for await _ in stream { return true }
                return false
            }
            group.addTask {
                try? await Task.sleep(for: timeout)
                return false
            }
            let result = await group.next() ?? false
            group.cancelAll()
            return result
        }
    }

}

private struct ProjectRunTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
