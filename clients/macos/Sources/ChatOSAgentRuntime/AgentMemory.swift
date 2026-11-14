import CryptoKit
import Foundation

/// Identifiers are chosen by the application, never by an AI tool argument.
public struct AgentMemoryScope: Codable, Equatable, Sendable {
    public let tenantID: String
    public let sourceID: String
    public let threadID: String
    public let subjectID: String
    public let agentName: String?
    public let projectSubjectID: String?
    public let projectAggregationSubjectID: String?
    public let threadKind: String?
    public let runID: UUID
    public let runtimeScope: String
    /// Kept optional for checkpoint compatibility. All v2 Agent identities enable
    /// typed Agent Memory and, when project-bound, typed Project Summary recall.
    public let includeSubjectMemory: Bool?

    private static func isCanonicalAgentID(_ value: String) -> Bool {
        !["main-chat", "main_chat", "task-execution", "task_execution"].contains(value)
    }

    public init(tenantID: String, profile: String, projectID: UUID, runID: UUID, runtimeScope: String) throws {
        try self.init(
            tenantID: tenantID, profile: profile, projectID: projectID.uuidString,
            runID: runID, runtimeScope: runtimeScope
        )
    }

    public init(tenantID: String, profile: String, projectID: String?, runID: UUID, runtimeScope: String) throws {
        guard !tenantID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              ["approval", "story"].contains(profile), !runtimeScope.isEmpty else { throw AgentRuntimeError.scopeMismatch }
        let projectID = projectID?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard projectID?.rangeOfCharacter(from: .controlCharacters) == nil else {
            throw AgentRuntimeError.scopeMismatch
        }
        self.tenantID = tenantID
        self.sourceID = "chatos"
        let agentID = profile == "approval"
            ? "local_connector_command_approval_agent"
            : "builtin-\(profile)-agent"
        self.threadID = "client-agent:agent:\(agentID)"
        self.subjectID = "agent:\(agentID)"
        self.agentName = profile == "approval" ? "本地审批" : "剧情规划 Agent"
        self.projectSubjectID = projectID.flatMap { $0.isEmpty ? nil : "project:\($0)" }
        self.projectAggregationSubjectID = nil
        self.threadKind = profile == "approval" ? "approval_agent" : "story_agent"
        self.runID = runID
        self.runtimeScope = runtimeScope
        self.includeSubjectMemory = true
    }

    /// One user-scoped Agent keeps one raw Thread across conversations and projects. The current
    /// Project is recalled dynamically, and each record carries its Project identity so Memory
    /// Engine can feed the matching Project Summary without binding the Thread to one Project.
    public init(
        tenantID: String,
        agentID: String,
        agentName: String? = nil,
        projectID: String?,
        runID: UUID,
        runtimeScope: String
    ) throws {
        let identifiers = [tenantID, agentID]
        guard identifiers.allSatisfy({ value in
            !value.isEmpty
                && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
                && value.rangeOfCharacter(from: .controlCharacters) == nil
        }), projectID.map({ value in
            !value.isEmpty
                && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
                && value.rangeOfCharacter(from: .controlCharacters) == nil
        }) ?? true, Self.isCanonicalAgentID(agentID), agentName.map({ name in
            !name.isEmpty
                && name == name.trimmingCharacters(in: .whitespacesAndNewlines)
                && name.rangeOfCharacter(from: .controlCharacters) == nil
        }) ?? true, !runtimeScope.isEmpty else { throw AgentRuntimeError.scopeMismatch }
        self.tenantID = tenantID
        self.sourceID = "chatos"
        self.threadID = "client-agent:agent:\(agentID)"
        self.subjectID = "agent:\(agentID)"
        self.agentName = agentName
        self.projectSubjectID = projectID.map { "project:\($0)" }
        self.projectAggregationSubjectID = nil
        self.threadKind = "agent_conversation"
        self.runID = runID
        self.runtimeScope = runtimeScope
        self.includeSubjectMemory = true
    }

