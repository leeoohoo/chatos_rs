@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentExecutionDirectoryResolverTests: XCTestCase {
    func testApprovalUsesCommandWorkingDirectoryInsideProject() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let backend = fixture.projectRoot.appendingPathComponent("backend", isDirectory: true)
        try FileManager.default.createDirectory(at: backend, withIntermediateDirectories: true)

        let invocation = Self.invocation(path: "backend")
        let approvalCWD = try NativeLocalAgentToolApprovalHandler.approvalWorkingDirectory(
            for: invocation,
            projectRoot: fixture.projectRoot
        )

        XCTAssertEqual(approvalCWD.path, backend.resolvingSymlinksInPath().path)
    }

    func testApprovalDefaultsCommandWorkingDirectoryToProjectRoot() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }

        let approvalCWD = try NativeLocalAgentToolApprovalHandler.approvalWorkingDirectory(
            for: Self.invocation(path: nil),
            projectRoot: fixture.projectRoot
        )

        XCTAssertEqual(
            approvalCWD.path,
            fixture.projectRoot.resolvingSymlinksInPath().path
        )
    }

    func testResolverRejectsTraversalOutsideProject() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let outside = fixture.root.appendingPathComponent("outside", isDirectory: true)
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)

        XCTAssertThrowsError(try NativeLocalAgentExecutionDirectoryResolver.resolve(
            "../outside",
            projectRoot: fixture.projectRoot
        )) { error in
            XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .invalidField("path"))
        }
    }

    func testResolverRejectsSymlinkOutsideProject() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let outside = fixture.root.appendingPathComponent("outside", isDirectory: true)
        let link = fixture.projectRoot.appendingPathComponent("escape")
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: outside)

        XCTAssertThrowsError(try NativeLocalAgentExecutionDirectoryResolver.resolve(
            "escape",
            projectRoot: fixture.projectRoot
        )) { error in
            XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .invalidField("path"))
        }
    }

    func testResolverRejectsMissingDirectoryAndFile() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let file = fixture.projectRoot.appendingPathComponent("pom.xml")
        try Data().write(to: file)

        for path in ["missing", "pom.xml"] {
            XCTAssertThrowsError(try NativeLocalAgentExecutionDirectoryResolver.resolve(
                path,
                projectRoot: fixture.projectRoot
            )) { error in
                XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .invalidField("path"))
            }
        }
    }

    private static func invocation(path: String?) -> LocalAgentToolInvocationRecord {
        var arguments: [String: LocalAgentJSONValue] = [
            "command": .string("pwd"),
            "purpose": .string("检查命令工作目录"),
        ]
        if let path { arguments["path"] = .string(path) }
        return .init(
            invocationID: "invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "execute_command",
            executionRoute: "platform",
            arguments: .object(arguments),
            sideEffecting: true,
            requiresApproval: true,
            approvalStatus: "pending",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "running",
            result: nil,
            error: nil,
            version: 1,
            claimToken: "claim-token",
            claimUntilUnixMs: 2,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }
}

private struct Fixture {
    let root: URL
    let projectRoot: URL

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("approval-cwd-\(UUID().uuidString)", isDirectory: true)
        projectRoot = root.appendingPathComponent("project", isDirectory: true)
        try FileManager.default.createDirectory(at: projectRoot, withIntermediateDirectories: true)
    }

    func remove() {
        try? FileManager.default.removeItem(at: root)
    }
}
