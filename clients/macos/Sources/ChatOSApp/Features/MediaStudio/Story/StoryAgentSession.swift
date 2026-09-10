import ChatOSAgentRuntime
import Foundation

/// A single planning transaction. Draft mutations and domain receipts are stored together.
/// It has no access to media submission APIs, shell tools, or an arbitrary project/account.
actor StoryAgentSession {
    private var state: StoryAgentRun
    private let store: StoryProjectStore
    private let publish: @Sendable (StoryAgentRun) async -> Void

    init(run: StoryAgentRun, store: StoryProjectStore, publish: @escaping @Sendable (StoryAgentRun) async -> Void) {
        self.state = run; self.store = store; self.publish = publish
    }

    func prepareForResume(policy: AgentRunPolicy) async throws -> StoryAgentRun {
        try policy.validate()
        let definitions = try StoryAgentTools.definitions(stage: state.stage)
        if let id = state.checkpoint.inFlightCallID {
            guard let call = state.checkpoint.pendingCalls.first(where: { $0.id == id }),
                  definitions.contains(where: { $0.name == call.name }) else { throw StoryAgentError.invalidRun }
            if let saved = state.toolReceipts[id] {
                guard saved.name == call.name, saved.arguments == call.arguments else { throw StoryAgentError.invalidRun }
                state.checkpoint.receipts[id] = saved.outcome
            }
            // This session only mutates its atomic local draft. With no saved domain receipt,
            // the mutation never committed; replaying this local-only transaction is safe.
            state.checkpoint.inFlightCallID = nil
        }
        state.policy = policy
        state.checkpoint.noProgressRounds = 0
        try await persist()
        return state
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        try Task.checkCancellation()
        if let saved = state.toolReceipts[call.id] {
            guard saved.name == call.name, saved.arguments == call.arguments else { throw StoryAgentError.invalidRun }
            return saved.outcome
        }
        guard let definition = try StoryAgentTools.definitions(stage: state.stage).first(where: { $0.name == call.name }) else {
            return .failure(StoryAgentError.wrongStage.localizedDescription)
        }
        var next = state
        let outcome: AgentToolOutcome
        do {
            try AgentSchemaValidator.validate(arguments: call.arguments, schema: definition.schema)
            outcome = try StoryAgentTools.execute(call, run: &next)
        } catch { return .failure(error.localizedDescription) }
        next.toolReceipts[call.id] = .init(name: call.name, arguments: call.arguments, outcome: outcome)
        next.updatedAt = Date()
        // If this write fails, no subsequent tool may run. Do not return a success receipt.
        try await store.saveRun(next, owner: next.owner)
        state = next
        await publish(state)
        return outcome
    }

    func record(_ checkpoint: AgentRunCheckpoint, event: AgentRunEvent) async throws {
        guard checkpoint.id == state.id, checkpoint.scope == state.checkpoint.scope else { throw StoryAgentError.invalidRun }
        state.checkpoint = checkpoint
        state.events.append(event)
        try await persist()
    }

    func finish(_ checkpoint: AgentRunCheckpoint) async throws -> StoryAgentRun {
        guard checkpoint.id == state.id else { throw StoryAgentError.invalidRun }
        state.checkpoint = checkpoint
        try await persist()
        return state
    }

    func abort(_ reason: String) async throws {
        if state.checkpoint.status != .completed { state.checkpoint.status = .paused }
        state.checkpoint.stopReason = reason
        state.events.append(.init(kind: "story_paused", detail: reason, modelCalls: state.checkpoint.modelCalls))
        try await persist()
    }

    private func persist() async throws {
        state.updatedAt = Date()
        try await store.saveRun(state, owner: state.owner)
        await publish(state)
    }
}