    /// A Todo keeps an isolated raw Thread across retries while recalling and contributing to
    /// the assignee Agent Memory and current Project Summary.
    public init(
        tenantID: String,
        todoID: String,
        agentID: String,
        agentName: String? = nil,
        projectID: String,
        runID: UUID,
        runtimeScope: String
    ) throws {
        let identifiers = [tenantID, todoID, agentID, projectID]
        guard identifiers.allSatisfy({ value in
            !value.isEmpty
                && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
                && value.rangeOfCharacter(from: .controlCharacters) == nil
        }), Self.isCanonicalAgentID(agentID), agentName.map({ name in
            !name.isEmpty
                && name == name.trimmingCharacters(in: .whitespacesAndNewlines)
                && name.rangeOfCharacter(from: .controlCharacters) == nil
        }) ?? true, !runtimeScope.isEmpty else { throw AgentRuntimeError.scopeMismatch }
        self.tenantID = tenantID
        self.sourceID = "chatos"
        self.threadID = "client-agent:todo:\(todoID)"
        self.subjectID = "agent:\(agentID)"
        self.agentName = agentName
        self.projectSubjectID = "project:\(projectID)"
        self.projectAggregationSubjectID = "project:\(projectID)"
        self.threadKind = "todo_execution"
        self.runID = runID
        self.runtimeScope = runtimeScope
        self.includeSubjectMemory = true
    }

    public func recordID(at index: Int) -> String { "client-agent:\(runID):message:\(index)" }

    /// Returns whether an immutable record can be written through a Memory service bound to this
    /// thread. Agent and Todo identities intentionally reuse one thread across separate runs, so
    /// their durable outbox may contain records created by an earlier run.
    public func acceptsRecord(id: String, index: Int) -> Bool {
        guard let location = recordLocation(for: id), location.index == index else { return false }
        return location.runID == runID || allowsCrossRunHistory
    }

    /// Agent and Todo threads are stable for their respective identity, so compose may return
    /// records written by earlier runs of that same Agent or Todo.
    public var allowsCrossRunHistory: Bool {
        threadID.hasPrefix("client-agent:agent:")
            || threadID.hasPrefix("client-agent:todo:")
    }

    /// Parses only record IDs emitted by `recordID(at:)`. Memory Engine has already verified the
    /// tenant/source/thread boundary; this additionally prevents arbitrary IDs inside that thread
    /// from being promoted into model context.
    public func recordLocation(for id: String) -> (runID: UUID, index: Int)? {
        let prefix = "client-agent:"
        let marker = ":message:"
        guard id.hasPrefix(prefix),
              let markerRange = id.range(of: marker, options: .backwards),
              markerRange.lowerBound > id.index(id.startIndex, offsetBy: prefix.count) else {
            return nil
        }
        let runText = String(id[id.index(id.startIndex, offsetBy: prefix.count)..<markerRange.lowerBound])
        let indexText = String(id[markerRange.upperBound...])
        guard let parsedRunID = UUID(uuidString: runText),
              parsedRunID.uuidString == runText,
              let index = Int(indexText), index >= 0,
              String(index) == indexText else {
            return nil
        }
        return (parsedRunID, index)
    }
}

public struct AgentMemoryToolExchange: Codable, Equatable, Sendable {
    public let assistantRecordID: String
    public let exchangeID: String
    public let callID: String
    public let resultOrdinal: Int
    public let resultCount: Int

    public init(
        assistantRecordID: String,
        exchangeID: String,
        callID: String,
        resultOrdinal: Int,
        resultCount: Int
    ) throws {
        guard !assistantRecordID.isEmpty, !exchangeID.isEmpty, !callID.isEmpty,
              resultOrdinal >= 0, resultCount > 0, resultOrdinal < resultCount else {
            throw AgentContextError.invalidHistory
        }
        self.assistantRecordID = assistantRecordID
        self.exchangeID = exchangeID
        self.callID = callID
        self.resultOrdinal = resultOrdinal
        self.resultCount = resultCount
    }
}

