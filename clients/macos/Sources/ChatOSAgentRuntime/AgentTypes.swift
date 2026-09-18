import Foundation

public struct AgentToolCall: Codable, Equatable, Sendable {
    public var id: String
    public var name: String
    public var arguments: String
    public init(id: String, name: String, arguments: String) { self.id = id; self.name = name; self.arguments = arguments }
}

public struct AgentMessageAttachment: Codable, Equatable, Sendable {
    public enum Kind: String, Codable, Sendable {
        case image
        case file
        case audio
    }

    public var name: String
    public var mimeType: String
    public var kind: Kind
    public var localFileURL: URL

    public init(name: String, mimeType: String, kind: Kind, localFileURL: URL) {
        self.name = name
        self.mimeType = mimeType
        self.kind = kind
        self.localFileURL = localFileURL
    }
}

public struct AgentMessage: Codable, Equatable, Sendable {
    public enum Role: String, Codable, Sendable { case system, user, assistant, tool }
    public var role: Role
    public var content: String
    public var toolCalls: [AgentToolCall]
    public var toolCallID: String?
    /// Exact JSON-encoded Responses `response.output` array. Persisting this
    /// preserves encrypted reasoning and compaction items across stateless calls.
    public var responseOutputJSON: Data?
    public var usage: AgentUsage?
    /// Local, program-owned input files. Model transports resolve these URLs to data payloads;
    /// the filesystem path itself is never sent to the model provider.
    public var attachments: [AgentMessageAttachment]?
    public init(
        role: Role,
        content: String = "",
        toolCalls: [AgentToolCall] = [],
        toolCallID: String? = nil,
        attachments: [AgentMessageAttachment] = []
    ) {
        self.role = role; self.content = content; self.toolCalls = toolCalls
        self.toolCallID = toolCallID; self.responseOutputJSON = nil; self.usage = nil
        self.attachments = attachments.isEmpty ? nil : attachments
    }
    public init(
        role: Role, content: String, toolCalls: [AgentToolCall],
        toolCallID: String? = nil, responseOutputJSON: Data?, usage: AgentUsage?,
        attachments: [AgentMessageAttachment] = []
    ) {
        self.role = role; self.content = content; self.toolCalls = toolCalls
        self.toolCallID = toolCallID; self.responseOutputJSON = responseOutputJSON; self.usage = usage
        self.attachments = attachments.isEmpty ? nil : attachments
    }

    public var attachmentItems: [AgentMessageAttachment] { attachments ?? [] }
}

public struct AgentUsage: Codable, Equatable, Sendable {
    public var inputTokens: Int
    public var cachedTokens: Int
    public var outputTokens: Int
    public var requests: Int

    public init(inputTokens: Int = 0, cachedTokens: Int = 0, outputTokens: Int = 0, requests: Int = 0) {
        self.inputTokens = inputTokens; self.cachedTokens = cachedTokens
        self.outputTokens = outputTokens; self.requests = requests
    }

