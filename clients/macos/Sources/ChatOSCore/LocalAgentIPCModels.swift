// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public let localAgentProtocolVersion: UInt32 = 5

public enum LocalAgentJSONValue: Codable, Equatable, Sendable {
    case null
    case bool(Bool)
    case signed(Int64)
    case unsigned(UInt64)
    case number(Double)
    case string(String)
    case array([LocalAgentJSONValue])
    case object([String: LocalAgentJSONValue])

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() { self = .null }
        else if let value = try? container.decode(Bool.self) { self = .bool(value) }
        else if let value = try? container.decode(Int64.self) { self = .signed(value) }
        else if let value = try? container.decode(UInt64.self) { self = .unsigned(value) }
        else if let value = try? container.decode(Double.self) { self = .number(value) }
        else if let value = try? container.decode(String.self) { self = .string(value) }
        else if let value = try? container.decode([LocalAgentJSONValue].self) { self = .array(value) }
        else { self = .object(try container.decode([String: LocalAgentJSONValue].self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case .bool(let value): try container.encode(value)
        case .signed(let value): try container.encode(value)
        case .unsigned(let value): try container.encode(value)
        case .number(let value): try container.encode(value)
        case .string(let value): try container.encode(value)
        case .array(let value): try container.encode(value)
        case .object(let value): try container.encode(value)
        }
    }
}

public struct LocalAgentFrozenSnapshot: Codable, Equatable, Sendable {
    public var snapshotID: String
    public var revision: String
    public var digest: String
    public var payload: LocalAgentJSONValue

    public init(snapshotID: String, revision: String, digest: String, payload: LocalAgentJSONValue) {
        self.snapshotID = snapshotID
        self.revision = revision
        self.digest = digest
        self.payload = payload
    }
}

public struct LocalAgentAttachmentReference: Codable, Equatable, Sendable {
    public var attachmentID: String
    public var mediaType: String
    public var payloadReference: String
    public var payloadDigest: String
    public var byteSize: UInt64

    public init(
        attachmentID: String,
        mediaType: String,
        payloadReference: String,
        payloadDigest: String,
        byteSize: UInt64
    ) {
        self.attachmentID = attachmentID
        self.mediaType = mediaType
        self.payloadReference = payloadReference
        self.payloadDigest = payloadDigest
        self.byteSize = byteSize
    }
}

public struct LocalAgentCreateMainChatTurn: Codable, Equatable, Sendable {
    public var threadID: String
    public var turnID: String
    public var messageID: String
    public var projectID: String?
    public var modelConfigID: String
    public var promptSnapshot: LocalAgentFrozenSnapshot
    public var capabilitySnapshot: LocalAgentFrozenSnapshot
    public var projectSnapshot: LocalAgentFrozenSnapshot?
    public var content: String?
    public var attachments: [LocalAgentAttachmentReference]

    public init(
        threadID: String,
        turnID: String,
        messageID: String,
        projectID: String?,
        modelConfigID: String,
        promptSnapshot: LocalAgentFrozenSnapshot,
        capabilitySnapshot: LocalAgentFrozenSnapshot,
        projectSnapshot: LocalAgentFrozenSnapshot?,
        content: String?,
        attachments: [LocalAgentAttachmentReference]
    ) {
        self.threadID = threadID
        self.turnID = turnID
        self.messageID = messageID
        self.projectID = projectID
        self.modelConfigID = modelConfigID
        self.promptSnapshot = promptSnapshot
        self.capabilitySnapshot = capabilitySnapshot
        self.projectSnapshot = projectSnapshot
        self.content = content
        self.attachments = attachments
    }
}

public struct LocalAgentCreateTask: Codable, Equatable, Sendable {
    public var taskID: String
    public var sourceThreadID: String
    public var sourceTurnID: String
    public var projectID: String
    public var objective: String
    public var acceptanceCriteria: [String]
    public var modelConfigID: String
    public var promptSnapshot: LocalAgentFrozenSnapshot
    public var projectSnapshot: LocalAgentFrozenSnapshot
    public var capabilitySnapshot: LocalAgentFrozenSnapshot

