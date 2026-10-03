import ChatOSAgentRuntime
import ChatOSCore
import Foundation

public struct LocalAgentGroupChatRun: Codable, Sendable, Equatable, Identifiable {
    public let id: UUID
    public let context: LocalAgentChatRunContext
    public let modelConfigID: String
    public let policy: AgentRunPolicy
    public let progressiveSkillSnapshot: LocalAgentProgressiveSkillSnapshot?
    public var checkpoint: AgentRunCheckpoint
    public var events: [AgentRunEvent]
    public let createdAtUnixMs: Int64
    public var updatedAtUnixMs: Int64

    public init(
        id: UUID,
        context: LocalAgentChatRunContext,
        modelConfigID: String,
        policy: AgentRunPolicy,
        progressiveSkillSnapshot: LocalAgentProgressiveSkillSnapshot? = nil,
        checkpoint: AgentRunCheckpoint,
        events: [AgentRunEvent] = [],
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) throws {
        self.id = id
        self.context = context
        self.modelConfigID = modelConfigID
        self.policy = policy
        self.progressiveSkillSnapshot = progressiveSkillSnapshot
        self.checkpoint = checkpoint
        self.events = events
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
        try validate()
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(modelConfigID, field: "modelConfigID")
        try policy.validate()
        guard checkpoint.id == id,
              checkpoint.scope == Self.runtimeScope(for: context),
              createdAtUnixMs >= 0,
              updatedAtUnixMs >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("run")
        }
    }

    public static func runtimeScope(for context: LocalAgentChatRunContext) -> String {
        "account:\(context.ownerUserID):project:\(context.projectID):room:\(context.roomID):agent:\(context.agentID):delivery:\(context.deliveryID)"
    }
}

/// Lightweight task-board projection produced directly by SQLite. The task board must not decode
/// every historical Run (including model messages and tool payloads) just to display four fields.
public struct LocalAgentTodoRunSummary: Sendable, Equatable, Identifiable {
    public var id: UUID { runID }
    public let todoID: String
    public let runID: UUID
    public let status: AgentRunCheckpoint.Status
    public let receiptCount: Int
    public let committedPaths: [String]
    public let updatedAtUnixMs: Int64

