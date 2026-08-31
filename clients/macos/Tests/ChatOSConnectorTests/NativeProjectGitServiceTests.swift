@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeProjectGitServiceTests: XCTestCase {
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
}

private struct GitTestContext {
    var root: URL
    var logicalRoot: String
    var service: NativeProjectGitService
}

private struct GitTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
