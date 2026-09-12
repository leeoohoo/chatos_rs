// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public let localAgentProtocolVersion: UInt32 = 11

public enum LocalAgentProtocolJSON {
    public static func encoder() -> JSONEncoder {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }

    public static func decoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .custom { path in
            let source = path.last?.stringValue ?? ""
            let components = source.split(separator: "_")
            guard let first = components.first else { return LocalAgentCodingKey(source) }
            let value = String(first) + components.dropFirst().map { component in
                switch component.lowercased() {
                case "id": "ID"
                case "ids": "IDs"
                case "ok": "OK"
                case "url": "URL"
                default: component.prefix(1).uppercased() + component.dropFirst()
                }
            }.joined()
            return LocalAgentCodingKey(value)
        }
        return decoder
    }
}

private struct LocalAgentCodingKey: CodingKey {
    let stringValue: String
    let intValue: Int? = nil
    init(_ stringValue: String) { self.stringValue = stringValue }
    init?(stringValue: String) { self.init(stringValue) }
    init?(intValue: Int) { return nil }
}

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

public struct LocalAgentRetryTask: Codable, Equatable, Sendable {
    public var taskID: String
    public var expectedRunID: String
    public var instruction: String?

    public init(taskID: String, expectedRunID: String, instruction: String? = nil) {
        self.taskID = taskID
        self.expectedRunID = expectedRunID
        self.instruction = instruction
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
    case retryTask(LocalAgentRetryTask)
    case pauseRun(runID: String)
    case resumeRun(runID: String)
    case cancelRun(runID: String)
    case answerUserQuestion(runID: String, interactionID: String, answer: LocalAgentUserAnswer)
    case decideToolApproval(invocationID: String, decision: LocalAgentToolApprovalDecision, reason: String?)
    case getRun(runID: String)
    case getTask(taskID: String)
    case getMainChatRunBinding(runID: String)
    case listRuns(cursor: String?, limit: UInt32)
    case listTasks(cursor: String?, limit: UInt32)
    case subscribeRunEvents(afterSequence: UInt64, limit: UInt32)
    case getUIEventCursor
    case acknowledgeUIEvents(throughSequence: UInt64)
    case getStorageProfile
    case testPostgresConnection(connectionSecretReference: String)
    case applyStorageProfile(profile: LocalAgentStorageProfileSelection, confirmNoActiveRuns: Bool)
    case exportClientData(destinationReference: String, includeLargePayloadReferences: Bool)
    case importClientData(sourceReference: String, expectedArchiveDigest: String, confirmNoActiveRuns: Bool)
    case installProjectPluginCapability(
        projectID: String,
        pluginID: String,
        releaseID: String,
        capabilityRecord: LocalAgentJSONValue
    )
    case removeProjectPluginCapability(projectID: String, pluginID: String, releaseID: String)
}

extension LocalAgentCommand: Encodable {
    private enum CodingKeys: String, CodingKey { case type, payload }
    private struct RunPayload: Encodable { let runID: String }
    private struct TaskPayload: Encodable { let taskID: String }
    private struct ListPayload: Encodable {
        let cursor: String?
        let limit: UInt32
    }
    private struct EventsPayload: Encodable {
        let afterSeq: UInt64
        let limit: UInt32
    }
    private struct AcknowledgeEventsPayload: Encodable { let throughSeq: UInt64 }
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
    private struct InstallPluginCapabilityPayload: Encodable {
        let projectID: String
        let pluginID: String
        let releaseID: String
        let capabilityRecord: LocalAgentJSONValue
    }
    private struct RemovePluginCapabilityPayload: Encodable {
        let projectID: String
        let pluginID: String
        let releaseID: String
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
        case let .retryTask(payload):
            try container.encode("retry_task", forKey: .type)
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
        case let .getTask(taskID):
            try container.encode("get_task", forKey: .type)
            try container.encode(TaskPayload(taskID: taskID), forKey: .payload)
        case let .getMainChatRunBinding(runID):
            try encodeRun("get_main_chat_run_binding", runID, into: &container)
        case let .listRuns(cursor, limit):
            try container.encode("list_runs", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .listTasks(cursor, limit):
            try container.encode("list_tasks", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .subscribeRunEvents(afterSequence, limit):
            try container.encode("subscribe_run_events", forKey: .type)
            try container.encode(EventsPayload(afterSeq: afterSequence, limit: limit), forKey: .payload)
        case .getUIEventCursor:
            try container.encode("get_ui_event_cursor", forKey: .type)
        case let .acknowledgeUIEvents(throughSequence):
            try container.encode("acknowledge_ui_events", forKey: .type)
            try container.encode(
                AcknowledgeEventsPayload(throughSeq: throughSequence),
                forKey: .payload
            )
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
        case let .installProjectPluginCapability(projectID, pluginID, releaseID, capability):
            try container.encode("install_project_plugin_capability", forKey: .type)
            try container.encode(
                InstallPluginCapabilityPayload(
                    projectID: projectID,
                    pluginID: pluginID,
                    releaseID: releaseID,
                    capabilityRecord: capability
                ),
                forKey: .payload
            )
        case let .removeProjectPluginCapability(projectID, pluginID, releaseID):
            try container.encode("remove_project_plugin_capability", forKey: .type)
            try container.encode(
                RemovePluginCapabilityPayload(
                    projectID: projectID,
                    pluginID: pluginID,
                    releaseID: releaseID
                ),
                forKey: .payload
            )
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

    public init(
        runID: String,
        profileKey: String,
        ownerUserID: String,
        ownerEntityType: String,
        ownerEntityID: String,
        projectID: String?,
        status: LocalAgentRunStatus,
        version: UInt64,
        stepSeq: UInt64,
        iteration: UInt32,
        retryCount: UInt32,
        modelConfigID: String,
        modelConfigRevision: UInt64,
        modelRuntimeSnapshot: LocalAgentJSONValue,
        contextStrategy: String,
        promptRevision: String,
        capabilitySnapshotRef: String,
        pendingBatchID: String? = nil,
        pendingInteraction: LocalAgentJSONValue? = nil,
        terminalOutcome: LocalAgentJSONValue? = nil,
        deadlineAt: String? = nil,
        createdAt: String,
        updatedAt: String
    ) {
        self.runID = runID
        self.profileKey = profileKey
        self.ownerUserID = ownerUserID
        self.ownerEntityType = ownerEntityType
        self.ownerEntityID = ownerEntityID
        self.projectID = projectID
        self.status = status
        self.version = version
        self.stepSeq = stepSeq
        self.iteration = iteration
        self.retryCount = retryCount
        self.modelConfigID = modelConfigID
        self.modelConfigRevision = modelConfigRevision
        self.modelRuntimeSnapshot = modelRuntimeSnapshot
        self.contextStrategy = contextStrategy
        self.promptRevision = promptRevision
        self.capabilitySnapshotRef = capabilitySnapshotRef
        self.pendingBatchID = pendingBatchID
        self.pendingInteraction = pendingInteraction
        self.terminalOutcome = terminalOutcome
        self.deadlineAt = deadlineAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentTaskSnapshot: Codable, Equatable, Sendable {
    public var taskID: String
    public var revision: UInt64
    public var sourceThreadID: String
    public var sourceTurnID: String
    public var projectID: String
    public var currentRunID: String
    public var runIDs: [String]
    public var objective: String
    public var acceptanceCriteria: [String]
    public var status: String
    public var modelConfigID: String
    public var modelConfigRevision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        taskID: String,
        revision: UInt64,
        sourceThreadID: String,
        sourceTurnID: String,
        projectID: String,
        currentRunID: String,
        runIDs: [String],
        objective: String,
        acceptanceCriteria: [String],
        status: String,
        modelConfigID: String,
        modelConfigRevision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.taskID = taskID
        self.revision = revision
        self.sourceThreadID = sourceThreadID
        self.sourceTurnID = sourceTurnID
        self.projectID = projectID
        self.currentRunID = currentRunID
        self.runIDs = runIDs
        self.objective = objective
        self.acceptanceCriteria = acceptanceCriteria
        self.status = status
        self.modelConfigID = modelConfigID
        self.modelConfigRevision = modelConfigRevision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentUIEvent: Decodable, Equatable, Sendable {
    public var eventSeq: UInt64
    public var emittedAt: String
    public var event: LocalAgentUIEventPayload

    public init(eventSeq: UInt64, emittedAt: String, event: LocalAgentUIEventPayload) {
        self.eventSeq = eventSeq
        self.emittedAt = emittedAt
        self.event = event
    }
}

public struct LocalAgentMainChatRunBinding: Codable, Equatable, Sendable {
    public var runID: String
    public var threadID: String
    public var turnID: String
    public var messageID: String
    public var userMessage: LocalAgentStoredMessage

    public init(
        runID: String,
        threadID: String,
        turnID: String,
        messageID: String,
        userMessage: LocalAgentStoredMessage
    ) {
        self.runID = runID
        self.threadID = threadID
        self.turnID = turnID
        self.messageID = messageID
        self.userMessage = userMessage
    }
}

public enum LocalAgentStoredMessageRole: String, Codable, Equatable, Sendable {
    case system, user, assistant, tool
}

public enum LocalAgentStoredMessageMode: String, Codable, Equatable, Sendable {
    case semantic
    case providerContext = "provider_context"
}

public enum LocalAgentStoredMemorySyncStatus: String, Codable, Equatable, Sendable {
    case pending, synced, failed
}

public struct LocalAgentStoredMessage: Codable, Equatable, Sendable {
    public var recordID: String
    public var runID: String
    public var threadID: String
    public var turnID: String
    public var sequence: UInt64
    public var role: LocalAgentStoredMessageRole
    public var content: String?
    public var reasoning: String?
    public var structuredPayload: LocalAgentJSONValue?
    public var toolCallID: String?
    public var responseID: String?
    public var messageMode: LocalAgentStoredMessageMode
    public var messageSource: String
    public var memorySyncStatus: LocalAgentStoredMemorySyncStatus
    public var createdAt: String

    public init(
        recordID: String,
        runID: String,
        threadID: String,
        turnID: String,
        sequence: UInt64,
        role: LocalAgentStoredMessageRole,
        content: String? = nil,
        reasoning: String? = nil,
        structuredPayload: LocalAgentJSONValue? = nil,
        toolCallID: String? = nil,
        responseID: String? = nil,
        messageMode: LocalAgentStoredMessageMode,
        messageSource: String,
        memorySyncStatus: LocalAgentStoredMemorySyncStatus,
        createdAt: String
    ) {
        self.recordID = recordID
        self.runID = runID
        self.threadID = threadID
        self.turnID = turnID
        self.sequence = sequence
        self.role = role
        self.content = content
        self.reasoning = reasoning
        self.structuredPayload = structuredPayload
        self.toolCallID = toolCallID
        self.responseID = responseID
        self.messageMode = messageMode
        self.messageSource = messageSource
        self.memorySyncStatus = memorySyncStatus
        self.createdAt = createdAt
    }
}

public enum LocalAgentModelStreamDeltaKind: String, Decodable, Equatable, Sendable {
    case content, reasoning, status
}

public struct LocalAgentModelStreamEvent: Decodable, Equatable, Sendable {
    public var runID: String
    public var stepSeq: UInt64
    public var deltaKind: LocalAgentModelStreamDeltaKind
    public var delta: String

    public init(
        runID: String,
        stepSeq: UInt64,
        deltaKind: LocalAgentModelStreamDeltaKind,
        delta: String
    ) {
        self.runID = runID
        self.stepSeq = stepSeq
        self.deltaKind = deltaKind
        self.delta = delta
    }
}

public enum LocalAgentToolEffect: String, Decodable, Equatable, Sendable {
    case read
    case idempotentWrite = "idempotent_write"
    case write, billable, terminal
}

public enum LocalAgentToolExecutionStatus: String, Decodable, Equatable, Sendable {
    case requested
    case awaitingApproval = "awaiting_approval"
    case approved, started, succeeded, failed, rejected
    case outcomeUnknown = "outcome_unknown"
}

public struct LocalAgentToolSnapshot: Decodable, Equatable, Sendable {
    public var invocationID: String
    public var runID: String
    public var batchID: String
    public var toolCallID: String
    public var toolName: String
    public var effect: LocalAgentToolEffect
    public var argumentsDigest: String
    public var status: LocalAgentToolExecutionStatus
    public var boundedResult: LocalAgentJSONValue?
    public var approvalDecidedAt: String?
    public var approvalReason: String?
    public var startedAt: String?
    public var completedAt: String?

    public init(
        invocationID: String,
        runID: String,
        batchID: String,
        toolCallID: String,
        toolName: String,
        effect: LocalAgentToolEffect,
        argumentsDigest: String,
        status: LocalAgentToolExecutionStatus,
        boundedResult: LocalAgentJSONValue? = nil,
        approvalDecidedAt: String? = nil,
        approvalReason: String? = nil,
        startedAt: String? = nil,
        completedAt: String? = nil
    ) {
        self.invocationID = invocationID
        self.runID = runID
        self.batchID = batchID
        self.toolCallID = toolCallID
        self.toolName = toolName
        self.effect = effect
        self.argumentsDigest = argumentsDigest
        self.status = status
        self.boundedResult = boundedResult
        self.approvalDecidedAt = approvalDecidedAt
        self.approvalReason = approvalReason
        self.startedAt = startedAt
        self.completedAt = completedAt
    }
}

public struct LocalAgentUserInteractionOption: Decodable, Equatable, Sendable {
    public var optionID: String
    public var label: String
    public var description: String?

    public init(optionID: String, label: String, description: String? = nil) {
        self.optionID = optionID
        self.label = label
        self.description = description
    }
}

public struct LocalAgentUserInteractionEvent: Decodable, Equatable, Sendable {
    public var interactionID: String
    public var runID: String
    public var prompt: String
    public var options: [LocalAgentUserInteractionOption]
    public var imageReferences: [String]
    public var details: LocalAgentJSONValue?

    public init(
        interactionID: String,
        runID: String,
        prompt: String,
        options: [LocalAgentUserInteractionOption],
        imageReferences: [String],
        details: LocalAgentJSONValue? = nil
    ) {
        self.interactionID = interactionID
        self.runID = runID
        self.prompt = prompt
        self.options = options
        self.imageReferences = imageReferences
        self.details = details
    }
}

public struct LocalAgentMemorySyncStatus: Decodable, Equatable, Sendable {
    public var runID: String?
    public var pendingCount: UInt64
    public var failedCount: UInt64
    public var lastErrorCode: String?

    public init(
        runID: String? = nil,
        pendingCount: UInt64,
        failedCount: UInt64,
        lastErrorCode: String? = nil
    ) {
        self.runID = runID
        self.pendingCount = pendingCount
        self.failedCount = failedCount
        self.lastErrorCode = lastErrorCode
    }
}

public enum LocalAgentHostRuntimeState: String, Decodable, Equatable, Sendable {
    case starting, ready
    case storageUnavailable = "storage_unavailable"
    case shuttingDown = "shutting_down"
}

public struct LocalAgentHostRuntimeStatus: Decodable, Equatable, Sendable {
    public var state: LocalAgentHostRuntimeState
    public var activeRunCount: UInt64
    public var errorCode: String?

    public init(
        state: LocalAgentHostRuntimeState,
        activeRunCount: UInt64,
        errorCode: String? = nil
    ) {
        self.state = state
        self.activeRunCount = activeRunCount
        self.errorCode = errorCode
    }
}

public enum LocalAgentUIEventPayload: Equatable, Sendable {
    case runSnapshot(LocalAgentRunSnapshot)
    case modelStream(LocalAgentModelStreamEvent)
    case toolSnapshot(LocalAgentToolSnapshot)
    case userInteraction(LocalAgentUserInteractionEvent)
    case memorySync(LocalAgentMemorySyncStatus)
    case hostStatus(LocalAgentHostRuntimeStatus)
}

extension LocalAgentUIEventPayload: Decodable {
    private enum CodingKeys: String, CodingKey { case type, payload }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "run_snapshot":
            self = .runSnapshot(try container.decode(LocalAgentRunSnapshot.self, forKey: .payload))
        case "model_stream":
            self = .modelStream(try container.decode(LocalAgentModelStreamEvent.self, forKey: .payload))
        case "tool_snapshot":
            self = .toolSnapshot(try container.decode(LocalAgentToolSnapshot.self, forKey: .payload))
        case "user_interaction":
            self = .userInteraction(try container.decode(LocalAgentUserInteractionEvent.self, forKey: .payload))
        case "memory_sync":
            self = .memorySync(try container.decode(LocalAgentMemorySyncStatus.self, forKey: .payload))
        case "host_status":
            self = .hostStatus(try container.decode(LocalAgentHostRuntimeStatus.self, forKey: .payload))
        case let type:
            throw DecodingError.dataCorruptedError(
                forKey: .type,
                in: container,
                debugDescription: "Unknown local Agent UI event type: \(type)"
            )
        }
    }
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
    case runCreated(operationID: String, run: LocalAgentRunSnapshot)
    case run(LocalAgentRunSnapshot)
    case task(LocalAgentTaskSnapshot)
    case mainChatRunBinding(LocalAgentMainChatRunBinding)
    case runs([LocalAgentRunSnapshot], nextCursor: String?)
    case tasks([LocalAgentTaskSnapshot], nextCursor: String?)
    case events([LocalAgentUIEvent], nextSequence: UInt64, hasMore: Bool)
    case uiEventCursor(eventSequence: UInt64)
    case storageProfile(LocalAgentStorageProfile)
    case postgresConnectionTest(LocalAgentPostgresConnectionTest)
    case dataTransfer(LocalAgentDataTransfer)
    case success
    case error(LocalAgentIPCErrorPayload)
}

extension LocalAgentResponse: Decodable {
    private enum CodingKeys: String, CodingKey { case type, payload }
    private struct Accepted: Decodable { let operationID: String }
    private struct RunCreated: Decodable {
        let operationID: String
        let run: LocalAgentRunSnapshot
    }
    private struct Runs: Decodable {
        let runs: [LocalAgentRunSnapshot]
        let nextCursor: String?
    }
    private struct Tasks: Decodable {
        let tasks: [LocalAgentTaskSnapshot]
        let nextCursor: String?
    }
    private struct Events: Decodable {
        let events: [LocalAgentUIEvent]
        let nextSeq: UInt64
        let hasMore: Bool
    }
    private struct UIEventCursor: Decodable { let eventSeq: UInt64 }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "accepted":
            self = .accepted(operationID: try container.decode(Accepted.self, forKey: .payload).operationID)
        case "run_created":
            let value = try container.decode(RunCreated.self, forKey: .payload)
            self = .runCreated(operationID: value.operationID, run: value.run)
        case "run": self = .run(try container.decode(LocalAgentRunSnapshot.self, forKey: .payload))
        case "task": self = .task(try container.decode(LocalAgentTaskSnapshot.self, forKey: .payload))
        case "main_chat_run_binding":
            self = .mainChatRunBinding(
                try container.decode(LocalAgentMainChatRunBinding.self, forKey: .payload)
            )
        case "runs":
            let value = try container.decode(Runs.self, forKey: .payload)
            self = .runs(value.runs, nextCursor: value.nextCursor)
        case "tasks":
            let value = try container.decode(Tasks.self, forKey: .payload)
            self = .tasks(value.tasks, nextCursor: value.nextCursor)
        case "events":
            let value = try container.decode(Events.self, forKey: .payload)
            self = .events(value.events, nextSequence: value.nextSeq, hasMore: value.hasMore)
        case "ui_event_cursor":
            self = .uiEventCursor(
                eventSequence: try container.decode(UIEventCursor.self, forKey: .payload).eventSeq
            )
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
