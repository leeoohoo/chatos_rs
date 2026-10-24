@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import XCTest

final class NativeProjectGitServiceTests: XCTestCase {
    func testGitProcessRejectsOversizedOutputWithoutUnboundedAccumulation() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-git-output-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("large.txt")
        try Data(repeating: 0x78, count: 512 * 1_024).write(to: file)

        XCTAssertThrowsError(try NativeGitProcess.run(
            arguments: ["diff", "--no-index", "--no-color", "--", "/dev/null", file.path],
            directory: root,
            allowedExitCodes: [0, 1],
            maximumOutputBytes: 64 * 1_024
        )) { error in
            XCTAssertEqual(error as? NativeGitError, .outputTooLarge)
        }
    }

    func testGitProcessTimeoutTerminatesItsProcessGroup() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-git-timeout-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let startedAt = ContinuousClock.now

        XCTAssertThrowsError(try NativeGitProcess.run(
            arguments: ["-c", "alias.chatos-wait=!sleep 60", "chatos-wait"],
            directory: root,
            timeout: 0.05
        )) { error in
            XCTAssertEqual(error as? NativeGitError, .commandTimedOut)
        }
        XCTAssertLessThan(ContinuousClock.now - startedAt, .seconds(3))
    }

    func testGitBlockingWorkIsConcurrentAndCancellationStaysOperationScoped() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-git-isolation-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let parentPIDURL = root.appendingPathComponent("parent.pid")
        let childPIDURL = root.appendingPathComponent("child.pid")
        let longScript = root.appendingPathComponent("long.sh")
        let survivorScript = root.appendingPathComponent("survivor.sh")
        try """
        #!/bin/sh
        trap '' TERM
        printf '%s' "$$" > '\(parentPIDURL.path)'
        sleep 30 &
        printf '%s' "$!" > '\(childPIDURL.path)'
        wait
        """.write(to: longScript, atomically: true, encoding: .utf8)
        try """
        #!/bin/sh
        sleep 1
        printf isolated
        """.write(to: survivorScript, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: longScript.path
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: survivorScript.path
        )

        let longRunning = Task {
            try await NativeGitProcess.performBlockingWork { controller in
                try NativeGitProcess.run(
                    arguments: [
                        "-c", "alias.chatos-long=!\(longScript.path)", "chatos-long",
                    ],
                    directory: root,
                    timeout: 30,
                    controller: controller
                )
            }
        }
        for _ in 0..<200
            where !FileManager.default.fileExists(atPath: parentPIDURL.path)
                || !FileManager.default.fileExists(atPath: childPIDURL.path) {
            try await Task.sleep(for: .milliseconds(10))
        }
        let parentPIDText = try String(contentsOf: parentPIDURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let childPIDText = try String(contentsOf: childPIDURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let parentPID = try XCTUnwrap(pid_t(parentPIDText))
        let childPID = try XCTUnwrap(pid_t(childPIDText))

        let startedAt = ContinuousClock.now
        let survivor = Task {
            try await NativeGitProcess.performBlockingWork { controller in
                try NativeGitProcess.run(
                    arguments: [
                        "-c", "alias.chatos-survivor=!\(survivorScript.path)", "chatos-survivor",
                    ],
                    directory: root,
                    timeout: 5,
                    controller: controller
                )
            }
        }
        let canceller = Task {
            try await Task.sleep(for: .milliseconds(200))
            longRunning.cancel()
        }
        let survivorOutput = try await survivor.value
        XCTAssertEqual(survivorOutput.stdoutString, "isolated")
        XCTAssertLessThan(ContinuousClock.now - startedAt, .seconds(2))
        _ = try await canceller.value
        do {
            _ = try await longRunning.value
            XCTFail("expected the owned Git process group to be cancelled")
        } catch is CancellationError {
            // Expected: cancellation is scoped to this operation's process group.
        }
        for _ in 0..<100 where gitFixtureProcessExists(parentPID) || gitFixtureProcessExists(childPID) {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertFalse(gitFixtureProcessExists(parentPID))
        XCTAssertFalse(gitFixtureProcessExists(childPID))
    }

    func testLoadsChangesStagesCommitsAndBuildsDiff() async throws {
        let context = try makeContext()
        defer { try? FileManager.default.removeItem(at: context.root) }
        try await context.service.initializeRepository(projectRoot: context.logicalRoot)
        try configureIdentity(in: context.root)

        let file = context.root.appendingPathComponent("README.md")
        try Data("# First\n".utf8).write(to: file)

        var snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertTrue(snapshot.isRepository)
        XCTAssertEqual(snapshot.currentBranch, "main")
        let initialChange = try XCTUnwrap(snapshot.changes.first(where: { $0.path == "README.md" }))
        XCTAssertEqual(initialChange.kind, .untracked)
        XCTAssertEqual(initialChange.hasWorkingTreeChanges, true)

        try await context.service.stage(projectRoot: context.logicalRoot, paths: ["README.md"])
        snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertEqual(
            snapshot.changes.first(where: { $0.path == "README.md" })?.hasStagedChanges,
            true
        )

        try await context.service.commit(projectRoot: context.logicalRoot, message: "docs: add readme")
        snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertNil(snapshot.changes.first(where: { $0.path == "README.md" }))
        XCTAssertEqual(snapshot.commits.first?.subject, "docs: add readme")

        try Data("# First\n\nSecond line\n".utf8).write(to: file)
        snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        let change = try XCTUnwrap(snapshot.changes.first(where: { $0.path == "README.md" }))
        let diff = try await context.service.diff(
            projectRoot: context.logicalRoot,
            change: change,
            staged: false
        )
        XCTAssertTrue(diff.content.contains("+Second line"))

        try await context.service.saveRemote(
            projectRoot: context.logicalRoot,
            originalName: nil,
            name: "origin",
            url: "https://example.com/team/project.git"
        )
        snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertEqual(
            snapshot.remotes,
            [.init(name: "origin", url: "https://example.com/team/project.git")]
        )
        try await context.service.saveRemote(
            projectRoot: context.logicalRoot,
            originalName: "origin",
            name: "upstream",
            url: "git@example.com:team/project.git"
        )
        snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertEqual(snapshot.remotes.first?.name, "upstream")
        try await context.service.removeRemote(
            projectRoot: context.logicalRoot,
            name: "upstream"
        )
        snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertTrue(snapshot.remotes.isEmpty)
    }

    func testCreatesSwitchesAndMergesBranches() async throws {
        let context = try makeContext()
        defer { try? FileManager.default.removeItem(at: context.root) }
        try await context.service.initializeRepository(projectRoot: context.logicalRoot)
        try configureIdentity(in: context.root)

        let file = context.root.appendingPathComponent("value.txt")
        try Data("main\n".utf8).write(to: file)
        try await context.service.stage(projectRoot: context.logicalRoot, paths: ["value.txt"])
        try await context.service.commit(projectRoot: context.logicalRoot, message: "initial")

        try await context.service.createBranch(
            projectRoot: context.logicalRoot,
            name: "feature/git-workbench",
            switchToBranch: true
        )
        var snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertEqual(snapshot.currentBranch, "feature/git-workbench")

        try Data("feature\n".utf8).write(to: file)
        try await context.service.stage(projectRoot: context.logicalRoot, paths: ["value.txt"])
        try await context.service.commit(projectRoot: context.logicalRoot, message: "feature change")
        try await context.service.switchBranch(projectRoot: context.logicalRoot, branch: "main")
        try await context.service.mergeBranch(
            projectRoot: context.logicalRoot,
            branch: "feature/git-workbench"
        )

        snapshot = try await context.service.snapshot(projectRoot: context.logicalRoot)
        XCTAssertEqual(snapshot.currentBranch, "main")
        XCTAssertEqual(snapshot.commits.first?.subject, "feature change")
        XCTAssertEqual(try String(contentsOf: file, encoding: .utf8), "feature\n")
    }

    private func makeContext() throws -> GitTestContext {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-git-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.deviceID = "device"
        state.workspaces = [
            .init(id: "workspace", alias: "test", absoluteRoot: root.path, fingerprint: "test"),
        ]
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: GitTicketProvider()
        )
        return GitTestContext(
            root: root,
            logicalRoot: "local://connector/device/workspace",
            service: NativeProjectGitService(connector: connector)
        )
    }

    private func configureIdentity(in root: URL) throws {
        try runGit(["config", "user.name", "ChatOS Tests"], in: root)
        try runGit(["config", "user.email", "tests@chatos.local"], in: root)
    }

    private func runGit(_ arguments: [String], in root: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = arguments
        process.currentDirectoryURL = root
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0)
    }

    private func gitFixtureProcessExists(_ processID: pid_t) -> Bool {
        Darwin.kill(processID, 0) == 0 || errno == EPERM
    }
}

private struct GitTestContext {
    var root: URL
    var logicalRoot: String
    var service: NativeProjectGitService
}

private struct GitTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
