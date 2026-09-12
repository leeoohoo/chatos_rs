// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public protocol NativeLocalAgentTaskStateClient: Sendable {
    func run(id: String) async throws -> LocalAgentRunSnapshot
    func task(id: String) async throws -> LocalAgentTaskSnapshot
    func runs(cursor: String?, limit: UInt32) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    )
    func tasks(cursor: String?, limit: UInt32) async throws -> (
        tasks: [LocalAgentTaskSnapshot], nextCursor: String?
    )
}

extension NativeLocalAgentIPCClient: NativeLocalAgentTaskStateClient {}

public enum NativeLocalAgentTaskEventSinkError: Error, Equatable, Sendable {
    case paginationDidNotAdvance(String)
    case invalidRunIdentity(String)
}

extension NativeLocalAgentTaskEventSinkError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .paginationDidNotAdvance(kind):
            "本地 Agent \(kind) 分页游标没有推进"
        case let .invalidRunIdentity(runID):
            "本地 Task Runner 返回了不一致的 Run：\(runID)"
        }
    }
}

/// Restores and incrementally maintains the native Task Runner projection.
/// It never calls the remote Task Runner API and never treats event replay as
/// the authority after a Host or app restart.
public actor NativeLocalAgentTaskEventSink: LocalAgentUIEventApplying {
    private static let pageLimit: UInt32 = 500

    private let client: any NativeLocalAgentTaskStateClient
    private let store: any LocalAgentTaskStateStoring
    private var taskRunIDs = Set<String>()
    private var ignoredRunIDs = Set<String>()

    public init(
        client: any NativeLocalAgentTaskStateClient,
        store: any LocalAgentTaskStateStoring
    ) {
        self.client = client
        self.store = store
    }

    public func restore() async throws {
        async let tasks = allTasks()
        async let runs = allRuns()
        let (taskSnapshots, runSnapshots) = try await (tasks, runs)
        try await store.restoreLocalAgentTasks(taskSnapshots, runs: runSnapshots)
        taskRunIDs = Set(taskSnapshots.map(\.runID))
        ignoredRunIDs = Set(runSnapshots.lazy
            .filter { $0.profileKey != "task_runner" }
            .map(\.runID))
    }

    public func applyLocalAgentUIEvent(
        _ event: LocalAgentUIEvent,
        mainChatBinding: LocalAgentMainChatRunBinding?
    ) async throws {
        if mainChatBinding != nil { return }
        guard let runID = event.event.runID else { return }
        if ignoredRunIDs.contains(runID) { return }
        if !taskRunIDs.contains(runID) {
            let run: LocalAgentRunSnapshot
            if case let .runSnapshot(snapshot) = event.event {
                run = snapshot
            } else {
                run = try await client.run(id: runID)
            }
            guard run.runID == runID else {
                throw NativeLocalAgentTaskEventSinkError.invalidRunIdentity(runID)
            }
            guard run.profileKey == "task_runner" else {
                ignoredRunIDs.insert(runID)
                return
            }
            let task = try await client.task(id: run.ownerEntityID)
            try await store.registerLocalAgentTask(task, run: run)
            taskRunIDs.insert(runID)
        }
        try await store.applyLocalAgentTaskEvent(event)
    }

    private func allRuns() async throws -> [LocalAgentRunSnapshot] {
        var cursor: String?
        var values: [LocalAgentRunSnapshot] = []
        repeat {
            let page = try await client.runs(cursor: cursor, limit: Self.pageLimit)
            values.append(contentsOf: page.runs)
            try validateNextCursor(page.nextCursor, previous: cursor, kind: "Run")
            cursor = page.nextCursor
        } while cursor != nil
        return values
    }

    private func allTasks() async throws -> [LocalAgentTaskSnapshot] {
        var cursor: String?
        var values: [LocalAgentTaskSnapshot] = []
        repeat {
            let page = try await client.tasks(cursor: cursor, limit: Self.pageLimit)
            values.append(contentsOf: page.tasks)
            try validateNextCursor(page.nextCursor, previous: cursor, kind: "Task")
            cursor = page.nextCursor
        } while cursor != nil
        return values
    }

    private func validateNextCursor(
        _ next: String?,
        previous: String?,
        kind: String
    ) throws {
        if let next, next == previous {
            throw NativeLocalAgentTaskEventSinkError.paginationDidNotAdvance(kind)
        }
    }
}

/// One account-level replay stream fans out to presentation projections. A
/// page is acknowledged only after every projection accepts every event.
public actor NativeLocalAgentCompositeEventSink: LocalAgentUIEventApplying {
    private let sinks: [any LocalAgentUIEventApplying]

    public init(sinks: [any LocalAgentUIEventApplying]) {
        precondition(!sinks.isEmpty)
        self.sinks = sinks
    }

    public func applyLocalAgentUIEvent(
        _ event: LocalAgentUIEvent,
        mainChatBinding: LocalAgentMainChatRunBinding?
    ) async throws {
        for sink in sinks {
            try await sink.applyLocalAgentUIEvent(
                event,
                mainChatBinding: mainChatBinding
            )
        }
    }
}

private extension LocalAgentUIEventPayload {
    var runID: String? {
        switch self {
        case let .runSnapshot(run): run.runID
        case let .modelStream(event): event.runID
        case let .toolSnapshot(tool): tool.runID
        case let .userInteraction(interaction): interaction.runID
        case let .memorySync(status): status.runID
        case .hostStatus: nil
        }
    }
}
