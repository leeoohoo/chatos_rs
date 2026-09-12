// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public enum LocalAgentTaskStateError: Error, Equatable, Sendable {
    case missingRun(String)
    case missingTask(String)
    case identityMismatch(String)
}

extension LocalAgentTaskStateError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .missingRun(taskID):
            "本地任务 \(taskID) 没有对应的 Task Runner Run"
        case let .missingTask(runID):
            "Task Runner Run \(runID) 没有对应的本地任务"
        case let .identityMismatch(reason):
            "本地任务身份不一致：\(reason)"
        }
    }
}

public struct LocalAgentTaskModelStepState: Equatable, Sendable {
    public var stepSequence: UInt64
    public var content: String
    public var reasoning: String
    public var status: String

    public init(
        stepSequence: UInt64,
        content: String = "",
        reasoning: String = "",
        status: String = ""
    ) {
        self.stepSequence = stepSequence
        self.content = content
        self.reasoning = reasoning
        self.status = status
    }
}

public struct LocalAgentTaskState: Identifiable, Equatable, Sendable {
    public var id: String { task.taskID }
    public var task: LocalAgentTaskSnapshot
    public var run: LocalAgentRunSnapshot
    public var modelSteps: [LocalAgentTaskModelStepState]
    public var tools: [LocalAgentToolSnapshot]
    public var pendingInteraction: LocalAgentUserInteractionEvent?
    public var userPrompt: AskUserPrompt?
    public var memorySync: LocalAgentMemorySyncStatus?
    public var lastAppliedEventSequence: UInt64

    public init(
        task: LocalAgentTaskSnapshot,
        run: LocalAgentRunSnapshot,
        modelSteps: [LocalAgentTaskModelStepState] = [],
        tools: [LocalAgentToolSnapshot] = [],
        pendingInteraction: LocalAgentUserInteractionEvent? = nil,
        userPrompt: AskUserPrompt? = nil,
        memorySync: LocalAgentMemorySyncStatus? = nil,
        lastAppliedEventSequence: UInt64 = 0
    ) {
        self.task = task
        self.run = run
        self.modelSteps = modelSteps
        self.tools = tools
        self.pendingInteraction = pendingInteraction
        self.userPrompt = userPrompt
        self.memorySync = memorySync
        self.lastAppliedEventSequence = lastAppliedEventSequence
    }
}

public protocol LocalAgentTaskStateStoring: Sendable {
    func restoreLocalAgentTasks(
        _ tasks: [LocalAgentTaskSnapshot],
        runs: [LocalAgentRunSnapshot]
    ) async throws
    func registerLocalAgentTask(
        _ task: LocalAgentTaskSnapshot,
        run: LocalAgentRunSnapshot
    ) async throws
    func applyLocalAgentTaskEvent(_ event: LocalAgentUIEvent) async throws
    func localAgentTasks(sessionID: String?) async -> [LocalAgentTaskState]
    func localAgentTask(taskID: String) async -> LocalAgentTaskState?
    func localAgentTaskUpdates(sessionID: String) async -> AsyncStream<Void>
}

