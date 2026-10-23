import ChatOSAgentRuntime
import Foundation

public enum ChatOSAgentWorkerProtocol {
    public static let minimumVersion = 1
    public static let maximumVersion = 1
    public static let maximumInlineBytes = 64 * 1_024 * 1_024
}

public struct AgentWorkerHelloRequest: Codable, Equatable, Sendable {
    public let protocolMinVersion: Int
    public let protocolMaxVersion: Int
    public let parentProcessID: Int32
    public let runID: UUID
    public let nonce: UUID

    public init(parentProcessID: Int32, runID: UUID, nonce: UUID) {
        self.protocolMinVersion = ChatOSAgentWorkerProtocol.minimumVersion
        self.protocolMaxVersion = ChatOSAgentWorkerProtocol.maximumVersion
        self.parentProcessID = parentProcessID
        self.runID = runID
        self.nonce = nonce
    }
}

public struct AgentWorkerHelloResponse: Codable, Equatable, Sendable {
    public let selectedProtocolVersion: Int
    public let workerInstanceID: UUID
    public let runID: UUID
    public let nonce: UUID

    public init(
        selectedProtocolVersion: Int,
        workerInstanceID: UUID,
        runID: UUID,
        nonce: UUID
    ) {
        self.selectedProtocolVersion = selectedProtocolVersion
        self.workerInstanceID = workerInstanceID
        self.runID = runID
        self.nonce = nonce
    }
}

public struct AgentWorkerStartRequest: Codable, Equatable, Sendable {
    public let checkpoint: AgentRunCheckpoint
    public let scope: String
    public let policy: AgentRunPolicy
    public let tools: [AgentToolDefinition]
    public let hasContextProvider: Bool
    public let modelUsesServerSideCompaction: Bool

    public init(
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        tools: [AgentToolDefinition],
        hasContextProvider: Bool,
        modelUsesServerSideCompaction: Bool
    ) {
        self.checkpoint = checkpoint
        self.scope = scope
        self.policy = policy
        self.tools = tools
        self.hasContextProvider = hasContextProvider
        self.modelUsesServerSideCompaction = modelUsesServerSideCompaction
    }
}

public struct AgentWorkerModelRequest: Codable, Equatable, Sendable {
    public let id: UUID
    public let messages: [AgentMessage]
    public let tools: [AgentToolDefinition]
    public let timeout: TimeInterval

    public init(id: UUID, messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) {
        self.id = id
        self.messages = messages
        self.tools = tools
        self.timeout = timeout
    }
}

public struct AgentWorkerModelEvent: Codable, Equatable, Sendable {
    public let requestID: UUID
    public let event: AgentModelStreamEvent
    public init(requestID: UUID, event: AgentModelStreamEvent) {
        self.requestID = requestID
        self.event = event
    }
}

public struct AgentWorkerRemoteFailure: Codable, Equatable, Sendable {
    public let detail: String
    public let transient: Bool
    public init(detail: String, transient: Bool = false) {
        self.detail = String(detail.prefix(4_096))
        self.transient = transient
    }
}

public struct AgentWorkerModelResponse: Codable, Equatable, Sendable {
    public let requestID: UUID
    public let message: AgentMessage?
    public let failure: AgentWorkerRemoteFailure?
    public init(requestID: UUID, message: AgentMessage?, failure: AgentWorkerRemoteFailure? = nil) {
        self.requestID = requestID
        self.message = message
        self.failure = failure
    }
}

public struct AgentWorkerToolRequest: Codable, Equatable, Sendable {
    public let id: UUID
    public let call: AgentToolCall
    public init(id: UUID, call: AgentToolCall) { self.id = id; self.call = call }
}

public struct AgentWorkerToolResponse: Codable, Equatable, Sendable {
    public let requestID: UUID
    public let outcome: AgentToolOutcome?
    public let failure: AgentWorkerRemoteFailure?
    public init(requestID: UUID, outcome: AgentToolOutcome?, failure: AgentWorkerRemoteFailure? = nil) {
        self.requestID = requestID; self.outcome = outcome; self.failure = failure
    }
}

public struct AgentWorkerCompletionRequest: Codable, Equatable, Sendable {
    public let id: UUID
    public init(id: UUID) { self.id = id }
}

public struct AgentWorkerCompletionResponse: Codable, Equatable, Sendable {
    public let requestID: UUID
    public let result: String?
    public let failure: AgentWorkerRemoteFailure?
    public init(requestID: UUID, result: String?, failure: AgentWorkerRemoteFailure? = nil) {
        self.requestID = requestID; self.result = result; self.failure = failure
    }
}

