import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeTerminalExecutorTests {
    @Test
    func rejectsWorkingDirectoryOutsideSelectedWorkspace() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let outside = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        defer {
            try? FileManager.default.removeItem(at: root)
            try? FileManager.default.removeItem(at: outside)
        }
        let workspace = LocalConnectorWorkspace(
            id: "workspace-1",
            alias: "Project",
            absoluteRoot: root.path,
            fingerprint: "abc"
        )

        #expect(throws: NativeConnectorError.self) {
            _ = try NativeTerminalExecutor.execute(
                command: "/usr/bin/pwd",
                args: [],
                cwd: outside.path,
                workspace: workspace
            )
        }
    }

    @Test
    func executesInsideSelectedWorkspace() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let workspace = LocalConnectorWorkspace(
            id: "workspace-1",
            alias: "Project",
            absoluteRoot: root.path,
            fingerprint: "abc"
        )

        let result = try NativeTerminalExecutor.execute(
            command: "/bin/pwd",
            args: [],
            cwd: root.path,
            workspace: workspace
        )

        #expect(result.success)
        #expect(result.cwd == root.standardizedFileURL.resolvingSymlinksInPath().path)
        let shellWorkingDirectory = URL(
            fileURLWithPath: result.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        )
        #expect(shellWorkingDirectory.lastPathComponent == root.lastPathComponent)
    }

    @Test
    func resolvesCommandFromPathForRelayExecution() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let workspace = LocalConnectorWorkspace(
            id: "workspace-1",
            alias: "Project",
            absoluteRoot: root.path,
            fingerprint: "abc"
        )

        let result = try NativeTerminalExecutor.execute(
            command: "pwd",
            args: [],
            cwd: root.path,
            workspace: workspace
        )

        #expect(result.success)
    }
}