public struct AgentMemoryEntry: Sendable {
    public let id: String
    public let index: Int
    public let message: AgentMessage
    public let createdAt: Date
    public let conversationTurnID: String
    public let toolExchange: AgentMemoryToolExchange?
    public init(
        id: String,
        index: Int,
        message: AgentMessage,
        createdAt: Date,
        conversationTurnID: String,
        toolExchange: AgentMemoryToolExchange? = nil
    ) {
        self.id = id; self.index = index; self.message = message; self.createdAt = createdAt
        self.conversationTurnID = conversationTurnID
        self.toolExchange = toolExchange
    }
}

public struct AgentModelCapabilitySnapshot: Codable, Equatable, Sendable {
    public let revision: String
    public let contextWindowTokens: Int
    public let outputReserveTokens: Int
    public let protocolOverheadEstimatorID: String
    public let tokenizerID: String?

    public init(revision: String, contextWindowTokens: Int, outputReserveTokens: Int,
                protocolOverheadEstimatorID: String, tokenizerID: String?) throws {
        guard !revision.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !protocolOverheadEstimatorID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              contextWindowTokens > 0,
              outputReserveTokens > 0,
              outputReserveTokens < contextWindowTokens else {
            throw AgentRuntimeError.modelConfigurationUnavailable
        }
        self.revision = revision
        self.contextWindowTokens = contextWindowTokens
        self.outputReserveTokens = outputReserveTokens
        self.protocolOverheadEstimatorID = protocolOverheadEstimatorID
        self.tokenizerID = tokenizerID
    }
}

public struct AgentMemoryComposeRequest: Equatable, Sendable {
    public let modelCapability: AgentModelCapabilitySnapshot
    public let queryText: String
    public let currentTurnID: String
    public let availableMemoryTokens: Int

    public init(modelCapability: AgentModelCapabilitySnapshot, queryText: String,
                currentTurnID: String, availableMemoryTokens: Int) throws {
        guard !queryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !currentTurnID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw AgentContextError.invalidHistory
        }
        guard availableMemoryTokens > 0,
              availableMemoryTokens <= modelCapability.contextWindowTokens - modelCapability.outputReserveTokens else {
            throw AgentContextError.budgetExceeded
        }
        self.availableMemoryTokens = availableMemoryTokens
        self.modelCapability = modelCapability
        self.queryText = queryText
        self.currentTurnID = currentTurnID
    }
}

public struct AgentMemoryContextBlock: Equatable, Sendable {
    public let blockType: String
    public let text: String
    public init(blockType: String, text: String) { self.blockType = blockType; self.text = text }
}

public struct AgentMemoryContextRecord: Equatable, Sendable {
    public let id: String
    public let message: AgentMessage
    public init(id: String, message: AgentMessage) { self.id = id; self.message = message }
}

/// The native representation of Memory Engine's `ComposeContextResponse`.
/// Keep both blocks and records: callers must use the composed response rather than rebuilding
/// a different context from a list of record IDs.
public struct AgentMemoryContext: Sendable {
    public let blocks: [AgentMemoryContextBlock]
    public let recentRecords: [AgentMemoryContextRecord]
    /// Memory Engine sets this when it omitted older raw records from the already budgeted
    /// projection. Callers may use this projection immediately and start compaction in background.
    public let needsCompaction: Bool
    public let budgetUsedTokens: Int?
    public let pendingRecordCount: Int?
    public init(
        blocks: [AgentMemoryContextBlock],
        recentRecords: [AgentMemoryContextRecord],
        needsCompaction: Bool = false,
        budgetUsedTokens: Int? = nil,
        pendingRecordCount: Int? = nil
    ) {
        self.blocks = blocks
        self.recentRecords = recentRecords
        self.needsCompaction = needsCompaction
        self.budgetUsedTokens = budgetUsedTokens
        self.pendingRecordCount = pendingRecordCount
    }
}