    public init(
        todoID: String,
        runID: UUID,
        status: AgentRunCheckpoint.Status,
        receiptCount: Int,
        committedPaths: [String],
        updatedAtUnixMs: Int64
    ) {
        self.todoID = todoID
        self.runID = runID
        self.status = status
        self.receiptCount = receiptCount
        self.committedPaths = committedPaths
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

/// Lightweight history-row projection produced directly by SQLite. The run-history list only
/// needs presentation metadata; model messages, tool receipts and event payloads are decoded when
/// the Human opens one Run in the inspector.
public struct LocalAgentRunHistorySummary: Sendable, Equatable, Identifiable {
    public let id: UUID
    public let agentID: String
    public let deliveryID: String
    public let projectID: String
    public let roomID: String
    public let triggerMessageID: String
    public let lane: LocalAgentRunLane
    public let triggerKind: ProjectAgentDeliveryTriggerKind
    public let status: AgentRunCheckpoint.Status
    public let eventCount: Int
    public let modelCalls: Int
    public let memoryThreadID: String?
    public let stopReason: String?
    public let diagnosticReason: String?
    public let elapsedSeconds: Double
    public let hasResult: Bool
    public let updatedAtUnixMs: Int64

    public init(
        id: UUID,
        agentID: String,
        deliveryID: String,
        projectID: String,
        roomID: String,
        triggerMessageID: String,
        lane: LocalAgentRunLane,
        triggerKind: ProjectAgentDeliveryTriggerKind,
        status: AgentRunCheckpoint.Status,
        eventCount: Int,
        modelCalls: Int,
        memoryThreadID: String?,
        stopReason: String?,
        diagnosticReason: String?,
        elapsedSeconds: Double,
        hasResult: Bool,
        updatedAtUnixMs: Int64
    ) {
        self.id = id
        self.agentID = agentID
        self.deliveryID = deliveryID
        self.projectID = projectID
        self.roomID = roomID
        self.triggerMessageID = triggerMessageID
        self.lane = lane
        self.triggerKind = triggerKind
        self.status = status
        self.eventCount = eventCount
        self.modelCalls = modelCalls
        self.memoryThreadID = memoryThreadID
        self.stopReason = stopReason
        self.diagnosticReason = diagnosticReason
        self.elapsedSeconds = elapsedSeconds
        self.hasResult = hasResult
        self.updatedAtUnixMs = updatedAtUnixMs
    }

    public init(run: LocalAgentGroupChatRun, triggerKind: ProjectAgentDeliveryTriggerKind) {
        self.init(
            id: run.id,
            agentID: run.context.agentID,
            deliveryID: run.context.deliveryID,
            projectID: run.context.projectID,
            roomID: run.context.roomID,
            triggerMessageID: run.context.triggerMessageID,
            lane: run.context.lane,
            triggerKind: triggerKind,
            status: run.checkpoint.status,
            eventCount: run.events.count,
            modelCalls: run.checkpoint.modelCalls,
            memoryThreadID: run.checkpoint.memory?.scope.threadID,
            stopReason: run.checkpoint.stopReason,
            diagnosticReason: run.events.last(where: {
                $0.kind == "needs_review" || $0.kind == "resume_failed"
            })?.detail,
            elapsedSeconds: run.checkpoint.elapsedSeconds,
            hasResult: [run.checkpoint.result, run.checkpoint.completionResult]
                .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
                .contains(where: { !$0.isEmpty }),
            updatedAtUnixMs: run.updatedAtUnixMs
        )
    }
}

public protocol LocalAgentGroupChatRunStoring: Sendable {
    func saveRun(_ run: LocalAgentGroupChatRun) async throws
    func run(ownerUserID: String, deliveryID: String) async throws -> LocalAgentGroupChatRun?
    func listInterruptedRuns(
        ownerUserID: String,
        limit: Int
    ) async throws -> [LocalAgentGroupChatRun]
    func listUnfinishedRuns(
        ownerUserID: String,
        projectID: String,
        limit: Int
    ) async throws -> [LocalAgentGroupChatRun]
}

actor LocalAgentGroupChatRunSession {
    private var run: LocalAgentGroupChatRun
    private let store: any LocalAgentGroupChatRunStoring
    private let now: @Sendable () -> Int64
    private let didPersist: @Sendable (LocalAgentGroupChatRun) async -> Void

    init(
        run: LocalAgentGroupChatRun,
        store: any LocalAgentGroupChatRunStoring,
        now: @escaping @Sendable () -> Int64,
        didPersist: @escaping @Sendable (LocalAgentGroupChatRun) async -> Void = { _ in }
    ) {
        self.run = run
        self.store = store
        self.now = now
        self.didPersist = didPersist
    }

    func record(checkpoint: AgentRunCheckpoint, event: AgentRunEvent) async throws {
        guard checkpoint.id == run.id,
              checkpoint.scope == run.checkpoint.scope else {
            throw AgentGroupChatError.conflict
        }
        run.checkpoint = checkpoint
        run.events.append(event)
        run.updatedAtUnixMs = max(now(), run.updatedAtUnixMs)
        try await store.saveRun(run)
        await didPersist(run)
    }

    func finish(checkpoint: AgentRunCheckpoint) async throws -> LocalAgentGroupChatRun {
        guard checkpoint.id == run.id,
              checkpoint.scope == run.checkpoint.scope else {
            throw AgentGroupChatError.conflict
        }
        run.checkpoint = checkpoint
        run.updatedAtUnixMs = max(now(), run.updatedAtUnixMs)
        try await store.saveRun(run)
        await didPersist(run)
        return run
    }
}
