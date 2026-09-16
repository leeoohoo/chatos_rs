import CryptoKit
import Foundation

/// Identifiers are chosen by the application, never by an AI tool argument.
public struct AgentMemoryScope: Codable, Equatable, Sendable {
    public let tenantID: String
    public let sourceID: String
    public let threadID: String
    public let subjectID: String
    public let runID: UUID
    public let runtimeScope: String
    /// `nil` preserves the original story behavior (subject recall enabled).
    /// Approval runs are isolated so prior approval content cannot influence a
    /// later security decision, while their complete records are still stored.
    public let includeSubjectMemory: Bool?

    public init(tenantID: String, profile: String, projectID: UUID, runID: UUID, runtimeScope: String) throws {
        try self.init(
            tenantID: tenantID, profile: profile, projectID: projectID.uuidString,
            runID: runID, runtimeScope: runtimeScope
        )
    }

    public init(tenantID: String, profile: String, projectID: String, runID: UUID, runtimeScope: String) throws {
        guard !tenantID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !projectID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              ["approval", "story"].contains(profile), !runtimeScope.isEmpty else { throw AgentRuntimeError.scopeMismatch }
        self.tenantID = tenantID
        self.sourceID = "chatos"
        self.threadID = "client-agent:\(profile):\(projectID):\(runID)"
        self.subjectID = "client-agent:\(profile):\(projectID)"
        self.runID = runID
        self.runtimeScope = runtimeScope
        self.includeSubjectMemory = profile == "approval" ? false : nil
    }

    public func recordID(at index: Int) -> String { "client-agent:\(runID):message:\(index)" }
}

public struct AgentMemoryEntry: Sendable {
    public let id: String
    public let index: Int
    public let message: AgentMessage
    public let createdAt: Date
    public init(id: String, index: Int, message: AgentMessage, createdAt: Date) {
        self.id = id; self.index = index; self.message = message; self.createdAt = createdAt
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
    public init(blocks: [AgentMemoryContextBlock], recentRecords: [AgentMemoryContextRecord]) {
        self.blocks = blocks; self.recentRecords = recentRecords
    }
}

public protocol AgentMemoryServicing: Sendable {
    func ensureThread() async throws
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws
    func compose() async throws -> AgentMemoryContext
}

/// The full audit transcript stays in AgentRunCheckpoint.messages, not in each model request.
public struct AgentMemoryCheckpoint: Codable, Equatable, Sendable {
    public let scope: AgentMemoryScope
    public let recordEpoch: Date
    public let pinnedMessageCount: Int
    public var syncedMessageCount = 0
    public var syncedDigest: String = ""
    public var threadCreated = false
    public var syncInFlightEnd: Int?
    public init(scope: AgentMemoryScope, pinnedMessageCount: Int, recordEpoch: Date = Date()) {
        self.scope = scope; self.pinnedMessageCount = pinnedMessageCount; self.recordEpoch = recordEpoch
    }
}

public struct AgentContextPolicy: Codable, Equatable, Sendable {
    /// The first request in a run/resume must fit this local safety budget.
    /// Official OpenAI Responses owns subsequent in-run compaction.
    public var windowTokens = 250_000
    public var outputReserveTokens = 30_000
    public init() {}
    public var hardInputLimit: Int { windowTokens }
    public func validate() throws {
        guard (2_048...2_000_000).contains(windowTokens),
              (256..<windowTokens).contains(outputReserveTokens) else {
            throw AgentRuntimeError.invalidPolicy
        }
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
