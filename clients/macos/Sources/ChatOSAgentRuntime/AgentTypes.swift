import ChatOSCore
import Foundation

public struct AgentToolCall: Codable, Equatable, Sendable {
    public var id: String
    public var name: String
    public var arguments: String
    public init(id: String, name: String, arguments: String) { self.id = id; self.name = name; self.arguments = arguments }
}

public struct AgentMessage: Codable, Equatable, Sendable {
    public enum Role: String, Codable, Sendable { case system, user, assistant, tool }
    public var role: Role
    public var content: String
    public var toolCalls: [AgentToolCall]
    public var toolCallID: String?
    public init(role: Role, content: String = "", toolCalls: [AgentToolCall] = [], toolCallID: String? = nil) {
        self.role = role; self.content = content; self.toolCalls = toolCalls; self.toolCallID = toolCallID
    }
}

public struct AgentToolDefinition: Sendable {
    public enum Effect: String, Codable, Sendable { case readOnly, write, billable, terminal }
    public var name: String
    public var description: String
    public var schema: Data
    public var effect: Effect
    public init(name: String, description: String, schema: Data, effect: Effect = .readOnly) {
        self.name = name; self.description = description; self.schema = schema; self.effect = effect
    }
}

public struct AgentToolOutcome: Codable, Equatable, Sendable {
    public var content: String
    public var madeProgress: Bool
    public var isError: Bool
    public init(_ content: String, madeProgress: Bool = true, isError: Bool = false) {
        self.content = content; self.madeProgress = madeProgress; self.isError = isError
    }
    public static func failure(_ message: String) -> Self { .init(message, madeProgress: false, isError: true) }
}

public struct AgentRunCheckpoint: Codable, Equatable, Sendable {
    public enum Status: String, Codable, Sendable { case ready, running, paused, completed, failed, needsReview, limitReached }
    public var id = UUID()
    public var scope: String
    public var messages: [AgentMessage]
    public var pendingCalls: [AgentToolCall] = []
    public var inFlightCallID: String?
    public var receipts: [String: AgentToolOutcome] = [:]
    public var callFingerprints: [String: String] = [:]
    public var observations: [String: String] = [:]
    public var modelCalls = 0
    public var noProgressRounds = 0
    public var elapsedSeconds: Double = 0
    public var status: Status = .ready
    public var result: String?
    public var completionResult: String?
    public var stopReason: String?
    public var memory: AgentMemoryCheckpoint?
    public init(scope: String, messages: [AgentMessage]) { self.scope = scope; self.messages = messages }
}

public struct AgentRunEvent: Codable, Identifiable, Equatable, Sendable {
    public var id = UUID()
    public var date = Date()
    public var kind: String
    public var detail: String
    public var modelCalls: Int
    public init(kind: String, detail: String, modelCalls: Int) { self.kind = kind; self.detail = detail; self.modelCalls = modelCalls }
}

/// Transient transport progress. Only the fully validated message returned by `stream` may be
/// written to a checkpoint or used to execute tools.
public enum AgentModelStreamEvent: Equatable, Sendable {
    case responseCreated
    case textDelta(String)
    case toolCallDelta(index: Int, id: String?, name: String?, argumentsDelta: String)
    case completed
}

public protocol AgentModelClient: Sendable {
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage
    func stream(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
                onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void) async throws -> AgentMessage
}

public extension AgentModelClient {
    func stream(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
                onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void) async throws -> AgentMessage {
        await onEvent(.responseCreated)
        let message = try await complete(messages: messages, tools: tools, timeout: timeout)
        if !message.content.isEmpty { await onEvent(.textDelta(message.content)) }
        for (index, call) in message.toolCalls.enumerated() {
            await onEvent(.toolCallDelta(index: index, id: call.id, name: call.name,
                                         argumentsDelta: call.arguments))
        }
        await onEvent(.completed)
        return message
    }
}

public enum AgentRuntimeError: LocalizedError, Sendable {
    case invalidPolicy, invalidResponse, contextTooLarge, contextOverflow, timeout, scopeMismatch
    case provider(Int)
    case providerDetail(Int, String)
    public var errorDescription: String? {
        switch self {
        case .invalidPolicy: "Agent 运行设置无效，请检查设置中的范围。"
        case .invalidResponse: "模型没有返回有效且完整的工具调用。"
        case .contextTooLarge: "Agent 上下文超过安全大小限制，请从已保存的业务检查点开始新一轮运行。"
        case .contextOverflow: "模型报告上下文超出窗口，需要压缩后才能继续。"
        case .timeout: "Agent 已达到设置中的超时时限。"
        case .scopeMismatch: "运行记录不属于当前账户、项目或业务版本。"
        case .provider(let code): "模型请求失败（HTTP \(code)）。"
        case .providerDetail(let code, let detail): "模型请求失败（HTTP \(code)）：\(detail)"
        }
    }
    public static func isTransient(_ error: Error) -> Bool {
        if case .provider(let code) = error as? AgentRuntimeError { return code == 408 || code == 429 || code >= 500 }
        if case .providerDetail(let code, _) = error as? AgentRuntimeError { return code == 408 || code == 429 || code >= 500 }
        guard let error = error as? URLError else { return false }
        return [.timedOut, .networkConnectionLost, .cannotConnectToHost, .notConnectedToInternet].contains(error.code)
    }
}
