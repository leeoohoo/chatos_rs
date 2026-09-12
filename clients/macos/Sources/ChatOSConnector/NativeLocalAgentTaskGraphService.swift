// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

/// Task inspection backed exclusively by the owner-scoped Rust Local Agent Host.
/// Rust owns graph membership, Run identity, result extraction and event ordering;
/// this type only maps the shared wire projection into macOS presentation models.
public struct NativeLocalAgentTaskGraphService: MessageTaskGraphServicing {
    private let accountSession: any NativeLocalAgentAccountSessionAccess

    public init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        self.accountSession = accountSession
    }

    public func fetchGraph(
        sourceThreadID: String,
        sourceTurnID: String
    ) async throws -> MessageTaskGraphSnapshot {
        let client = try await accountSession.activeClient()
        let graph = try await client.taskGraph(
            sourceThreadID: sourceThreadID,
            sourceTurnID: sourceTurnID
        )
        return MessageTaskGraphSnapshot(
            rootTaskIDs: graph.rootTaskIDs,
            nodes: graph.nodes.map { node in
                MessageTaskGraphNode(
                    task: map(node.task),
                    depth: Int(node.depth),
                    isRoot: node.isRoot,
                    isCurrentMessage: true
                )
            },
            edges: graph.edges.map { edge in
                MessageTaskGraphEdge(
                    id: edge.edgeID,
                    sourceID: edge.sourceTaskID,
                    targetID: edge.targetTaskID,
                    kind: edge.kind
                )
            },
            sourceSessionID: graph.sourceThreadID,
            sourceTurnID: graph.sourceTurnID
        )
    }

    public func fetchTask(taskID: String) async throws -> MessageTask {
        let client = try await accountSession.activeClient()
        let task = try await client.task(id: taskID)
        let detail = try await client.taskRunDetail(
            taskID: task.taskID,
            runID: task.currentRunID,
            eventLimit: 1,
            eventOffset: 0
        )
        return map(
            LocalAgentTaskProjection(
                task: detail.task,
                currentRun: detail.run
            )
        )
    }

    public func fetchRun(
        taskID: String,
        runID: String,
        includeEvents: Bool,
        eventLimit: Int,
        eventOffset: Int
    ) async throws -> MessageTaskRunDetail {
        let client = try await accountSession.activeClient()
        let limit = includeEvents ? min(max(eventLimit, 1), 500) : 1
        let detail = try await client.taskRunDetail(
            taskID: taskID,
            runID: runID,
            eventLimit: UInt32(limit),
            eventOffset: UInt32(max(eventOffset, 0))
        )
        return MessageTaskRunDetail(
            task: map(
                LocalAgentTaskProjection(
                    task: detail.task,
                    currentRun: detail.run
                )
            ),
            run: map(detail.run, taskID: detail.task.taskID),
            events: includeEvents ? detail.events.map(map) : [],
            eventsTotal: includeEvents ? Int(detail.eventsTotal) : 0,
            eventsHasMore: includeEvents && detail.eventsHasMore
        )
    }

    public func retryTask(
        taskID: String,
        expectedRunID: String,
        instruction: String?
    ) async throws -> MessageTaskRun {
        let client = try await accountSession.activeClient()
        let result = try await client.retryTask(
            LocalAgentRetryTask(
                taskID: taskID,
                expectedRunID: expectedRunID,
                instruction: instruction?
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                    .nilIfEmpty
            )
        )
        return map(LocalAgentTaskRunSummary(run: result.run), taskID: taskID)
    }

    public func cancelTask(taskID: String) async throws {
        let client = try await accountSession.activeClient()
        let task = try await client.task(id: taskID)
        _ = try await client.accepted(.cancelRun(runID: task.currentRunID))
    }

    private func map(_ projection: LocalAgentTaskProjection) -> MessageTask {
        let task = projection.task
        let current = projection.currentRun
        let run = map(current, taskID: task.taskID)
        return MessageTask(
            id: task.taskID,
            title: task.objective,
            description: task.acceptanceCriteria.joined(separator: "\n"),
            objective: task.objective,
            status: run.status,
            defaultModelConfigID: task.modelConfigID,
            resultSummary: current.resultSummary,
            lastRunID: task.currentRunID,
            lastRunStatus: run.status,
            lastRun: MessageTaskLastRunSummary(
                id: run.id,
                status: run.status,
                modelPhaseStatus: run.modelPhaseStatus,
                resultSummary: run.resultSummary,
                reportContent: run.reportContent,
                errorMessage: run.errorMessage,
                startedAt: run.startedAt,
                finishedAt: run.finishedAt
            ),
            sourceSessionID: task.sourceThreadID,
            sourceTurnID: task.sourceTurnID,
            createdAt: Self.date(task.createdAt),
            updatedAt: Self.date(task.updatedAt)
        )
    }

    private func map(
        _ summary: LocalAgentTaskRunSummary,
        taskID: String
    ) -> MessageTaskRun {
        let run = summary.run
        return MessageTaskRun(
            id: run.runID,
            taskID: taskID,
            status: run.status.rawValue,
            modelPhaseStatus: "iteration_\(run.iteration)",
            startedAt: Self.date(run.createdAt),
            finishedAt: run.status.isTerminal ? Self.date(run.updatedAt) : nil,
            resultSummary: summary.resultSummary,
            reportContent: summary.reportContent,
            errorMessage: summary.errorMessage
        )
    }

    private func map(_ event: LocalAgentRunTimelineEvent) -> MessageTaskRunEvent {
        MessageTaskRunEvent(
            id: event.eventID,
            eventType: event.eventType,
            message: event.message,
            createdAt: Self.date(event.createdAt)
        )
    }

    private static func date(_ value: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = fractional.date(from: value) { return date }
        let wholeSeconds = ISO8601DateFormatter()
        wholeSeconds.formatOptions = [.withInternetDateTime]
        return wholeSeconds.date(from: value)
    }
}

private extension LocalAgentRunStatus {
    var isTerminal: Bool {
        self == .succeeded || self == .failed || self == .cancelled
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