    public init(
        taskID: String,
        sourceThreadID: String,
        sourceTurnID: String,
        projectID: String,
        objective: String,
        acceptanceCriteria: [String],
        modelConfigID: String,
        promptSnapshot: LocalAgentFrozenSnapshot,
        projectSnapshot: LocalAgentFrozenSnapshot,
        capabilitySnapshot: LocalAgentFrozenSnapshot
    ) {
        self.taskID = taskID
        self.sourceThreadID = sourceThreadID
        self.sourceTurnID = sourceTurnID
        self.projectID = projectID
        self.objective = objective
        self.acceptanceCriteria = acceptanceCriteria
        self.modelConfigID = modelConfigID
        self.promptSnapshot = promptSnapshot
        self.projectSnapshot = projectSnapshot
        self.capabilitySnapshot = capabilitySnapshot
    }
}

public struct LocalAgentUserAnswer: Codable, Equatable, Sendable {
    public var text: String?
    public var selectedOptionIDs: [String]
    public var attachments: [LocalAgentAttachmentReference]

    private enum CodingKeys: String, CodingKey {
        case text
        case selectedOptionIDs = "selected_option_ids"
        case attachments
    }

    public init(
        text: String?,
        selectedOptionIDs: [String] = [],
        attachments: [LocalAgentAttachmentReference] = []
    ) {
        self.text = text
        self.selectedOptionIDs = selectedOptionIDs
        self.attachments = attachments
    }
}

public enum LocalAgentToolApprovalDecision: String, Codable, Equatable, Sendable {
    case approve
    case reject
}

public enum LocalAgentCommand: Equatable, Sendable {
    case createMainChatTurn(LocalAgentCreateMainChatTurn)
    case createTask(LocalAgentCreateTask)
    case pauseRun(runID: String)
    case resumeRun(runID: String)
    case cancelRun(runID: String)
    case answerUserQuestion(runID: String, interactionID: String, answer: LocalAgentUserAnswer)
    case decideToolApproval(invocationID: String, decision: LocalAgentToolApprovalDecision, reason: String?)
    case getRun(runID: String)
    case listRuns(cursor: String?, limit: UInt32)
    case subscribeRunEvents(afterSequence: UInt64, limit: UInt32)
    case getStorageProfile
    case testPostgresConnection(connectionSecretReference: String)
    case applyStorageProfile(profile: LocalAgentStorageProfileSelection, confirmNoActiveRuns: Bool)
    case exportClientData(destinationReference: String, includeLargePayloadReferences: Bool)
    case importClientData(sourceReference: String, expectedArchiveDigest: String, confirmNoActiveRuns: Bool)
}

extension LocalAgentCommand: Encodable {
    private enum CodingKeys: String, CodingKey { case type, payload }
    private struct RunPayload: Encodable { let runID: String }
    private struct ListPayload: Encodable {
        let cursor: String?
        let limit: UInt32
    }
    private struct EventsPayload: Encodable {
        let afterSeq: UInt64
        let limit: UInt32
    }
    private struct AnswerPayload: Encodable {
        let runID: String
        let interactionID: String
        let answer: LocalAgentUserAnswer
    }
    private struct ApprovalPayload: Encodable {
        let invocationID: String
        let decision: LocalAgentToolApprovalDecision
        let reason: String?
    }
    private struct PostgresTestPayload: Encodable { let connectionSecretReference: String }
    private struct ApplyStoragePayload: Encodable {
        let profile: LocalAgentStorageProfileSelection
        let confirmNoActiveRuns: Bool
    }
    private struct ExportPayload: Encodable {
        let destinationReference: String
        let includeLargePayloadReferences: Bool
    }
    private struct ImportPayload: Encodable {
        let sourceReference: String
        let expectedArchiveDigest: String
        let confirmNoActiveRuns: Bool
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .createMainChatTurn(payload):
            try container.encode("create_main_chat_turn", forKey: .type)
            try container.encode(payload, forKey: .payload)
        case let .createTask(payload):
            try container.encode("create_task", forKey: .type)
            try container.encode(payload, forKey: .payload)
        case let .pauseRun(runID):
            try encodeRun("pause_run", runID, into: &container)
        case let .resumeRun(runID):
            try encodeRun("resume_run", runID, into: &container)
        case let .cancelRun(runID):
            try encodeRun("cancel_run", runID, into: &container)
        case let .answerUserQuestion(runID, interactionID, answer):
            try container.encode("answer_user_question", forKey: .type)
            try container.encode(AnswerPayload(runID: runID, interactionID: interactionID, answer: answer), forKey: .payload)
        case let .decideToolApproval(invocationID, decision, reason):
            try container.encode("decide_tool_approval", forKey: .type)
            try container.encode(ApprovalPayload(invocationID: invocationID, decision: decision, reason: reason), forKey: .payload)
        case let .getRun(runID):
            try encodeRun("get_run", runID, into: &container)
        case let .listRuns(cursor, limit):
            try container.encode("list_runs", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .subscribeRunEvents(afterSequence, limit):
            try container.encode("subscribe_run_events", forKey: .type)
            try container.encode(EventsPayload(afterSeq: afterSequence, limit: limit), forKey: .payload)
        case .getStorageProfile:
            try container.encode("get_storage_profile", forKey: .type)
        case let .testPostgresConnection(reference):
            try container.encode("test_postgres_connection", forKey: .type)
            try container.encode(PostgresTestPayload(connectionSecretReference: reference), forKey: .payload)
        case let .applyStorageProfile(profile, confirmed):
            try container.encode("apply_storage_profile", forKey: .type)
            try container.encode(ApplyStoragePayload(profile: profile, confirmNoActiveRuns: confirmed), forKey: .payload)
        case let .exportClientData(reference, included):
            try container.encode("export_client_data", forKey: .type)
            try container.encode(ExportPayload(destinationReference: reference, includeLargePayloadReferences: included), forKey: .payload)
        case let .importClientData(reference, digest, confirmed):
            try container.encode("import_client_data", forKey: .type)
            try container.encode(ImportPayload(sourceReference: reference, expectedArchiveDigest: digest, confirmNoActiveRuns: confirmed), forKey: .payload)
        }
    }

