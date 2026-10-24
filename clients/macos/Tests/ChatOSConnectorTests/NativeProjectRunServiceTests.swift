@testable import ChatOSConnector
import ChatOSCore
import Darwin
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

    func testParentExitCleansDescendantsWithoutBlockingProjectRunActor() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("project-run-descendant-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        import pathlib
        import subprocess
        child = subprocess.Popen(["/bin/sleep", "10"])
        pathlib.Path("child.pid").write_text(str(child.pid))
        print("parent exiting", flush=True)
        """.utf8).write(to: root.appendingPathComponent("main.py"))
        let context = try await makeRunService(root: root)
        let childPIDURL = root.appendingPathComponent("child.pid")

        let startedAt = ContinuousClock.now
        try await context.service.start(
            projectID: context.projectID,
            targetID: context.targetID
        )
        for _ in 0..<200 where !FileManager.default.fileExists(atPath: childPIDURL.path) {
            try await Task.sleep(for: .milliseconds(10))
        }
        let childPID = try XCTUnwrap(pid_t(
            String(contentsOf: childPIDURL, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines)
        ))
        let didExit = try await waitForProjectRunExit(context: context, timeout: .seconds(3))
        XCTAssertTrue(didExit)
        XCTAssertLessThan(ContinuousClock.now - startedAt, .seconds(3))
        for _ in 0..<100 where projectRunProcessExists(childPID) {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertFalse(projectRunProcessExists(childPID))
    }

    func testStopTerminatesOnlyTheOwnedProjectRunProcessGroup() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("project-run-stop-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        import pathlib
        import subprocess
        import time
        child = subprocess.Popen(["/bin/sleep", "30"])
        pathlib.Path("child.pid").write_text(str(child.pid))
        time.sleep(30)
        """.utf8).write(to: root.appendingPathComponent("main.py"))
        let context = try await makeRunService(root: root)
        let childPIDURL = root.appendingPathComponent("child.pid")
        try await context.service.start(
            projectID: context.projectID,
            targetID: context.targetID
        )
        for _ in 0..<200 where !FileManager.default.fileExists(atPath: childPIDURL.path) {
            try await Task.sleep(for: .milliseconds(10))
        }
        let childPID = try XCTUnwrap(pid_t(
            String(contentsOf: childPIDURL, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines)
        ))
        let runningState = try await context.service.fetchState(projectID: context.projectID)
        let instanceID = try XCTUnwrap(runningState.instances.first?.id)

        let stoppedAt = ContinuousClock.now
        try await context.service.stop(instanceID: instanceID)
        let didExit = try await waitForProjectRunExit(context: context, timeout: .seconds(3))
        XCTAssertTrue(didExit)
        XCTAssertLessThan(ContinuousClock.now - stoppedAt, .seconds(3))
        for _ in 0..<100 where projectRunProcessExists(childPID) {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertFalse(projectRunProcessExists(childPID))
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

    private func makeRunService(root: URL) async throws -> ProjectRunTestContext {
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
            preferencesURL: root.appendingPathComponent("run-preferences.json")
        )
        let projectID = "project"
        await service.updateProjects([.init(
            id: projectID,
            name: "Project",
            rootPath: "local://connector/device/workspace",
            latestConversationID: nil
        )])
        let catalog = try await service.fetchCatalog(projectID: projectID)
        let targetID = try XCTUnwrap(catalog.targets.first?.id)
        return .init(service: service, projectID: projectID, targetID: targetID)
    }

    private func waitForProjectRunExit(
        context: ProjectRunTestContext,
        timeout: Duration
    ) async throws -> Bool {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        while clock.now < deadline {
            let state = try await context.service.fetchState(projectID: context.projectID)
            if state.instances.first?.isRunning == false { return true }
            try await Task.sleep(for: .milliseconds(20))
        }
        return false
    }

    private func projectRunProcessExists(_ processID: pid_t) -> Bool {
        Darwin.kill(processID, 0) == 0 || errno == EPERM
    }

}

private struct ProjectRunTestContext {
    var service: NativeProjectRunService
    var projectID: String
    var targetID: String
}

private struct ProjectRunTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
