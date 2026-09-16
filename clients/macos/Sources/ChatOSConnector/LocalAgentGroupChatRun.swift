import ChatOSAgentRuntime
import ChatOSCore
import Foundation

public struct LocalAgentGroupChatRun: Codable, Sendable, Equatable, Identifiable {
    public let id: UUID
    public let context: LocalAgentChatRunContext
    public let modelConfigID: String
    public let policy: AgentRunPolicy
    public var checkpoint: AgentRunCheckpoint
    public var events: [AgentRunEvent]
    public let createdAtUnixMs: Int64
    public var updatedAtUnixMs: Int64

    public init(
        id: UUID,
        context: LocalAgentChatRunContext,
        modelConfigID: String,
        policy: AgentRunPolicy,
        checkpoint: AgentRunCheckpoint,
        events: [AgentRunEvent] = [],
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) throws {
        self.id = id
        self.context = context
        self.modelConfigID = modelConfigID
        self.policy = policy
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

public protocol LocalAgentGroupChatRunStoring: Sendable {
    func saveRun(_ run: LocalAgentGroupChatRun) async throws
    func run(ownerUserID: String, deliveryID: String) async throws -> LocalAgentGroupChatRun?
}

actor LocalAgentGroupChatRunSession {
    private var run: LocalAgentGroupChatRun
    private let store: any LocalAgentGroupChatRunStoring
    private let now: @Sendable () -> Int64

    init(
        run: LocalAgentGroupChatRun,
        store: any LocalAgentGroupChatRunStoring,
        now: @escaping @Sendable () -> Int64
    ) {
        self.run = run
        self.store = store
        self.now = now
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
    }

    func finish(checkpoint: AgentRunCheckpoint) async throws -> LocalAgentGroupChatRun {
        guard checkpoint.id == run.id,
              checkpoint.scope == run.checkpoint.scope else {
            throw AgentGroupChatError.conflict
        }
        run.checkpoint = checkpoint
        run.updatedAtUnixMs = max(now(), run.updatedAtUnixMs)
        try await store.saveRun(run)
        return run
    }
}