public protocol AgentMemoryServicing: Sendable {
    func ensureThread() async throws
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws
    func compose(_ request: AgentMemoryComposeRequest) async throws -> AgentMemoryContext
    /// Starts the active summary requested by a compose response with `needsCompaction=true`.
    /// The server owns the background job after accepting it.
    func compactSummary() async throws
    func flushSummary() async throws
}

public extension AgentMemoryServicing {
    func compactSummary() async throws { try await flushSummary() }
    func flushSummary() async throws {}
}

/// The full audit transcript stays in AgentRunCheckpoint.messages, not in each model request.
public struct AgentMemoryCheckpoint: Codable, Equatable, Sendable {
    public let scope: AgentMemoryScope
    public let recordEpoch: Date
    public let pinnedMessageCount: Int
    /// Frozen before the first model call. Optional only so legacy checkpoints remain decodable;
    /// a legacy checkpoint that already called a model is rejected instead of adopting new limits.
    public var modelCapability: AgentModelCapabilitySnapshot?
    public var syncedMessageCount = 0
    public var syncedDigest: String = ""
    public var threadCreated = false
    public var syncInFlightEnd: Int?
    public init(
        scope: AgentMemoryScope,
        pinnedMessageCount: Int,
        recordEpoch: Date = Date(),
        modelCapability: AgentModelCapabilitySnapshot? = nil
    ) {
        self.scope = scope
        self.pinnedMessageCount = pinnedMessageCount
        self.recordEpoch = recordEpoch
        self.modelCapability = modelCapability
    }
}

public enum AgentContextError: LocalizedError, Sendable {
    case unavailable, invalidHistory, syncUncertain, budgetExceeded
    public var errorDescription: String? {
        switch self {
        case .unavailable: "Memory Engine 同步或上下文服务不可用，运行已暂停。"
        case .invalidHistory: "运行历史或记忆范围不一致，已暂停，不能丢弃记录或重放工具。"
        case .syncUncertain: "部分运行记录尚未确认写入，已暂停。请稍后恢复核对；不会自动重发并重置已有摘要。"
        case .budgetExceeded: "上下文仍超过设置的窗口预算，已暂停，未继续调用模型。"
        }
    }
}

public struct AgentMemoryCompactionFailure: LocalizedError, Sendable {
    public let reason: String

    public init(reason: String) {
        self.reason = reason
    }

    public var errorDescription: String? {
        "Memory Engine 历史压缩暂时失败：\(reason)"
    }
}

public enum AgentContextBudget {
    /// Mirrors `chatos_ai_runtime::estimated_json_tokens`: serialize the complete model-input
    /// payload and use four JSON bytes per estimated token. This remains an estimate, but the
    /// returned unit is tokens rather than raw UTF-8 bytes.
    public static func estimate(messages: [AgentMessage], tools: [AgentToolDefinition]) throws -> Int {
        let messagePayload = messages.map { message -> [String: Any] in
            var value: [String: Any] = ["role": message.role.rawValue, "content": message.content]
            if let toolCallID = message.toolCallID { value["tool_call_id"] = toolCallID }
            if !message.toolCalls.isEmpty {
                value["tool_calls"] = message.toolCalls.map { call in
                    ["id": call.id, "type": "function", "function": ["name": call.name, "arguments": call.arguments]]
                }
            }
            return value
        }
        let toolPayload = try tools.map { tool -> [String: Any] in
            ["type": "function", "function": [
                "name": tool.name,
                "description": tool.description,
                "parameters": try JSONSerialization.jsonObject(with: tool.schema),
            ]]
        }
        let payload: [String: Any] = ["messages": messagePayload, "tools": toolPayload]
        let bytes = try JSONSerialization.data(withJSONObject: payload).count
        return max(1, (bytes + 3) / 4)
    }

    static func digest(_ messages: ArraySlice<AgentMessage>) throws -> String {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
        return SHA256.hash(data: try encoder.encode(Array(messages))).map { String(format: "%02x", $0) }.joined()
    }
}