    private func encodeRun(
        _ type: String,
        _ runID: String,
        into container: inout KeyedEncodingContainer<CodingKeys>
    ) throws {
        try container.encode(type, forKey: .type)
        try container.encode(RunPayload(runID: runID), forKey: .payload)
    }
}

public enum LocalAgentStorageProfileSelection: Encodable, Equatable, Sendable {
    case sqlite(databaseReference: String, encryptionSecretReference: String)
    case postgres(connectionSecretReference: String)

    private enum CodingKeys: String, CodingKey {
        case backend, databaseReference, encryptionSecretReference, connectionSecretReference
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .sqlite(database, secret):
            try container.encode("sqlite", forKey: .backend)
            try container.encode(database, forKey: .databaseReference)
            try container.encode(secret, forKey: .encryptionSecretReference)
        case let .postgres(secret):
            try container.encode("postgres", forKey: .backend)
            try container.encode(secret, forKey: .connectionSecretReference)
        }
    }
}

public enum LocalAgentRunStatus: String, Codable, Equatable, Sendable {
    case queued
    case modelReady = "model_ready"
    case modelRunning = "model_running"
    case waitingToolResult = "waiting_tool_result"
    case continuationReady = "continuation_ready"
    case retryScheduled = "retry_scheduled"
    case paused
    case needsReview = "needs_review"
    case succeeded
    case failed
    case cancelled
}

public struct LocalAgentRunSnapshot: Codable, Equatable, Sendable {
    public var runID: String
    public var profileKey: String
    public var ownerUserID: String
    public var ownerEntityType: String
    public var ownerEntityID: String
    public var projectID: String?
    public var status: LocalAgentRunStatus
    public var version: UInt64
    public var stepSeq: UInt64
    public var iteration: UInt32
    public var retryCount: UInt32
    public var modelConfigID: String
    public var modelConfigRevision: UInt64
    public var modelRuntimeSnapshot: LocalAgentJSONValue
    public var contextStrategy: String
    public var promptRevision: String
    public var capabilitySnapshotRef: String
    public var pendingBatchID: String?
    public var pendingInteraction: LocalAgentJSONValue?
    public var terminalOutcome: LocalAgentJSONValue?
    public var deadlineAt: String?
    public var createdAt: String
    public var updatedAt: String
}