/// Native Task Runner projection derived only from Host Task/Run snapshots and
/// the durable UI event stream. It is an in-memory presentation projection;
/// the selected Client Storage Provider remains the authority across restarts.
public actor LocalAgentTaskStateStore: LocalAgentTaskStateStoring {
    private var tasksByID: [String: LocalAgentTaskState] = [:]
    private var taskIDByRunID: [String: String] = [:]
    private var updateContinuations: [String: [UUID: AsyncStream<Void>.Continuation]] = [:]

    public init() {}

    public func restoreLocalAgentTasks(
        _ tasks: [LocalAgentTaskSnapshot],
        runs: [LocalAgentRunSnapshot]
    ) throws {
        let taskRuns = runs.filter { $0.profileKey == "task_runner" }
        let runsByID = Dictionary(uniqueKeysWithValues: taskRuns.map { ($0.runID, $0) })
        var restored: [String: LocalAgentTaskState] = [:]
        var restoredTaskIDByRunID: [String: String] = [:]

        for task in tasks {
            guard let run = runsByID[task.runID] else {
                throw LocalAgentTaskStateError.missingRun(task.taskID)
            }
            try Self.validate(task: task, run: run)
            guard restored[task.taskID] == nil, restoredTaskIDByRunID[run.runID] == nil else {
                throw LocalAgentTaskStateError.identityMismatch("任务或 Run ID 重复")
            }
            let interaction = LocalAgentUIPresentation.pendingUserInteraction(run)
            restored[task.taskID] = LocalAgentTaskState(
                task: task,
                run: run,
                pendingInteraction: interaction,
                userPrompt: interaction.map {
                    LocalAgentUIPresentation.askUserPrompt(
                        $0,
                        sessionID: task.sourceThreadID,
                        turnID: task.sourceTurnID,
                        emittedAt: run.updatedAt
                    )
                }
            )
            restoredTaskIDByRunID[run.runID] = task.taskID
        }
        for run in taskRuns where restoredTaskIDByRunID[run.runID] == nil {
            throw LocalAgentTaskStateError.missingTask(run.runID)
        }

        let changedSessions = Set(tasksByID.values.map(\.task.sourceThreadID))
            .union(restored.values.map(\.task.sourceThreadID))
        tasksByID = restored
        taskIDByRunID = restoredTaskIDByRunID
        changedSessions.forEach(notify)
    }

    public func registerLocalAgentTask(
        _ task: LocalAgentTaskSnapshot,
        run: LocalAgentRunSnapshot
    ) throws {
        try Self.validate(task: task, run: run)
        if let existingTaskID = taskIDByRunID[run.runID], existingTaskID != task.taskID {
            throw LocalAgentTaskStateError.identityMismatch("同一个 Run 关联到两个任务")
        }
        if let existing = tasksByID[task.taskID], existing.run.runID != run.runID {
            throw LocalAgentTaskStateError.identityMismatch("同一个任务关联到两个 Run")
        }
        let previous = tasksByID[task.taskID]
        let restoredInteraction = LocalAgentUIPresentation.pendingUserInteraction(run)
        let userPrompt: AskUserPrompt?
        if let previousPrompt = previous?.userPrompt,
           previousPrompt.id == restoredInteraction?.interactionID {
            userPrompt = previousPrompt
        } else if let restoredInteraction {
            userPrompt = LocalAgentUIPresentation.askUserPrompt(
                restoredInteraction,
                sessionID: task.sourceThreadID,
                turnID: task.sourceTurnID,
                emittedAt: run.updatedAt
            )
        } else {
            userPrompt = previous?.userPrompt
        }
        tasksByID[task.taskID] = LocalAgentTaskState(
            task: task,
            run: run,
            modelSteps: previous?.modelSteps ?? [],
            tools: previous?.tools ?? [],
            pendingInteraction: restoredInteraction,
            userPrompt: userPrompt,
            memorySync: previous?.memorySync,
            lastAppliedEventSequence: previous?.lastAppliedEventSequence ?? 0
        )
        taskIDByRunID[run.runID] = task.taskID
        notify(task.sourceThreadID)
    }

    public func applyLocalAgentTaskEvent(_ event: LocalAgentUIEvent) throws {
        guard let runID = event.event.runID,
              let taskID = taskIDByRunID[runID],
              var state = tasksByID[taskID]
        else { return }
        guard event.eventSeq > state.lastAppliedEventSequence else { return }

        switch event.event {
        case let .runSnapshot(run):
            try Self.validate(task: state.task, run: run)
            state.run = run
            if let interaction = LocalAgentUIPresentation.pendingUserInteraction(run) {
                state.pendingInteraction = interaction
                if state.userPrompt?.id != interaction.interactionID {
                    state.userPrompt = LocalAgentUIPresentation.askUserPrompt(
                        interaction,
                        sessionID: state.task.sourceThreadID,
                        turnID: state.task.sourceTurnID,
                        emittedAt: event.emittedAt
                    )
                }
            } else if run.status != .paused {
                state.pendingInteraction = nil
                if state.userPrompt?.status.isPending == true {
                    state.userPrompt?.status = run.status == .cancelled ? .canceled
                        : (run.status == .failed ? .failed : .ok)
                    state.userPrompt?.updatedAt = LocalAgentUIPresentation.date(run.updatedAt)
                }
            }
        case let .modelStream(stream):
            let index: Int
            if let existing = state.modelSteps.firstIndex(where: {
                $0.stepSequence == stream.stepSeq
            }) {
                index = existing
            } else {
                state.modelSteps.append(LocalAgentTaskModelStepState(
                    stepSequence: stream.stepSeq
                ))
                state.modelSteps.sort { $0.stepSequence < $1.stepSequence }
                index = state.modelSteps.firstIndex(where: {
                    $0.stepSequence == stream.stepSeq
                })!
            }
            switch stream.deltaKind {
            case .content: state.modelSteps[index].content += stream.delta
            case .reasoning: state.modelSteps[index].reasoning += stream.delta
            case .status: state.modelSteps[index].status += stream.delta
            }
        case let .toolSnapshot(tool):
            if let index = state.tools.firstIndex(where: {
                $0.invocationID == tool.invocationID
            }) {
                state.tools[index] = tool
            } else {
                state.tools.append(tool)
            }
        case let .userInteraction(interaction):
            state.pendingInteraction = interaction
            state.userPrompt = LocalAgentUIPresentation.askUserPrompt(
                interaction,
                sessionID: state.task.sourceThreadID,
                turnID: state.task.sourceTurnID,
                emittedAt: event.emittedAt
            )
        case let .memorySync(status):
            state.memorySync = status
        case .hostStatus:
            return
        }
        state.lastAppliedEventSequence = event.eventSeq
        tasksByID[taskID] = state
        notify(state.task.sourceThreadID)
    }

    public func localAgentTasks(sessionID: String? = nil) -> [LocalAgentTaskState] {
        tasksByID.values
            .filter { sessionID == nil || $0.task.sourceThreadID == sessionID }
            .sorted {
                if $0.run.updatedAt != $1.run.updatedAt {
                    return $0.run.updatedAt > $1.run.updatedAt
                }
                return $0.task.taskID < $1.task.taskID
            }
    }

    public func localAgentTask(taskID: String) -> LocalAgentTaskState? {
        tasksByID[taskID]
    }

    public func localAgentTaskUpdates(sessionID: String) -> AsyncStream<Void> {
        let subscriptionID = UUID()
        return AsyncStream { continuation in
            updateContinuations[sessionID, default: [:]][subscriptionID] = continuation
            continuation.onTermination = { [weak self] _ in
                Task { await self?.removeContinuation(subscriptionID, sessionID: sessionID) }
            }
        }
    }

    public func localAgentPrompts(
        sessionID: String,
        limit: Int
    ) -> [AskUserPrompt] {
        guard limit > 0 else { return [] }
        return Array(tasksByID.values
            .filter { $0.task.sourceThreadID == sessionID }
            .compactMap(\.userPrompt)
            .sorted(by: Self.promptOrder)
            .suffix(limit))
    }

    public func localAgentPromptRoute(
        promptID: String,
        sessionID: String
    ) throws -> LocalAgentAskUserRoute {
        guard let state = tasksByID.values.first(where: {
            $0.task.sourceThreadID == sessionID
                && $0.userPrompt?.id == promptID
                && $0.userPrompt?.status.isPending == true
        }), let interaction = state.pendingInteraction else {
            throw LocalAgentConversationHistoryError.promptUnavailable
        }
        return LocalAgentAskUserRoute(
            runID: state.run.runID,
            interactionID: interaction.interactionID
        )
    }

    public func updateLocalAgentPromptStatus(
        promptID: String,
        sessionID: String,
        status: AskUserPromptStatus
    ) throws -> AskUserPrompt {
        guard let taskID = tasksByID.first(where: {
            $0.value.task.sourceThreadID == sessionID
                && $0.value.userPrompt?.id == promptID
        })?.key, var state = tasksByID[taskID], var prompt = state.userPrompt else {
            throw LocalAgentConversationHistoryError.promptUnavailable
        }
        prompt.status = status
        prompt.updatedAt = Date()
        state.userPrompt = prompt
        if !status.isPending { state.pendingInteraction = nil }
        tasksByID[taskID] = state
        notify(sessionID)
        return prompt
    }

    public func localAgentRunControls(sessionID: String) -> [LocalAgentRunControlState] {
        tasksByID.values
            .filter { $0.task.sourceThreadID == sessionID }
            .map {
                LocalAgentUIPresentation.runControl(
                    $0.run,
                    sessionID: $0.task.sourceThreadID,
                    turnID: $0.task.sourceTurnID
                )
            }
            .filter { !$0.isTerminal }
            .sorted(by: Self.controlOrder)
    }

    public func localAgentPendingToolApprovals(
        sessionID: String
    ) -> [LocalAgentToolApprovalRequest] {
        tasksByID.values
            .filter {
                $0.task.sourceThreadID == sessionID
                    && !LocalAgentUIPresentation.runControl(
                        $0.run,
                        sessionID: $0.task.sourceThreadID,
                        turnID: $0.task.sourceTurnID
                    ).isTerminal
            }
            .flatMap { state in
                state.tools
                    .filter { $0.status == .awaitingApproval }
                    .map {
                        LocalAgentUIPresentation.toolApproval(
                            $0,
                            sessionID: state.task.sourceThreadID,
                            turnID: state.task.sourceTurnID
                        )
                    }
            }
            .sorted { $0.invocationID < $1.invocationID }
    }

    public func requireLocalAgentRunControl(
        runID: String,
        sessionID: String
    ) throws -> LocalAgentRunControlState {
        guard let state = tasksByID.values.first(where: {
            $0.run.runID == runID && $0.task.sourceThreadID == sessionID
        }) else {
            throw LocalAgentConversationHistoryError.runUnavailable
        }
        let control = LocalAgentUIPresentation.runControl(
            state.run,
            sessionID: state.task.sourceThreadID,
            turnID: state.task.sourceTurnID
        )
        guard !control.isTerminal else {
            throw LocalAgentConversationHistoryError.runUnavailable
        }
        return control
    }

    public func requireLocalAgentToolApproval(
        invocationID: String,
        sessionID: String
    ) throws -> LocalAgentToolApprovalRequest {
        guard let state = tasksByID.values.first(where: {
            $0.task.sourceThreadID == sessionID
                && !LocalAgentUIPresentation.runControl(
                    $0.run,
                    sessionID: $0.task.sourceThreadID,
                    turnID: $0.task.sourceTurnID
                ).isTerminal
                && $0.tools.contains(where: {
                    $0.invocationID == invocationID && $0.status == .awaitingApproval
                })
        }), let tool = state.tools.first(where: { $0.invocationID == invocationID }) else {
            throw LocalAgentConversationHistoryError.toolApprovalUnavailable
        }
        return LocalAgentUIPresentation.toolApproval(
            tool,
            sessionID: state.task.sourceThreadID,
            turnID: state.task.sourceTurnID
        )
    }

    private func removeContinuation(_ id: UUID, sessionID: String) {
        updateContinuations[sessionID]?[id] = nil
        if updateContinuations[sessionID]?.isEmpty == true {
            updateContinuations[sessionID] = nil
        }
    }

    private func notify(_ sessionID: String) {
        updateContinuations[sessionID]?.values.forEach { $0.yield(()) }
    }

    private static func validate(
        task: LocalAgentTaskSnapshot,
        run: LocalAgentRunSnapshot
    ) throws {
        guard run.profileKey == "task_runner",
              run.ownerEntityType == "task",
              run.ownerEntityID == task.taskID,
              run.runID == task.runID,
              run.ownerUserID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false,
              run.projectID == task.projectID,
              run.modelConfigID == task.modelConfigID,
              run.modelConfigRevision == task.modelConfigRevision
        else {
            throw LocalAgentTaskStateError.identityMismatch(
                "Task、Run、项目或模型快照不匹配"
            )
        }
    }

    private static func promptOrder(_ lhs: AskUserPrompt, _ rhs: AskUserPrompt) -> Bool {
        let left = lhs.createdAt ?? lhs.updatedAt ?? .distantPast
        let right = rhs.createdAt ?? rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.id < rhs.id
    }

    private static func controlOrder(
        _ lhs: LocalAgentRunControlState,
        _ rhs: LocalAgentRunControlState
    ) -> Bool {
        let left = lhs.updatedAt ?? .distantPast
        let right = rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.runID < rhs.runID
    }
}

extension LocalAgentTaskStateStore: LocalAgentAskUserStateStoring {}
extension LocalAgentTaskStateStore: LocalAgentRunControlStateStoring {}

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
