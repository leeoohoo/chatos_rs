// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public let localAgentProtocolVersion: UInt32 = 26

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

public struct LocalAgentCreateApprovalReview: Codable, Equatable, Sendable {
    public var reviewID: String
    public var modelConfigID: String
    public var source: String
    public var cwd: String
    public var operation: String
    public var requestedPermissionsDescription: String?
    public var riskLevel: String
    public var riskReason: String?
    public var reasoningEffort: String?

    public init(
        reviewID: String,
        modelConfigID: String,
        source: String,
        cwd: String,
        operation: String,
        requestedPermissionsDescription: String?,
        riskLevel: String,
        riskReason: String?,
        reasoningEffort: String?
    ) {
        self.reviewID = reviewID
        self.modelConfigID = modelConfigID
        self.source = source
        self.cwd = cwd
        self.operation = operation
        self.requestedPermissionsDescription = requestedPermissionsDescription
        self.riskLevel = riskLevel
        self.riskReason = riskReason
        self.reasoningEffort = reasoningEffort
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

public struct LocalAgentProjectDraft: Codable, Equatable, Sendable {
    public var name: String
    public var description: String
    public var workspaceID: String
    public var relativeRoot: String

    public init(name: String, description: String, workspaceID: String, relativeRoot: String) {
        self.name = name
        self.description = description
        self.workspaceID = workspaceID
        self.relativeRoot = relativeRoot
    }
}

public enum LocalAgentProjectStatus: String, Codable, Equatable, Sendable {
    case active, archived, removed
}

public struct LocalAgentProjectSnapshot: Codable, Equatable, Sendable {
    public var projectID: String
    public var ownerUserID: String
    public var draft: LocalAgentProjectDraft
    public var revision: UInt64
    public var status: LocalAgentProjectStatus
    public var createdAt: String
    public var updatedAt: String

    public init(
        projectID: String,
        ownerUserID: String,
        draft: LocalAgentProjectDraft,
        revision: UInt64,
        status: LocalAgentProjectStatus,
        createdAt: String,
        updatedAt: String
    ) {
        self.projectID = projectID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.status = status
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public enum LocalAgentClipboardKind: String, Codable, Equatable, Sendable {
    case text, url, files, image
}

public struct LocalAgentClipboardDraft: Codable, Equatable, Sendable {
    public var kind: LocalAgentClipboardKind
    public var mimeType: String
    public var contentHash: String
    public var textPreview: String?
    public var sourceBundleID: String?
    public var payloadReference: String
    public var byteCount: UInt64
    public var pasteboardType: String?

    public init(
        kind: LocalAgentClipboardKind,
        mimeType: String,
        contentHash: String,
        textPreview: String?,
        sourceBundleID: String?,
        payloadReference: String,
        byteCount: UInt64,
        pasteboardType: String?
    ) {
        self.kind = kind
        self.mimeType = mimeType
        self.contentHash = contentHash
        self.textPreview = textPreview
        self.sourceBundleID = sourceBundleID
        self.payloadReference = payloadReference
        self.byteCount = byteCount
        self.pasteboardType = pasteboardType
    }
}

public struct LocalAgentClipboardSnapshot: Codable, Equatable, Sendable {
    public var entryID: String
    public var ownerUserID: String
    public var draft: LocalAgentClipboardDraft
    public var revision: UInt64
    public var isPinned: Bool
    public var createdAt: String
    public var updatedAt: String

    public init(
        entryID: String,
        ownerUserID: String,
        draft: LocalAgentClipboardDraft,
        revision: UInt64,
        isPinned: Bool,
        createdAt: String,
        updatedAt: String
    ) {
        self.entryID = entryID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.isPinned = isPinned
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentClipboardMutationResult: Codable, Equatable, Sendable {
    public var entry: LocalAgentClipboardSnapshot?
    public var discardedPayloadReferences: [String]

    public init(
        entry: LocalAgentClipboardSnapshot?,
        discardedPayloadReferences: [String]
    ) {
        self.entry = entry
        self.discardedPayloadReferences = discardedPayloadReferences
    }
}

public enum LocalAgentMediaKind: String, Codable, Equatable, Sendable {
    case image, video
}

public enum LocalAgentMediaStatus: String, Codable, Equatable, Sendable {
    case pending, completed, failed
}

public struct LocalAgentMediaAsset: Codable, Equatable, Sendable {
    public var assetID: String
    public var mimeType: String
    public var payloadReference: String
    public var contentHash: String
    public var byteCount: UInt64
    public var revisedPrompt: String?

    public init(
        assetID: String,
        mimeType: String,
        payloadReference: String,
        contentHash: String,
        byteCount: UInt64,
        revisedPrompt: String?
    ) {
        self.assetID = assetID
        self.mimeType = mimeType
        self.payloadReference = payloadReference
        self.contentHash = contentHash
        self.byteCount = byteCount
        self.revisedPrompt = revisedPrompt
    }
}

public struct LocalAgentMediaDraft: Codable, Equatable, Sendable {
    public var projectID: String?
    public var kind: LocalAgentMediaKind
    public var status: LocalAgentMediaStatus
    public var prompt: String
    public var modelName: String
    public var generatedAt: String
    public var assets: [LocalAgentMediaAsset]

    public init(
        projectID: String?,
        kind: LocalAgentMediaKind,
        status: LocalAgentMediaStatus,
        prompt: String,
        modelName: String,
        generatedAt: String,
        assets: [LocalAgentMediaAsset]
    ) {
        self.projectID = projectID
        self.kind = kind
        self.status = status
        self.prompt = prompt
        self.modelName = modelName
        self.generatedAt = generatedAt
        self.assets = assets
    }
}

public struct LocalAgentMediaSnapshot: Codable, Equatable, Sendable {
    public var recordID: String
    public var ownerUserID: String
    public var draft: LocalAgentMediaDraft
    public var revision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        recordID: String,
        ownerUserID: String,
        draft: LocalAgentMediaDraft,
        revision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.recordID = recordID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentMediaMutationResult: Codable, Equatable, Sendable {
    public var record: LocalAgentMediaSnapshot?
    public var discardedPayloadReferences: [String]

    public init(
        record: LocalAgentMediaSnapshot?,
        discardedPayloadReferences: [String]
    ) {
        self.record = record
        self.discardedPayloadReferences = discardedPayloadReferences
    }
}

public enum LocalAgentStoryKind: String, Codable, Equatable, Sendable {
    case project
    case agentRun = "agent_run"
    case mediaBatch = "media_batch"
}

public struct LocalAgentStoryDraft: Codable, Equatable, Sendable {
    public var projectID: String
    public var kind: LocalAgentStoryKind
    public var status: String?
    public var state: LocalAgentJSONValue

    public init(
        projectID: String,
        kind: LocalAgentStoryKind,
        status: String?,
        state: LocalAgentJSONValue
    ) {
        self.projectID = projectID
        self.kind = kind
        self.status = status
        self.state = state
    }
}

public struct LocalAgentStorySnapshot: Codable, Equatable, Sendable {
    public var recordID: String
    public var ownerUserID: String
    public var draft: LocalAgentStoryDraft
    public var revision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        recordID: String,
        ownerUserID: String,
        draft: LocalAgentStoryDraft,
        revision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.recordID = recordID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public enum LocalAgentNotepadKind: String, Codable, Equatable, Sendable {
    case folder
    case note
}

public struct LocalAgentNotepadDraft: Codable, Equatable, Sendable {
    public var kind: LocalAgentNotepadKind
    public var folder: String
    public var title: String
    public var content: String
    public var tags: [String]

    public init(
        kind: LocalAgentNotepadKind,
        folder: String,
        title: String,
        content: String,
        tags: [String]
    ) {
        self.kind = kind
        self.folder = folder
        self.title = title
        self.content = content
        self.tags = tags
    }
}

public struct LocalAgentNotepadSnapshot: Codable, Equatable, Sendable {
    public var recordID: String
    public var ownerUserID: String
    public var draft: LocalAgentNotepadDraft
    public var revision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        recordID: String,
        ownerUserID: String,
        draft: LocalAgentNotepadDraft,
        revision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.recordID = recordID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentTerminalHistoryDraft: Codable, Equatable, Sendable {
    public var projectID: String?
    public var terminalSessionID: String
    public var command: String
    public var exitCode: Int32?
    public var state: LocalAgentJSONValue

    public init(
        projectID: String?,
        terminalSessionID: String,
        command: String,
        exitCode: Int32?,
        state: LocalAgentJSONValue
    ) {
        self.projectID = projectID
        self.terminalSessionID = terminalSessionID
        self.command = command
        self.exitCode = exitCode
        self.state = state
    }
}

public struct LocalAgentTerminalHistorySnapshot: Codable, Equatable, Sendable {
    public var recordID: String
    public var ownerUserID: String
    public var draft: LocalAgentTerminalHistoryDraft
    public var revision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        recordID: String,
        ownerUserID: String,
        draft: LocalAgentTerminalHistoryDraft,
        revision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.recordID = recordID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentApprovalHistoryDraft: Codable, Equatable, Sendable {
    public var command: String
    public var cwd: String
    public var source: String
    public var mode: String
    public var decision: String
    public var risk: String
    public var reason: String?

    public init(
        command: String,
        cwd: String,
        source: String,
        mode: String,
        decision: String,
        risk: String,
        reason: String?
    ) {
        self.command = command
        self.cwd = cwd
        self.source = source
        self.mode = mode
        self.decision = decision
        self.risk = risk
        self.reason = reason
    }
}

public struct LocalAgentApprovalHistorySnapshot: Codable, Equatable, Sendable {
    public var recordID: String
    public var ownerUserID: String
    public var draft: LocalAgentApprovalHistoryDraft
    public var revision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        recordID: String,
        ownerUserID: String,
        draft: LocalAgentApprovalHistoryDraft,
        revision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.recordID = recordID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentClientSettingSnapshot: Codable, Equatable, Sendable {
    public var key: String
    public var ownerUserID: String
    public var value: LocalAgentJSONValue
    public var revision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        key: String,
        ownerUserID: String,
        value: LocalAgentJSONValue,
        revision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.key = key
        self.ownerUserID = ownerUserID
        self.value = value
        self.revision = revision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct LocalAgentInstalledPluginDraft: Codable, Equatable, Sendable {
    public var pluginID: String
    public var release: String
    public var enabled: Bool
    public var installation: LocalAgentJSONValue

    public init(
        pluginID: String,
        release: String,
        enabled: Bool,
        installation: LocalAgentJSONValue
    ) {
        self.pluginID = pluginID
        self.release = release
        self.enabled = enabled
        self.installation = installation
    }
}

public struct LocalAgentInstalledPluginSnapshot: Codable, Equatable, Sendable {
    public var recordID: String
    public var ownerUserID: String
    public var draft: LocalAgentInstalledPluginDraft
    public var revision: UInt64
    public var createdAt: String
    public var updatedAt: String

    public init(
        recordID: String,
        ownerUserID: String,
        draft: LocalAgentInstalledPluginDraft,
        revision: UInt64,
        createdAt: String,
        updatedAt: String
    ) {
        self.recordID = recordID
        self.ownerUserID = ownerUserID
        self.draft = draft
        self.revision = revision
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public enum LocalAgentCommand: Equatable, Sendable {
    case updateAccessToken(String)
    case createMainChatTurn(LocalAgentCreateMainChatTurn)
    case createApprovalReview(LocalAgentCreateApprovalReview)
    case createTask(LocalAgentCreateTask)
    case retryTask(LocalAgentRetryTask)
    case pauseRun(runID: String, expectedVersion: UInt64)
    case resumeRun(runID: String, expectedVersion: UInt64)
    case cancelRun(runID: String, expectedVersion: UInt64)
    case answerUserQuestion(runID: String, interactionID: String, answer: LocalAgentUserAnswer)
    case decideToolApproval(
        runID: String,
        invocationID: String,
        decision: LocalAgentToolApprovalDecision,
        reason: String?
    )
    case getRun(runID: String)
    case getRunDetail(runID: String, eventLimit: UInt32, eventOffset: UInt32)
    case getTask(taskID: String)
    case getTaskGraph(sourceThreadID: String, sourceTurnID: String)
    case getTaskRunDetail(taskID: String, runID: String, eventLimit: UInt32, eventOffset: UInt32)
    case getMainChatRunBinding(runID: String)
    case listRuns(cursor: String?, limit: UInt32)
    case listTasks(cursor: String?, limit: UInt32)
    case getProject(projectID: String)
    case listProjects(cursor: String?, limit: UInt32, includeInactive: Bool)
    case createProject(projectID: String, draft: LocalAgentProjectDraft)
    case updateProject(
        projectID: String,
        expectedRevision: UInt64,
        draft: LocalAgentProjectDraft,
        status: LocalAgentProjectStatus
    )
    case getClipboard(entryID: String)
    case listClipboard(cursor: String?, limit: UInt32)
    case storeClipboard(entryID: String, draft: LocalAgentClipboardDraft)
    case setClipboardPinned(entryID: String, expectedRevision: UInt64, isPinned: Bool)
    case deleteClipboard(entryID: String, expectedRevision: UInt64)
    case getMedia(recordID: String)
    case listMedia(cursor: String?, limit: UInt32)
    case putMedia(recordID: String, expectedRevision: UInt64?, draft: LocalAgentMediaDraft)
    case deleteMedia(recordID: String, expectedRevision: UInt64)
    case getStory(recordID: String)
    case listStories(cursor: String?, limit: UInt32)
    case putStory(recordID: String, expectedRevision: UInt64?, draft: LocalAgentStoryDraft)
    case deleteStory(recordID: String, expectedRevision: UInt64)
    case getNotepad(recordID: String)
    case listNotepad(cursor: String?, limit: UInt32)
    case putNotepad(recordID: String, expectedRevision: UInt64?, draft: LocalAgentNotepadDraft)
    case deleteNotepad(recordID: String, expectedRevision: UInt64)
    case renameNotepadFolder(folder: String, replacement: String)
    case deleteNotepadFolder(folder: String, recursive: Bool)
    case getClientSetting(key: String)
    case putClientSetting(key: String, expectedRevision: UInt64?, value: LocalAgentJSONValue)
    case deleteClientSetting(key: String, expectedRevision: UInt64)
    case appendTerminalHistory(recordID: String, draft: LocalAgentTerminalHistoryDraft)
    case listTerminalHistory(cursor: String?, limit: UInt32)
    case deleteTerminalHistory(recordID: String, expectedRevision: UInt64)
    case clearTerminalHistory
    case appendApprovalHistory(recordID: String, draft: LocalAgentApprovalHistoryDraft)
    case listApprovalHistory(cursor: String?, limit: UInt32)
    case listInstalledPlugins(cursor: String?, limit: UInt32)
    case putInstalledPlugin(expectedRevision: UInt64?, draft: LocalAgentInstalledPluginDraft)
    case deleteInstalledPlugin(pluginID: String, expectedRevision: UInt64)
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
    private struct AccessTokenPayload: Encodable { let accessToken: String }
    private struct RunControlPayload: Encodable {
        let runID: String
        let expectedVersion: UInt64
    }
    private struct RunDetailPayload: Encodable {
        let runID: String
        let eventLimit: UInt32
        let eventOffset: UInt32
    }
    private struct TaskPayload: Encodable { let taskID: String }
    private struct TaskGraphPayload: Encodable {
        let sourceThreadID: String
        let sourceTurnID: String
    }
    private struct TaskRunDetailPayload: Encodable {
        let taskID: String
        let runID: String
        let eventLimit: UInt32
        let eventOffset: UInt32
    }
    private struct ListPayload: Encodable {
        let cursor: String?
        let limit: UInt32
    }
    private struct PutInstalledPluginPayload: Encodable {
        let expectedRevision: UInt64?
        let draft: LocalAgentInstalledPluginDraft
    }
    private struct DeleteInstalledPluginPayload: Encodable {
        let pluginID: String
        let expectedRevision: UInt64
    }
    private struct ProjectPayload: Encodable { let projectID: String }
    private struct ListProjectsPayload: Encodable {
        let cursor: String?
        let limit: UInt32
        let includeInactive: Bool
    }
    private struct CreateProjectPayload: Encodable {
        let projectID: String
        let draft: LocalAgentProjectDraft
    }
    private struct UpdateProjectPayload: Encodable {
        let projectID: String
        let expectedRevision: UInt64
        let draft: LocalAgentProjectDraft
        let status: LocalAgentProjectStatus
    }
    private struct ClipboardPayload: Encodable { let entryID: String }
    private struct StoreClipboardPayload: Encodable {
        let entryID: String
        let draft: LocalAgentClipboardDraft
    }
    private struct SetClipboardPinnedPayload: Encodable {
        let entryID: String
        let expectedRevision: UInt64
        let isPinned: Bool
    }
    private struct DeleteClipboardPayload: Encodable {
        let entryID: String
        let expectedRevision: UInt64
    }
    private struct MediaPayload: Encodable { let recordID: String }
    private struct PutMediaPayload: Encodable {
        let recordID: String
        let expectedRevision: UInt64?
        let draft: LocalAgentMediaDraft
    }
    private struct DeleteMediaPayload: Encodable {
        let recordID: String
        let expectedRevision: UInt64
    }
    private struct StoryPayload: Encodable { let recordID: String }
    private struct PutStoryPayload: Encodable {
        let recordID: String
        let expectedRevision: UInt64?
        let draft: LocalAgentStoryDraft
    }
    private struct DeleteStoryPayload: Encodable {
        let recordID: String
        let expectedRevision: UInt64
    }
    private struct NotepadPayload: Encodable { let recordID: String }
    private struct PutNotepadPayload: Encodable {
        let recordID: String
        let expectedRevision: UInt64?
        let draft: LocalAgentNotepadDraft
    }
    private struct DeleteNotepadPayload: Encodable {
        let recordID: String
        let expectedRevision: UInt64
    }
    private struct RenameNotepadFolderPayload: Encodable {
        let folder: String
        let replacement: String
    }
    private struct DeleteNotepadFolderPayload: Encodable {
        let folder: String
        let recursive: Bool
    }
    private struct ClientSettingPayload: Encodable { let key: String }
    private struct PutClientSettingPayload: Encodable {
        let key: String
        let expectedRevision: UInt64?
        let value: LocalAgentJSONValue
    }
    private struct DeleteClientSettingPayload: Encodable {
        let key: String
        let expectedRevision: UInt64
    }
    private struct AppendTerminalHistoryPayload: Encodable {
        let recordID: String
        let draft: LocalAgentTerminalHistoryDraft
    }
    private struct DeleteTerminalHistoryPayload: Encodable {
        let recordID: String
        let expectedRevision: UInt64
    }
    private struct AppendApprovalHistoryPayload: Encodable {
        let recordID: String
        let draft: LocalAgentApprovalHistoryDraft
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
        let runID: String
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
        case let .updateAccessToken(accessToken):
            try container.encode("update_access_token", forKey: .type)
            try container.encode(AccessTokenPayload(accessToken: accessToken), forKey: .payload)
        case let .createMainChatTurn(payload):
            try container.encode("create_main_chat_turn", forKey: .type)
            try container.encode(payload, forKey: .payload)
        case let .createApprovalReview(payload):
            try container.encode("create_approval_review", forKey: .type)
            try container.encode(payload, forKey: .payload)
        case let .createTask(payload):
            try container.encode("create_task", forKey: .type)
            try container.encode(payload, forKey: .payload)
        case let .retryTask(payload):
            try container.encode("retry_task", forKey: .type)
            try container.encode(payload, forKey: .payload)
        case let .pauseRun(runID, expectedVersion):
            try encodeRunControl("pause_run", runID, expectedVersion, into: &container)
        case let .resumeRun(runID, expectedVersion):
            try encodeRunControl("resume_run", runID, expectedVersion, into: &container)
        case let .cancelRun(runID, expectedVersion):
            try encodeRunControl("cancel_run", runID, expectedVersion, into: &container)
        case let .answerUserQuestion(runID, interactionID, answer):
            try container.encode("answer_user_question", forKey: .type)
            try container.encode(AnswerPayload(runID: runID, interactionID: interactionID, answer: answer), forKey: .payload)
        case let .decideToolApproval(runID, invocationID, decision, reason):
            try container.encode("decide_tool_approval", forKey: .type)
            try container.encode(
                ApprovalPayload(
                    runID: runID,
                    invocationID: invocationID,
                    decision: decision,
                    reason: reason
                ),
                forKey: .payload
            )
        case let .getRun(runID):
            try encodeRun("get_run", runID, into: &container)
        case let .getRunDetail(runID, eventLimit, eventOffset):
            try container.encode("get_run_detail", forKey: .type)
            try container.encode(
                RunDetailPayload(
                    runID: runID,
                    eventLimit: eventLimit,
                    eventOffset: eventOffset
                ),
                forKey: .payload
            )
        case let .getTask(taskID):
            try container.encode("get_task", forKey: .type)
            try container.encode(TaskPayload(taskID: taskID), forKey: .payload)
        case let .getTaskGraph(sourceThreadID, sourceTurnID):
            try container.encode("get_task_graph", forKey: .type)
            try container.encode(
                TaskGraphPayload(
                    sourceThreadID: sourceThreadID,
                    sourceTurnID: sourceTurnID
                ),
                forKey: .payload
            )
        case let .getTaskRunDetail(taskID, runID, eventLimit, eventOffset):
            try container.encode("get_task_run_detail", forKey: .type)
            try container.encode(
                TaskRunDetailPayload(
                    taskID: taskID,
                    runID: runID,
                    eventLimit: eventLimit,
                    eventOffset: eventOffset
                ),
                forKey: .payload
            )
        case let .getMainChatRunBinding(runID):
            try encodeRun("get_main_chat_run_binding", runID, into: &container)
        case let .listRuns(cursor, limit):
            try container.encode("list_runs", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .listTasks(cursor, limit):
            try container.encode("list_tasks", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .getProject(projectID):
            try container.encode("get_project", forKey: .type)
            try container.encode(ProjectPayload(projectID: projectID), forKey: .payload)
        case let .listProjects(cursor, limit, includeInactive):
            try container.encode("list_projects", forKey: .type)
            try container.encode(
                ListProjectsPayload(
                    cursor: cursor,
                    limit: limit,
                    includeInactive: includeInactive
                ),
                forKey: .payload
            )
        case let .createProject(projectID, draft):
            try container.encode("create_project", forKey: .type)
            try container.encode(
                CreateProjectPayload(projectID: projectID, draft: draft),
                forKey: .payload
            )
        case let .updateProject(projectID, expectedRevision, draft, status):
            try container.encode("update_project", forKey: .type)
            try container.encode(
                UpdateProjectPayload(
                    projectID: projectID,
                    expectedRevision: expectedRevision,
                    draft: draft,
                    status: status
                ),
                forKey: .payload
            )
        case let .getClipboard(entryID):
            try container.encode("get_clipboard", forKey: .type)
            try container.encode(ClipboardPayload(entryID: entryID), forKey: .payload)
        case let .listClipboard(cursor, limit):
            try container.encode("list_clipboard", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .storeClipboard(entryID, draft):
            try container.encode("store_clipboard", forKey: .type)
            try container.encode(
                StoreClipboardPayload(entryID: entryID, draft: draft),
                forKey: .payload
            )
        case let .setClipboardPinned(entryID, expectedRevision, isPinned):
            try container.encode("set_clipboard_pinned", forKey: .type)
            try container.encode(
                SetClipboardPinnedPayload(
                    entryID: entryID,
                    expectedRevision: expectedRevision,
                    isPinned: isPinned
                ),
                forKey: .payload
            )
        case let .deleteClipboard(entryID, expectedRevision):
            try container.encode("delete_clipboard", forKey: .type)
            try container.encode(
                DeleteClipboardPayload(entryID: entryID, expectedRevision: expectedRevision),
                forKey: .payload
            )
        case let .getMedia(recordID):
            try container.encode("get_media", forKey: .type)
            try container.encode(MediaPayload(recordID: recordID), forKey: .payload)
        case let .listMedia(cursor, limit):
            try container.encode("list_media", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .putMedia(recordID, expectedRevision, draft):
            try container.encode("put_media", forKey: .type)
            try container.encode(
                PutMediaPayload(
                    recordID: recordID,
                    expectedRevision: expectedRevision,
                    draft: draft
                ),
                forKey: .payload
            )
        case let .deleteMedia(recordID, expectedRevision):
            try container.encode("delete_media", forKey: .type)
            try container.encode(
                DeleteMediaPayload(recordID: recordID, expectedRevision: expectedRevision),
                forKey: .payload
            )
        case let .getStory(recordID):
            try container.encode("get_story", forKey: .type)
            try container.encode(StoryPayload(recordID: recordID), forKey: .payload)
        case let .listStories(cursor, limit):
            try container.encode("list_stories", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .putStory(recordID, expectedRevision, draft):
            try container.encode("put_story", forKey: .type)
            try container.encode(
                PutStoryPayload(
                    recordID: recordID,
                    expectedRevision: expectedRevision,
                    draft: draft
                ),
                forKey: .payload
            )
        case let .deleteStory(recordID, expectedRevision):
            try container.encode("delete_story", forKey: .type)
            try container.encode(
                DeleteStoryPayload(recordID: recordID, expectedRevision: expectedRevision),
                forKey: .payload
            )
        case let .getNotepad(recordID):
            try container.encode("get_notepad", forKey: .type)
            try container.encode(NotepadPayload(recordID: recordID), forKey: .payload)
        case let .listNotepad(cursor, limit):
            try container.encode("list_notepad", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .putNotepad(recordID, expectedRevision, draft):
            try container.encode("put_notepad", forKey: .type)
            try container.encode(
                PutNotepadPayload(
                    recordID: recordID,
                    expectedRevision: expectedRevision,
                    draft: draft
                ),
                forKey: .payload
            )
        case let .deleteNotepad(recordID, expectedRevision):
            try container.encode("delete_notepad", forKey: .type)
            try container.encode(
                DeleteNotepadPayload(recordID: recordID, expectedRevision: expectedRevision),
                forKey: .payload
            )
        case let .renameNotepadFolder(folder, replacement):
            try container.encode("rename_notepad_folder", forKey: .type)
            try container.encode(
                RenameNotepadFolderPayload(folder: folder, replacement: replacement),
                forKey: .payload
            )
        case let .deleteNotepadFolder(folder, recursive):
            try container.encode("delete_notepad_folder", forKey: .type)
            try container.encode(
                DeleteNotepadFolderPayload(folder: folder, recursive: recursive),
                forKey: .payload
            )
        case let .getClientSetting(key):
            try container.encode("get_client_setting", forKey: .type)
            try container.encode(ClientSettingPayload(key: key), forKey: .payload)
        case let .putClientSetting(key, expectedRevision, value):
            try container.encode("put_client_setting", forKey: .type)
            try container.encode(
                PutClientSettingPayload(
                    key: key,
                    expectedRevision: expectedRevision,
                    value: value
                ),
                forKey: .payload
            )
        case let .deleteClientSetting(key, expectedRevision):
            try container.encode("delete_client_setting", forKey: .type)
            try container.encode(
                DeleteClientSettingPayload(key: key, expectedRevision: expectedRevision),
                forKey: .payload
            )
        case let .appendTerminalHistory(recordID, draft):
            try container.encode("append_terminal_history", forKey: .type)
            try container.encode(
                AppendTerminalHistoryPayload(recordID: recordID, draft: draft),
                forKey: .payload
            )
        case let .listTerminalHistory(cursor, limit):
            try container.encode("list_terminal_history", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .deleteTerminalHistory(recordID, expectedRevision):
            try container.encode("delete_terminal_history", forKey: .type)
            try container.encode(
                DeleteTerminalHistoryPayload(
                    recordID: recordID,
                    expectedRevision: expectedRevision
                ),
                forKey: .payload
            )
        case .clearTerminalHistory:
            try container.encode("clear_terminal_history", forKey: .type)
        case let .appendApprovalHistory(recordID, draft):
            try container.encode("append_approval_history", forKey: .type)
            try container.encode(
                AppendApprovalHistoryPayload(recordID: recordID, draft: draft),
                forKey: .payload
            )
        case let .listApprovalHistory(cursor, limit):
            try container.encode("list_approval_history", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .listInstalledPlugins(cursor, limit):
            try container.encode("list_installed_plugins", forKey: .type)
            try container.encode(ListPayload(cursor: cursor, limit: limit), forKey: .payload)
        case let .putInstalledPlugin(expectedRevision, draft):
            try container.encode("put_installed_plugin", forKey: .type)
            try container.encode(
                PutInstalledPluginPayload(expectedRevision: expectedRevision, draft: draft),
                forKey: .payload
            )
        case let .deleteInstalledPlugin(pluginID, expectedRevision):
            try container.encode("delete_installed_plugin", forKey: .type)
            try container.encode(
                DeleteInstalledPluginPayload(
                    pluginID: pluginID,
                    expectedRevision: expectedRevision
                ),
                forKey: .payload
            )
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

    public var containsSensitivePayload: Bool {
        if case .updateAccessToken = self { return true }
        return false
    }

    private func encodeRun(
        _ type: String,
        _ runID: String,
        into container: inout KeyedEncodingContainer<CodingKeys>
    ) throws {
        try container.encode(type, forKey: .type)
        try container.encode(RunPayload(runID: runID), forKey: .payload)
    }

    private func encodeRunControl(
        _ type: String,
        _ runID: String,
        _ expectedVersion: UInt64,
        into container: inout KeyedEncodingContainer<CodingKeys>
    ) throws {
        try container.encode(type, forKey: .type)
        try container.encode(
            RunControlPayload(runID: runID, expectedVersion: expectedVersion),
            forKey: .payload
        )
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
    public var initialRunID: String
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
        initialRunID: String,
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
        self.initialRunID = initialRunID
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

public struct LocalAgentTaskRunSummary: Codable, Equatable, Sendable {
    public var run: LocalAgentRunSnapshot
    public var resultSummary: String?
    public var reportContent: String?
    public var errorMessage: String?

    public init(
        run: LocalAgentRunSnapshot,
        resultSummary: String? = nil,
        reportContent: String? = nil,
        errorMessage: String? = nil
    ) {
        self.run = run
        self.resultSummary = resultSummary
        self.reportContent = reportContent
        self.errorMessage = errorMessage
    }
}

public struct LocalAgentTaskProjection: Codable, Equatable, Sendable {
    public var task: LocalAgentTaskSnapshot
    public var currentRun: LocalAgentTaskRunSummary

    public init(task: LocalAgentTaskSnapshot, currentRun: LocalAgentTaskRunSummary) {
        self.task = task
        self.currentRun = currentRun
    }
}

public struct LocalAgentTaskGraphNode: Codable, Equatable, Sendable {
    public var task: LocalAgentTaskProjection
    public var depth: UInt32
    public var isRoot: Bool

    public init(task: LocalAgentTaskProjection, depth: UInt32, isRoot: Bool) {
        self.task = task
        self.depth = depth
        self.isRoot = isRoot
    }
}

public struct LocalAgentTaskGraphEdge: Codable, Equatable, Sendable {
    public var edgeID: String
    public var sourceTaskID: String
    public var targetTaskID: String
    public var kind: String

    public init(edgeID: String, sourceTaskID: String, targetTaskID: String, kind: String) {
        self.edgeID = edgeID
        self.sourceTaskID = sourceTaskID
        self.targetTaskID = targetTaskID
        self.kind = kind
    }
}

public struct LocalAgentTaskGraphSnapshot: Codable, Equatable, Sendable {
    public var sourceThreadID: String
    public var sourceTurnID: String
    public var rootTaskIDs: [String]
    public var nodes: [LocalAgentTaskGraphNode]
    public var edges: [LocalAgentTaskGraphEdge]

    public init(
        sourceThreadID: String,
        sourceTurnID: String,
        rootTaskIDs: [String],
        nodes: [LocalAgentTaskGraphNode],
        edges: [LocalAgentTaskGraphEdge]
    ) {
        self.sourceThreadID = sourceThreadID
        self.sourceTurnID = sourceTurnID
        self.rootTaskIDs = rootTaskIDs
        self.nodes = nodes
        self.edges = edges
    }
}

public struct LocalAgentRunTimelineEvent: Codable, Equatable, Sendable {
    public var eventID: String
    public var eventType: String
    public var message: String?
    public var createdAt: String

    public init(eventID: String, eventType: String, message: String?, createdAt: String) {
        self.eventID = eventID
        self.eventType = eventType
        self.message = message
        self.createdAt = createdAt
    }
}

public struct LocalAgentTaskRunDetail: Codable, Equatable, Sendable {
    public var task: LocalAgentTaskSnapshot
    public var run: LocalAgentTaskRunSummary
    public var events: [LocalAgentRunTimelineEvent]
    public var eventsTotal: UInt32
    public var eventsHasMore: Bool

    public init(
        task: LocalAgentTaskSnapshot,
        run: LocalAgentTaskRunSummary,
        events: [LocalAgentRunTimelineEvent],
        eventsTotal: UInt32,
        eventsHasMore: Bool
    ) {
        self.task = task
        self.run = run
        self.events = events
        self.eventsTotal = eventsTotal
        self.eventsHasMore = eventsHasMore
    }
}

public struct LocalAgentRunDetail: Decodable, Equatable, Sendable {
    public var run: LocalAgentRunSnapshot
    public var events: [LocalAgentRunTimelineEvent]
    public var tools: [LocalAgentToolSnapshot]
    public var eventsTotal: UInt32
    public var eventsHasMore: Bool
    public var snapshotEventSequence: UInt64

    public init(
        run: LocalAgentRunSnapshot,
        events: [LocalAgentRunTimelineEvent],
        tools: [LocalAgentToolSnapshot],
        eventsTotal: UInt32,
        eventsHasMore: Bool,
        snapshotEventSequence: UInt64
    ) {
        self.run = run
        self.events = events
        self.tools = tools
        self.eventsTotal = eventsTotal
        self.eventsHasMore = eventsHasMore
        self.snapshotEventSequence = snapshotEventSequence
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
    public var runID: String
    public var pendingCount: UInt64
    public var failedCount: UInt64
    public var lastErrorCode: String?

    public init(
        runID: String,
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

    public init(code: String, message: String, retryable: Bool) {
        self.code = code
        self.message = message
        self.retryable = retryable
    }
}

public enum LocalAgentResponse: Equatable, Sendable {
    case accepted(operationID: String)
    case runCreated(operationID: String, run: LocalAgentRunSnapshot)
    case run(LocalAgentRunSnapshot)
    case runDetail(LocalAgentRunDetail)
    case task(LocalAgentTaskSnapshot)
    case taskGraph(LocalAgentTaskGraphSnapshot)
    case taskRunDetail(LocalAgentTaskRunDetail)
    case mainChatRunBinding(LocalAgentMainChatRunBinding)
    case runs([LocalAgentRunSnapshot], nextCursor: String?)
    case tasks([LocalAgentTaskSnapshot], nextCursor: String?)
    case project(LocalAgentProjectSnapshot)
    case projects([LocalAgentProjectSnapshot], nextCursor: String?)
    case clipboard(LocalAgentClipboardSnapshot)
    case clipboardRecords([LocalAgentClipboardSnapshot], nextCursor: String?)
    case clipboardMutation(LocalAgentClipboardMutationResult)
    case media(LocalAgentMediaSnapshot)
    case mediaRecords([LocalAgentMediaSnapshot], nextCursor: String?)
    case mediaMutation(LocalAgentMediaMutationResult)
    case story(LocalAgentStorySnapshot)
    case storyRecords([LocalAgentStorySnapshot], nextCursor: String?)
    case notepad(LocalAgentNotepadSnapshot)
    case notepadRecords([LocalAgentNotepadSnapshot], nextCursor: String?)
    case clientSetting(LocalAgentClientSettingSnapshot)
    case terminalHistory(LocalAgentTerminalHistorySnapshot)
    case terminalHistoryRecords([LocalAgentTerminalHistorySnapshot], nextCursor: String?)
    case approvalHistory(LocalAgentApprovalHistorySnapshot)
    case approvalHistoryRecords([LocalAgentApprovalHistorySnapshot], nextCursor: String?)
    case installedPlugin(LocalAgentInstalledPluginSnapshot)
    case installedPluginRecords([LocalAgentInstalledPluginSnapshot], nextCursor: String?)
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
    private struct Projects: Decodable {
        let projects: [LocalAgentProjectSnapshot]
        let nextCursor: String?
    }
    private struct ClipboardRecords: Decodable {
        let entries: [LocalAgentClipboardSnapshot]
        let nextCursor: String?
    }
    private struct MediaRecords: Decodable {
        let records: [LocalAgentMediaSnapshot]
        let nextCursor: String?
    }
    private struct StoryRecords: Decodable {
        let records: [LocalAgentStorySnapshot]
        let nextCursor: String?
    }
    private struct NotepadRecords: Decodable {
        let records: [LocalAgentNotepadSnapshot]
        let nextCursor: String?
    }
    private struct TerminalHistoryRecords: Decodable {
        let records: [LocalAgentTerminalHistorySnapshot]
        let nextCursor: String?
    }
    private struct ApprovalHistoryRecords: Decodable {
        let records: [LocalAgentApprovalHistorySnapshot]
        let nextCursor: String?
    }
    private struct InstalledPluginRecords: Decodable {
        let records: [LocalAgentInstalledPluginSnapshot]
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
        case "run_detail":
            self = .runDetail(try container.decode(LocalAgentRunDetail.self, forKey: .payload))
        case "task": self = .task(try container.decode(LocalAgentTaskSnapshot.self, forKey: .payload))
        case "task_graph":
            self = .taskGraph(
                try container.decode(LocalAgentTaskGraphSnapshot.self, forKey: .payload)
            )
        case "task_run_detail":
            self = .taskRunDetail(
                try container.decode(LocalAgentTaskRunDetail.self, forKey: .payload)
            )
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
        case "project":
            self = .project(
                try container.decode(LocalAgentProjectSnapshot.self, forKey: .payload)
            )
        case "projects":
            let value = try container.decode(Projects.self, forKey: .payload)
            self = .projects(value.projects, nextCursor: value.nextCursor)
        case "clipboard":
            self = .clipboard(
                try container.decode(LocalAgentClipboardSnapshot.self, forKey: .payload)
            )
        case "clipboard_records":
            let value = try container.decode(ClipboardRecords.self, forKey: .payload)
            self = .clipboardRecords(value.entries, nextCursor: value.nextCursor)
        case "clipboard_mutation":
            self = .clipboardMutation(
                try container.decode(LocalAgentClipboardMutationResult.self, forKey: .payload)
            )
        case "media":
            self = .media(
                try container.decode(LocalAgentMediaSnapshot.self, forKey: .payload)
            )
        case "media_records":
            let value = try container.decode(MediaRecords.self, forKey: .payload)
            self = .mediaRecords(value.records, nextCursor: value.nextCursor)
        case "media_mutation":
            self = .mediaMutation(
                try container.decode(LocalAgentMediaMutationResult.self, forKey: .payload)
            )
        case "story":
            self = .story(
                try container.decode(LocalAgentStorySnapshot.self, forKey: .payload)
            )
        case "story_records":
            let value = try container.decode(StoryRecords.self, forKey: .payload)
            self = .storyRecords(value.records, nextCursor: value.nextCursor)
        case "notepad":
            self = .notepad(
                try container.decode(LocalAgentNotepadSnapshot.self, forKey: .payload)
            )
        case "notepad_records":
            let value = try container.decode(NotepadRecords.self, forKey: .payload)
            self = .notepadRecords(value.records, nextCursor: value.nextCursor)
        case "client_setting":
            self = .clientSetting(
                try container.decode(LocalAgentClientSettingSnapshot.self, forKey: .payload)
            )
        case "terminal_history":
            self = .terminalHistory(
                try container.decode(LocalAgentTerminalHistorySnapshot.self, forKey: .payload)
            )
        case "terminal_history_records":
            let value = try container.decode(TerminalHistoryRecords.self, forKey: .payload)
            self = .terminalHistoryRecords(value.records, nextCursor: value.nextCursor)
        case "approval_history":
            self = .approvalHistory(
                try container.decode(LocalAgentApprovalHistorySnapshot.self, forKey: .payload)
            )
        case "approval_history_records":
            let value = try container.decode(ApprovalHistoryRecords.self, forKey: .payload)
            self = .approvalHistoryRecords(value.records, nextCursor: value.nextCursor)
        case "installed_plugin":
            self = .installedPlugin(
                try container.decode(LocalAgentInstalledPluginSnapshot.self, forKey: .payload)
            )
        case "installed_plugin_records":
            let value = try container.decode(InstalledPluginRecords.self, forKey: .payload)
            self = .installedPluginRecords(value.records, nextCursor: value.nextCursor)
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
