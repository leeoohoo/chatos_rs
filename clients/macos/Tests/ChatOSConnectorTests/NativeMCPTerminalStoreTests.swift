import Foundation
import Testing
@testable import ChatOSConnector

struct NativeMCPTerminalStoreTests {
    @Test
    func backgroundProcessCanBePolledAndWaited() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let started = try await store.execute(
            command: "printf 'first\\n'; sleep 0.1; printf 'second\\n'",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true
        )
        let id = try started.string("terminal_id")
        let waited = try await store.call(
            name: "process_wait",
            arguments: [
                "terminal_id": .string(id),
                "timeout_ms": .number(5_000),
            ],
            projectRoot: fixture.root
        )
        let output = try waited.string("output")

        #expect(output.contains("first"))
        #expect(output.contains("second"))
        #expect(try waited.bool("exited"))
    }

    @Test
    func oneExitEventResumesAllConcurrentWaiters() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let started = try await store.execute(
            command: "sleep 0.2",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true
        )
        let id = try started.string("terminal_id")
        let root = fixture.root

        let results = try await withThrowingTaskGroup(of: Bool.self) { group in
            for _ in 0..<12 {
                group.addTask {
                    let waited = try await store.call(
                        name: "process_wait",
                        arguments: [
                            "terminal_id": .string(id),
                            "timeout_ms": .number(5_000),
                        ],
                        projectRoot: root
                    )
                    return try waited.bool("exited")
                }
            }
            var values: [Bool] = []
            for try await value in group { values.append(value) }
            return values
        }

        #expect(results.count == 12)
        #expect(results.allSatisfy { $0 })
    }

    @Test
    func stdinCanBeWrittenToRunningProcess() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let started = try await store.execute(
            command: "read value; printf 'received:%s\\n' \"$value\"",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true
        )
        let id = try started.string("terminal_id")
        _ = try await store.call(
            name: "process_write",
            arguments: [
                "terminal_id": .string(id),
                "data": .string("hello"),
                "submit": .bool(true),
            ],
            projectRoot: fixture.root
        )
        let waited = try await store.call(
            name: "process_wait",
            arguments: [
                "terminal_id": .string(id),
                "timeout_ms": .number(5_000),
            ],
            projectRoot: fixture.root
        )

        #expect(try waited.string("output").contains("received:hello"))
    }

    @Test
    func cancellingForegroundExecutionTerminatesItsShellBeforeLateSideEffect() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let marker = fixture.root.appendingPathComponent("foreground-late.txt")
        let execution = Task {
            try await store.execute(
                command: "sleep 0.6; printf late > foreground-late.txt",
                cwd: fixture.root,
                projectRoot: fixture.root,
                background: false,
                ownerRunID: "executor-run-foreground"
            )
        }

        try await Task.sleep(for: .milliseconds(100))
        execution.cancel()
        do {
            _ = try await execution.value
            Issue.record("Cancelled foreground terminal execution unexpectedly completed")
        } catch is CancellationError {
            // Expected: cancellation must propagate instead of becoming a normal tool failure.
        }
        try await Task.sleep(for: .milliseconds(700))

        #expect(!FileManager.default.fileExists(atPath: marker.path))
    }

    @Test
    func foregroundExecutionTimesOutWithActionableFailureAndStopsLateSideEffect() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let marker = fixture.root.appendingPathComponent("foreground-timeout-late.txt")

        let result = try await store.execute(
            command: "/bin/sh -c 'sleep 2; printf late > foreground-timeout-late.txt'",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: false,
            timeoutMilliseconds: 1_000,
            ownerRunID: "executor-run-timeout"
        )

        #expect(try result.bool("timed_out"))
        #expect(!(try result.bool("success")))
        #expect(try result.string("finished_by") == "timeout")
        #expect(try result.string("error").contains("background=true"))
        try await Task.sleep(for: .milliseconds(1_200))
        #expect(!FileManager.default.fileExists(atPath: marker.path))
    }

    @Test
    func cancellingExecutorRunTerminatesItsBackgroundProcesses() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let marker = fixture.root.appendingPathComponent("background-late.txt")
        _ = try await store.execute(
            command: "sleep 0.6; printf late > background-late.txt",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true,
            ownerRunID: "executor-run-background"
        )

        let cancelled = await store.cancel(ownerRunID: "executor-run-background")
        try await Task.sleep(for: .milliseconds(700))

        #expect(cancelled == 1)
        #expect(!FileManager.default.fileExists(atPath: marker.path))
    }

    @Test
    func processHandlesAreIsolatedByOwningLocalAgentRun() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeLocalAgentTerminalStore()
        let started = try await store.execute(
            command: "sleep 5",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true,
            ownerRunID: "run-a"
        )
        let id = try started.string("terminal_id")
        let arguments: [String: NativeJSONValue] = ["terminal_id": .string(id)]

        do {
            _ = try await store.call(
                name: "process_poll",
                arguments: arguments,
                projectRoot: fixture.root,
                ownerRunID: "run-b"
            )
            Issue.record("Another Local Agent Run accessed a process handle it does not own")
        } catch {
            // Expected: scoped calls reveal neither process state nor output across Runs.
        }
        _ = try await store.call(
            name: "process_poll",
            arguments: arguments,
            projectRoot: fixture.root,
            ownerRunID: "run-a"
        )
        _ = await store.cancel(ownerRunID: "run-a")
    }

    @Test
    func processListingIsScopedToOwningLocalAgentRunWithoutAProcessHandle() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeLocalAgentTerminalStore()
        let first = try await store.execute(
            command: "sleep 5",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true,
            ownerRunID: "run-a"
        )
        _ = try await store.execute(
            command: "sleep 5",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: true,
            ownerRunID: "run-b"
        )

        let listed = try await store.call(
            name: "process_list",
            arguments: [:],
            projectRoot: fixture.root,
            ownerRunID: "run-a"
        )
        let processes = try listed.array("processes")

        #expect(processes.count == 1)
        #expect(try processes[0].string("terminal_id") == first.string("terminal_id"))
        await store.cancelAll()
    }

    @Test
    func largeOutputIsByteBoundedAndReportsDroppedOffsets() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore()
        let completed = try await store.execute(
            command: "/usr/bin/python3 -c 'import sys; sys.stdout.write(\"x\" * 3000000)'",
            cwd: fixture.root,
            projectRoot: fixture.root,
            background: false
        )
        let id = try completed.string("terminal_id")
        let polled = try await store.call(
            name: "process_poll",
            arguments: [
                "terminal_id": .string(id),
                "offset": .number(0),
                "limit": .number(200),
            ],
            projectRoot: fixture.root
        )

        #expect(try completed.bool("truncated"))
        #expect(try completed.string("output").count <= 512 * 1_024)
        #expect(try polled.bool("truncated"))
    }

    @Test
    func exitedProcessHistoryIsBounded() async throws {
        let fixture = try TerminalFixture()
        defer { fixture.dispose() }
        let store = NativeMCPTerminalStore(maximumRetainedExitedProcesses: 2)

        for index in 0..<3 {
            let started = try await store.execute(
                command: "printf '\(index)'",
                cwd: fixture.root,
                projectRoot: fixture.root,
                background: true
            )
            _ = try await store.call(
                name: "process_wait",
                arguments: [
                    "terminal_id": .string(try started.string("terminal_id")),
                    "timeout_ms": .number(5_000),
                ],
                projectRoot: fixture.root
            )
        }

        let listed = try await store.call(
            name: "process_list",
            arguments: [
                "include_exited": .bool(true),
                "limit": .number(100),
            ],
            projectRoot: fixture.root
        )
        #expect(try listed.number("process_count") == 2)
    }
}

private struct TerminalFixture {
    let root: URL

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-terminal-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func dispose() { try? FileManager.default.removeItem(at: root) }
}

private extension NativeJSONValue {
    func string(_ key: String) throws -> String {
        guard case let .object(values) = self, case let .string(value)? = values[key] else {
            throw TerminalTestError.invalidShape
        }
        return value
    }

    func bool(_ key: String) throws -> Bool {
        guard case let .object(values) = self, case let .bool(value)? = values[key] else {
            throw TerminalTestError.invalidShape
        }
        return value
    }

    func number(_ key: String) throws -> Int {
        guard case let .object(values) = self, case let .number(value)? = values[key] else {
            throw TerminalTestError.invalidShape
        }
        return Int(value)
    }

    func array(_ key: String) throws -> [NativeJSONValue] {
        guard case let .object(values) = self, case let .array(value)? = values[key] else {
            throw TerminalTestError.invalidShape
        }
        return value
    }
}

private enum TerminalTestError: Error { case invalidShape }
