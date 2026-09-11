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

    public init(tenantID: String, profile: String, projectID: UUID, runID: UUID, runtimeScope: String) throws {
        guard !tenantID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              ["approval", "story"].contains(profile), !runtimeScope.isEmpty else { throw AgentRuntimeError.scopeMismatch }
        self.tenantID = tenantID
        self.sourceID = "chatos"
        self.threadID = "client-agent:\(profile):\(projectID):\(runID)"
        self.subjectID = "client-agent:\(profile):\(projectID)"
        self.runID = runID
        self.runtimeScope = runtimeScope
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

public struct AgentSummaryStatus: Sendable {
    public var jobID: String?
    public var accepted: Bool
    public var running: Bool
    public var completed: Bool
    public var failed: Bool
    public var generated: Bool
    public var compacted: Bool
    public var errorMessage: String?
    public init(jobID: String? = nil, accepted: Bool = false, running: Bool = false,
                completed: Bool = false, failed: Bool = false, generated: Bool = false,
                compacted: Bool = false, errorMessage: String? = nil) {
        self.jobID = jobID; self.accepted = accepted; self.running = running
        self.completed = completed; self.failed = failed; self.generated = generated
        self.compacted = compacted; self.errorMessage = errorMessage
    }

    public var changedContext: Bool { generated || compacted }
}

public protocol AgentMemoryServicing: Sendable {
    func ensureThread() async throws
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws
    func compose() async throws -> AgentMemoryContext
    func startSummary(reason: String) async throws -> AgentSummaryStatus
    func summaryStatus(jobID: String?) async throws -> AgentSummaryStatus
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
    public var compactions = 0
    public init(scope: AgentMemoryScope, pinnedMessageCount: Int, recordEpoch: Date = Date()) {
        self.scope = scope; self.pinnedMessageCount = pinnedMessageCount; self.recordEpoch = recordEpoch
    }
}

public struct AgentContextPolicy: Codable, Equatable, Sendable {
    /// These defaults intentionally match `chatos_ai_runtime` so Task Runner and native
    /// Agents use the same soft/hard budget semantics. Users can override them for a model.
    public var windowTokens = 250_000
    public var outputReserveTokens = 30_000
    public var compactionThresholdTokens = 220_000
    public var maximumCompactionPasses = 8
    public var summaryTimeoutSeconds = 120
    public var summaryPollSeconds = 10
    public init() {}
    /// Same distinction as Task Runner: the reserve defines the proactive compaction point;
    /// the model window itself remains the hard failure limit.
    public var reservedInputLimit: Int { windowTokens - outputReserveTokens }
    public var hardInputLimit: Int { windowTokens }
    public func validate() throws {
        guard (2_048...2_000_000).contains(windowTokens), (256..<windowTokens).contains(outputReserveTokens),
              (512...reservedInputLimit).contains(compactionThresholdTokens),
              (1...16).contains(maximumCompactionPasses), (5...1_800).contains(summaryTimeoutSeconds),
              (1...30).contains(summaryPollSeconds) else { throw AgentRuntimeError.invalidPolicy }
    }
}

public enum AgentContextError: LocalizedError, Sendable {
    case unavailable, invalidHistory, syncUncertain, summaryFailed(String?), summaryTimedOut, noImprovement, budgetExceeded
    public var errorDescription: String? {
        switch self {
        case .unavailable: "Memory Engine 同步或上下文服务不可用，运行已暂停。"
        case .invalidHistory: "运行历史或记忆范围不一致，已暂停，不能丢弃记录或重放工具。"
        case .syncUncertain: "部分运行记录尚未确认写入，已暂停。请稍后恢复核对；不会自动重发并重置已有摘要。"
        case let .summaryFailed(detail):
            if let detail = detail?.trimmingCharacters(in: .whitespacesAndNewlines), !detail.isEmpty {
                "Memory Engine 摘要任务失败：\(detail)"
            } else {
                "Memory Engine 摘要任务失败，请检查摘要 Agent 模型与策略配置。"
            }
        case .summaryTimedOut: "等待上下文压缩超时，已保存摘要任务，可稍后继续。"
        case .noImprovement: "压缩没有缩小上下文，已暂停；请检查摘要配置或缩小单次输入。"
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