    mutating func add(_ usage: AgentUsage?) {
        guard let usage else { return }
        inputTokens += usage.inputTokens
        cachedTokens += usage.cachedTokens
        outputTokens += usage.outputTokens
        requests += usage.requests
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

public struct AgentRunPolicy: Codable, Equatable, Sendable {
    public var maximumModelCalls = 600
    public var requestTimeoutSeconds = 180
    public var runTimeoutSeconds = 7_200
    public var maximumRequestRetries = 5
    public var maximumNoProgressRounds = 8
    public var context: AgentContextPolicy? = nil
    public init() {}
    public func validate() throws {
        guard (1...10_000).contains(maximumModelCalls), (5...1_800).contains(requestTimeoutSeconds),
              (10...86_400).contains(runTimeoutSeconds), (0...10).contains(maximumRequestRetries),
              (1...100).contains(maximumNoProgressRounds) else { throw AgentRuntimeError.invalidPolicy }
        try (context ?? AgentContextPolicy()).validate()
    }
}

public struct AgentRuntimePreferences: Codable, Equatable, Sendable {
    public var global = AgentRunPolicy()
    public var approvalMaximumCalls: Int?
    public var storyMaximumCalls: Int?
    public init() {}
    public enum Profile { case approval, story }
    public func effective(_ profile: Profile) -> AgentRunPolicy {
        var policy = global
        if let value = profile == .approval ? approvalMaximumCalls : storyMaximumCalls { policy.maximumModelCalls = value }
        return policy
    }
    public func validate() throws { try global.validate(); try effective(.approval).validate(); try effective(.story).validate() }
}

/// Device-local configuration, shared by native approval and story creation. No secrets.
public struct AgentSettingsStore: Sendable {
    private let suiteName: String?
    private let key = "chatos.agent-runtime.settings.v1"
    private let retryDefaultMigrationKey = "chatos.agent-runtime.retry-default.v2"
    public init(suiteName: String? = nil) { self.suiteName = suiteName }
    public func load() throws -> AgentRuntimePreferences {
        let defaults = suiteName.flatMap(UserDefaults.init(suiteName:)) ?? .standard
        guard let data = defaults.data(forKey: key) else {
            defaults.set(true, forKey: retryDefaultMigrationKey)
            return .init()
        }
        var value = try JSONDecoder().decode(AgentRuntimePreferences.self, from: data)
        // Version 1 originally persisted the old default (`2`) even when the user only changed
        // unrelated context settings. Migrate that legacy default once, while preserving every
        // non-default retry value the user may have selected explicitly.
        var migratedLegacyRetryDefault = false
        if !defaults.bool(forKey: retryDefaultMigrationKey) {
            if value.global.maximumRequestRetries == 2 {
                value.global.maximumRequestRetries = 5
                migratedLegacyRetryDefault = true
            }
        }
        try value.validate()
        if !defaults.bool(forKey: retryDefaultMigrationKey) {
            if migratedLegacyRetryDefault {
                defaults.set(try JSONEncoder().encode(value), forKey: key)
            }
            defaults.set(true, forKey: retryDefaultMigrationKey)
        }
        return value
    }
    public func save(_ value: AgentRuntimePreferences) throws {
        try value.validate()
        let defaults = suiteName.flatMap(UserDefaults.init(suiteName:)) ?? .standard
        defaults.set(try JSONEncoder().encode(value), forKey: key)
    }
}

/// Program-owned instruction identity persisted alongside the exact prompt text in messages.
/// Optional checkpoint storage keeps runs created before this field fully decodable.
public struct AgentInstructionBundleCheckpoint: Codable, Equatable, Sendable {
    public var name: String
    public var version: Int
    public var contentSHA256: String
    public var language: String
    public var audience: String

    public init(
        name: String,
        version: Int,
        contentSHA256: String,
        language: String,
        audience: String
    ) {
        self.name = name
        self.version = version
        self.contentSHA256 = contentSHA256
        self.language = language
        self.audience = audience
    }
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
    public var usage: AgentUsage?
    public var instructionBundles: [AgentInstructionBundleCheckpoint]?
    public init(scope: String, messages: [AgentMessage]) { self.scope = scope; self.messages = messages }

    public var instructionBundleItems: [AgentInstructionBundleCheckpoint] {
        instructionBundles ?? []
    }
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
    var usesServerSideCompaction: Bool { get }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage
    func stream(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval,
                onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void) async throws -> AgentMessage
}

public extension AgentModelClient {
    var usesServerSideCompaction: Bool { false }

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
    case invalidPolicy, invalidResponse, invalidAttachment, contextTooLarge, contextOverflow, timeout, scopeMismatch
    case provider(Int)
    case providerDetail(Int, String)
    public var errorDescription: String? {
        switch self {
        case .invalidPolicy: "Agent 运行设置无效，请检查设置中的范围。"
        case .invalidResponse: "模型没有返回有效且完整的工具调用。"
        case .invalidAttachment: "Agent 消息附件不可读取或超过大小限制。"
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