public struct AgentWorkerContextRequest: Codable, Equatable, Sendable {
    public let id: UUID
    public let checkpoint: AgentRunCheckpoint
    public let tools: [AgentToolDefinition]
    public let policy: AgentContextPolicy
    public let deadline: Date
    public let synchronizeOnly: Bool

    public init(
        id: UUID,
        checkpoint: AgentRunCheckpoint,
        tools: [AgentToolDefinition],
        policy: AgentContextPolicy,
        deadline: Date,
        synchronizeOnly: Bool
    ) {
        self.id = id; self.checkpoint = checkpoint; self.tools = tools
        self.policy = policy; self.deadline = deadline; self.synchronizeOnly = synchronizeOnly
    }
}

public struct AgentWorkerContextFailure: Codable, Equatable, Sendable {
    public let checkpoint: AgentRunCheckpoint
    public let reason: String
    public let cancelled: Bool
    public init(checkpoint: AgentRunCheckpoint, reason: String, cancelled: Bool) {
        self.checkpoint = checkpoint; self.reason = String(reason.prefix(4_096)); self.cancelled = cancelled
    }
}

public struct AgentWorkerContextResponse: Codable, Equatable, Sendable {
    public let requestID: UUID
    public let prepared: AgentPreparedContext?
    public let contextFailure: AgentWorkerContextFailure?
    public let failure: AgentWorkerRemoteFailure?

    public init(
        requestID: UUID,
        prepared: AgentPreparedContext? = nil,
        contextFailure: AgentWorkerContextFailure? = nil,
        failure: AgentWorkerRemoteFailure? = nil
    ) {
        self.requestID = requestID; self.prepared = prepared
        self.contextFailure = contextFailure; self.failure = failure
    }
}

public struct AgentWorkerRecordRequest: Codable, Equatable, Sendable {
    public let id: UUID
    public let checkpoint: AgentRunCheckpoint
    public let event: AgentRunEvent
    public init(id: UUID, checkpoint: AgentRunCheckpoint, event: AgentRunEvent) {
        self.id = id; self.checkpoint = checkpoint; self.event = event
    }
}

public struct AgentWorkerRecordResponse: Codable, Equatable, Sendable {
    public let requestID: UUID
    public let failure: AgentWorkerRemoteFailure?
    public init(requestID: UUID, failure: AgentWorkerRemoteFailure? = nil) {
        self.requestID = requestID; self.failure = failure
    }
}

public struct AgentWorkerFinished: Codable, Equatable, Sendable {
    public let checkpoint: AgentRunCheckpoint
    public init(checkpoint: AgentRunCheckpoint) { self.checkpoint = checkpoint }
}

public enum AgentWorkerMessage: Codable, Equatable, Sendable {
    case hello(AgentWorkerHelloRequest)
    case helloResponse(AgentWorkerHelloResponse)
    case start(AgentWorkerStartRequest)
    case modelRequest(AgentWorkerModelRequest)
    case modelEvent(AgentWorkerModelEvent)
    case modelResponse(AgentWorkerModelResponse)
    case toolRequest(AgentWorkerToolRequest)
    case toolResponse(AgentWorkerToolResponse)
    case completionRequest(AgentWorkerCompletionRequest)
    case completionResponse(AgentWorkerCompletionResponse)
    case contextRequest(AgentWorkerContextRequest)
    case contextResponse(AgentWorkerContextResponse)
    case recordRequest(AgentWorkerRecordRequest)
    case recordResponse(AgentWorkerRecordResponse)
    case finished(AgentWorkerFinished)
    case failed(AgentWorkerRemoteFailure)
}

public enum AgentWorkerWireCodec {
    public static func encodeLine(_ message: AgentWorkerMessage) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        var data = try encoder.encode(message)
        guard data.count <= ChatOSAgentWorkerProtocol.maximumInlineBytes else {
            throw AgentRuntimeError.contextTooLarge
        }
        data.append(0x0A)
        return data
    }

    public static func decode(_ data: Data) throws -> AgentWorkerMessage {
        guard !data.isEmpty, data.count <= ChatOSAgentWorkerProtocol.maximumInlineBytes else {
            throw AgentRuntimeError.invalidResponse
        }
        return try JSONDecoder().decode(AgentWorkerMessage.self, from: data)
    }
}
