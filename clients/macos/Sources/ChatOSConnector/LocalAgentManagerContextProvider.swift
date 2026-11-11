import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// The shared tool-observation boundary, including Worker callbacks, schema rejections and
/// tool_result_read. The raw tool receipt is already durable before this read-only snapshot.
struct LocalAgentManagerContextProvider: AgentContextProviding {
    let base: any AgentContextProviding
    let store: SQLiteAgentGroupChatStore
    let context: LocalAgentChatRunContext
    let inboxSession: LocalAgentManagerInboxSession

    var scope: AgentMemoryScope { base.scope }

    func prepare(
        checkpoint initial: AgentRunCheckpoint,
        tools: [AgentToolDefinition],
        policy: AgentContextPolicy,
        modelCapability: AgentModelCapabilitySnapshot,
        deadline: Date,
        synchronizeOnly: Bool,
        shouldPause: @escaping @Sendable () async -> Bool,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentPreparedContext {
        var checkpoint = initial
        if checkpoint.messages.last?.role == .tool {
            let receipt = await inboxSession.completionReceipt()
            let state: String
            do {
                let hasNew = try await store.hasNewInboxMessages(
                    ownerUserID: context.ownerUserID, agentID: context.agentID,
                    readThrough: receipt.readThrough
                )
                state = #"{"inbox":{"status":"available","has_new_messages":\#(hasNew),"read_tool":"chat_read_all_unread"}}"#
            } catch {
                // A failed status read must never turn a completed write into an uncertain write
                // or falsely tell the model that the inbox is empty.
                state = #"{"inbox":{"status":"unavailable","has_new_messages":null,"read_tool":"chat_read_all_unread"}}"#
            }
            checkpoint.messages.append(.init(role: .system, content: """
            \(AgentRuntime.runtimeStateMarker)
            \(state)
            Latest Agent-wide inbox snapshot. If true or unavailable, call chat_read_all_unread before continuing. Reading does not confirm processing.
            """))
        }
        return try await base.prepare(
            checkpoint: checkpoint, tools: tools, policy: policy, modelCapability: modelCapability,
            deadline: deadline, synchronizeOnly: synchronizeOnly, shouldPause: shouldPause, record: record
        )
    }

    func finalizeMemory() async throws { try await base.finalizeMemory() }
}