public struct LocalAgentUIEvent: Codable, Equatable, Sendable {
    public var eventSeq: UInt64
    public var emittedAt: String
    public var event: LocalAgentTaggedValue
}

public struct LocalAgentTaggedValue: Codable, Equatable, Sendable {
    public var type: String
    public var payload: LocalAgentJSONValue?
}

public struct LocalAgentStorageProfile: Codable, Equatable, Sendable {
    public var backend: String
    public var health: String
    public var sqliteDatabaseReference: String?
    public var postgresConnectionSecretReference: String?
    public var schemaVersion: UInt32
    public var lastErrorCode: String?
}

public struct LocalAgentPostgresConnectionTest: Codable, Equatable, Sendable {
    public var serverVersion: String
    public var tlsActive: Bool
    public var authenticationOK: Bool
    public var transactionOK: Bool
    public var migrationPermissionOK: Bool
}

public struct LocalAgentDataTransfer: Codable, Equatable, Sendable {
    public var archiveReference: String
    public var archiveDigest: String
    public var recordCount: UInt64
}

public struct LocalAgentIPCErrorPayload: Codable, Error, Equatable, Sendable {
    public var code: String
    public var message: String
    public var retryable: Bool
}

public enum LocalAgentResponse: Equatable, Sendable {
    case accepted(operationID: String)
    case run(LocalAgentRunSnapshot)
    case runs([LocalAgentRunSnapshot], nextCursor: String?)
    case events([LocalAgentUIEvent], nextSequence: UInt64, hasMore: Bool)
    case storageProfile(LocalAgentStorageProfile)
    case postgresConnectionTest(LocalAgentPostgresConnectionTest)
    case dataTransfer(LocalAgentDataTransfer)
    case success
    case error(LocalAgentIPCErrorPayload)
}

extension LocalAgentResponse: Decodable {
    private enum CodingKeys: String, CodingKey { case type, payload }
    private struct Accepted: Decodable { let operationID: String }
    private struct Runs: Decodable {
        let runs: [LocalAgentRunSnapshot]
        let nextCursor: String?
    }
    private struct Events: Decodable {
        let events: [LocalAgentUIEvent]
        let nextSeq: UInt64
        let hasMore: Bool
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "accepted":
            self = .accepted(operationID: try container.decode(Accepted.self, forKey: .payload).operationID)
        case "run": self = .run(try container.decode(LocalAgentRunSnapshot.self, forKey: .payload))
        case "runs":
            let value = try container.decode(Runs.self, forKey: .payload)
            self = .runs(value.runs, nextCursor: value.nextCursor)
        case "events":
            let value = try container.decode(Events.self, forKey: .payload)
            self = .events(value.events, nextSequence: value.nextSeq, hasMore: value.hasMore)
        case "storage_profile": self = .storageProfile(try container.decode(LocalAgentStorageProfile.self, forKey: .payload))
        case "postgres_connection_test": self = .postgresConnectionTest(try container.decode(LocalAgentPostgresConnectionTest.self, forKey: .payload))
        case "data_transfer": self = .dataTransfer(try container.decode(LocalAgentDataTransfer.self, forKey: .payload))
        case "success": self = .success
        case "error": self = .error(try container.decode(LocalAgentIPCErrorPayload.self, forKey: .payload))
        case let type:
            throw DecodingError.dataCorruptedError(forKey: .type, in: container, debugDescription: "Unknown local Agent response type: \(type)")
        }
    }
}

public struct LocalAgentIPCReply: Decodable, Equatable, Sendable {
    public var protocolVersion: UInt32
    public var requestID: String
    public var response: LocalAgentResponse
}
