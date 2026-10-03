import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeTerminalExecutorTests {
    @Test
    func veryLargeOutputIsBoundedBeforeItAccumulatesInMemory() async throws {
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

        let result = try await NativeTerminalExecutor.execute(
            command: "/usr/bin/python3",
            args: ["-c", "import sys; sys.stdout.write('x' * 3000000)"],
            cwd: root.path,
            workspace: workspace
        )

        #expect(result.success)
        #expect(result.stdout.utf8.count == 512 * 1_024)
    }

    @Test
    func rejectsWorkingDirectoryOutsideSelectedWorkspace() async throws {
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

        await #expect(throws: NativeConnectorError.self) {
            _ = try await NativeTerminalExecutor.execute(
                command: "/usr/bin/pwd",
                args: [],
                cwd: outside.path,
                workspace: workspace
            )
        }
    }

    @Test
    func executesInsideSelectedWorkspace() async throws {
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

        let result = try await NativeTerminalExecutor.execute(
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
    func resolvesCommandFromPathForRelayExecution() async throws {
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

        let result = try await NativeTerminalExecutor.execute(
            command: "pwd",
            args: [],
            cwd: root.path,
            workspace: workspace
        )

        #expect(result.success)
    }

    @Test
    func hangingCommandTimesOutAndReturnsWithoutBlockingForever() async throws {
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
        let clock = ContinuousClock()
        let startedAt = clock.now

        let result = try await NativeTerminalExecutor.execute(
            command: "/bin/sleep",
            args: ["60"],
            cwd: root.path,
            workspace: workspace,
            timeout: 0.05
        )

        #expect(result.timedOut)
        #expect(!result.success)
        #expect(result.error?.contains("超时") == true)
        #expect(clock.now - startedAt < .seconds(3))
    }

    @Test
    func cancellingCommandKillsItsProcessGroupPromptly() async throws {
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
        let clock = ContinuousClock()
        let startedAt = clock.now
        let execution = Task {
            try await NativeTerminalExecutor.execute(
                command: "/bin/sleep",
                args: ["60"],
                cwd: root.path,
                workspace: workspace
            )
        }
        try await Task.sleep(for: .milliseconds(50))
        execution.cancel()

        await #expect(throws: CancellationError.self) {
            _ = try await execution.value
        }
        #expect(clock.now - startedAt < .seconds(3))
    }
}
