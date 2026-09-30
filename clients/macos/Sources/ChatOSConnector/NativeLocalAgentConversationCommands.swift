import Foundation

public struct LocalAgentConversationAttachmentSpec: Encodable, Sendable, Equatable {
    public let attachmentID: String
    public let displayName: String
    public let mediaType: String
    public let byteSize: UInt64
    public let sha256: String
    public let authorizedLocalRef: String
    public let metadata: LocalAgentJSONValue

    public init(
        attachmentID: String,
        displayName: String,
        mediaType: String,
        byteSize: UInt64,
        sha256: String,
        authorizedLocalRef: String,
        metadata: LocalAgentJSONValue = .object([:])
    ) {
        self.attachmentID = attachmentID
        self.displayName = displayName
        self.mediaType = mediaType
        self.byteSize = byteSize
        self.sha256 = sha256
        self.authorizedLocalRef = authorizedLocalRef
        self.metadata = metadata
    }

    private enum CodingKeys: String, CodingKey {
        case attachmentID = "attachment_id"
        case displayName = "display_name"
        case mediaType = "media_type"
        case byteSize = "byte_size"
        case sha256
        case authorizedLocalRef = "authorized_local_ref"
        case metadata
    }
}

public struct LocalAgentConversationTurnStartInput: Sendable, Equatable {
    public let ownerUserID: String
    public let conversationID: String
    public let expectedConversationVersion: UInt64
    public let turnID: String
    public let messageID: String
    public let runID: String
    public let message: String
    public let messageMetadata: LocalAgentJSONValue
    public let attachments: [LocalAgentConversationAttachmentSpec]
    public let modelConfigRef: String
    public let modelConfigRevision: String
    public let capabilityPolicyRevision: String
    public let maxIterations: UInt32

    public init(
        ownerUserID: String,
        conversationID: String,
        expectedConversationVersion: UInt64,
        turnID: String,
        messageID: String,
        runID: String,
        message: String,
        messageMetadata: LocalAgentJSONValue = .object([:]),
        attachments: [LocalAgentConversationAttachmentSpec] = [],
        modelConfigRef: String,
        modelConfigRevision: String,
        capabilityPolicyRevision: String,
        maxIterations: UInt32 = 32
    ) {
        self.ownerUserID = ownerUserID
        self.conversationID = conversationID
        self.expectedConversationVersion = expectedConversationVersion
        self.turnID = turnID
        self.messageID = messageID
        self.runID = runID
        self.message = message
        self.messageMetadata = messageMetadata
        self.attachments = attachments
        self.modelConfigRef = modelConfigRef
        self.modelConfigRevision = modelConfigRevision
        self.capabilityPolicyRevision = capabilityPolicyRevision
        self.maxIterations = maxIterations
    }
}

public struct LocalAgentConversationTurnUpdateInput: Sendable, Equatable {
    public let ownerUserID: String
    public let conversationID: String
    public let expectedConversationVersion: UInt64
    public let turnID: String
    public let expectedRunVersion: UInt64?
    public let messageID: String
    public let message: String
    public let messageMetadata: LocalAgentJSONValue
    public let attachments: [LocalAgentConversationAttachmentSpec]

    public init(
        ownerUserID: String,
        conversationID: String,
        expectedConversationVersion: UInt64,
        turnID: String,
        expectedRunVersion: UInt64?,
        messageID: String,
        message: String,
        messageMetadata: LocalAgentJSONValue = .object([:]),
        attachments: [LocalAgentConversationAttachmentSpec] = []
    ) {
        self.ownerUserID = ownerUserID
        self.conversationID = conversationID
        self.expectedConversationVersion = expectedConversationVersion
        self.turnID = turnID
        self.expectedRunVersion = expectedRunVersion
        self.messageID = messageID
        self.message = message
        self.messageMetadata = messageMetadata
        self.attachments = attachments
    }
}

public struct LocalAgentConversationTurnMutation: Decodable, Sendable, Equatable {
    public let conversation: LocalAgentConversationRecord
    public let turn: LocalAgentConversationTurnRecord
    public let message: LocalAgentConversationMessageRecord?
    public let attachments: [LocalAgentConversationAttachmentRecord]
}

public extension NativeLocalAgentConversationClient {
    func startTurn(
        _ input: LocalAgentConversationTurnStartInput
    ) async throws -> LocalAgentConversationTurnMutation {
        let response: TurnMutationResult = try await host.request(StartTurnCommand(input))
        try requireResult(response.type, expected: "conversation_turn_started")
        return response.result
    }

    func guideTurn(
        _ input: LocalAgentConversationTurnUpdateInput
    ) async throws -> LocalAgentConversationTurnMutation {
        let response: TurnMutationResult = try await host.request(GuideTurnCommand(input))
        try requireResult(response.type, expected: "conversation_turn_updated")
        return response.result
    }

    func resumeTurn(
        _ input: LocalAgentConversationTurnUpdateInput,
        expectedRunStatus: String,
        reason: String
    ) async throws -> LocalAgentConversationTurnMutation {
        guard let expectedRunVersion = input.expectedRunVersion else {
            throw NativeLocalAgentHostError.invalidCommand
        }
        let response: TurnMutationResult = try await host.request(ResumeTurnCommand(
            input,
            expectedRunVersion: expectedRunVersion,
            expectedRunStatus: expectedRunStatus,
            reason: reason
        ))
        try requireResult(response.type, expected: "conversation_turn_updated")
        return response.result
    }

    func cancelTurn(
        ownerUserID: String,
        conversationID: String,
        expectedConversationVersion: UInt64,
        turnID: String,
        expectedRunVersion: UInt64?,
        reason: String
    ) async throws -> LocalAgentConversationTurnMutation {
        let response: TurnMutationResult = try await host.request(CancelTurnCommand(
            type: "cancel_conversation_turn",
            ownerUserID: ownerUserID,
            conversationID: conversationID,
            expectedConversationVersion: expectedConversationVersion,
            turnID: turnID,
            expectedRunVersion: expectedRunVersion,
            reason: reason
        ))
        try requireResult(response.type, expected: "conversation_turn_updated")
        return response.result
    }

    private func requireResult(_ actual: String, expected: String) throws {
        guard actual == expected else { throw NativeLocalAgentHostError.invalidResponse }
    }
}

private struct StartTurnCommand: Encodable, Sendable {
    let type = "start_conversation_turn"
    let input: LocalAgentConversationTurnStartInput

    init(_ input: LocalAgentConversationTurnStartInput) { self.input = input }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: StartKeys.self)
        try container.encode(type, forKey: .type)
        try container.encode(input.ownerUserID, forKey: .ownerUserID)
        try container.encode(input.conversationID, forKey: .conversationID)
        try container.encode(input.expectedConversationVersion, forKey: .expectedConversationVersion)
        try container.encode(input.turnID, forKey: .turnID)
        try container.encode(input.messageID, forKey: .messageID)
        try container.encode(input.runID, forKey: .runID)
        try container.encode(input.message, forKey: .message)
        try container.encode(input.messageMetadata, forKey: .messageMetadata)
        try container.encode(input.attachments, forKey: .attachments)
        try container.encode(input.modelConfigRef, forKey: .modelConfigRef)
        try container.encode(input.modelConfigRevision, forKey: .modelConfigRevision)
        try container.encode(input.capabilityPolicyRevision, forKey: .capabilityPolicyRevision)
        try container.encode(input.maxIterations, forKey: .maxIterations)
    }
}

private struct GuideTurnCommand: Encodable, Sendable {
    let type = "guide_conversation_turn"
    let input: LocalAgentConversationTurnUpdateInput

    init(_ input: LocalAgentConversationTurnUpdateInput) { self.input = input }

    func encode(to encoder: Encoder) throws {
        try encodeUpdate(type: type, input: input, to: encoder)
    }
}

private struct ResumeTurnCommand: Encodable, Sendable {
    let type = "resume_conversation_turn"
    let input: LocalAgentConversationTurnUpdateInput
    let expectedRunVersion: UInt64
    let expectedRunStatus: String
    let reason: String

    init(
        _ input: LocalAgentConversationTurnUpdateInput,
        expectedRunVersion: UInt64,
        expectedRunStatus: String,
        reason: String
    ) {
        self.input = input
        self.expectedRunVersion = expectedRunVersion
        self.expectedRunStatus = expectedRunStatus
        self.reason = reason
    }

    func encode(to encoder: Encoder) throws {
        try encodeUpdate(type: type, input: input, to: encoder) { container in
            try container.encode(expectedRunVersion, forKey: .expectedRunVersion)
            try container.encode(expectedRunStatus, forKey: .expectedRunStatus)
            try container.encode(reason, forKey: .reason)
        }
    }
}

private struct CancelTurnCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let conversationID: String
    let expectedConversationVersion: UInt64
    let turnID: String
    let expectedRunVersion: UInt64?
    let reason: String

    private enum CodingKeys: String, CodingKey {
        case type, reason
        case ownerUserID = "owner_user_id"
        case conversationID = "conversation_id"
        case expectedConversationVersion = "expected_conversation_version"
        case turnID = "turn_id"
        case expectedRunVersion = "expected_run_version"
    }
}

private enum StartKeys: String, CodingKey {
    case type, message, attachments
    case ownerUserID = "owner_user_id"
    case conversationID = "conversation_id"
    case expectedConversationVersion = "expected_conversation_version"
    case turnID = "turn_id"
    case messageID = "message_id"
    case runID = "run_id"
    case messageMetadata = "message_metadata"
    case modelConfigRef = "model_config_ref"
    case modelConfigRevision = "model_config_revision"
    case capabilityPolicyRevision = "capability_policy_revision"
    case maxIterations = "max_iterations"
}

private enum UpdateKeys: String, CodingKey {
    case type, message, attachments, reason
    case ownerUserID = "owner_user_id"
    case conversationID = "conversation_id"
    case expectedConversationVersion = "expected_conversation_version"
    case turnID = "turn_id"
    case expectedRunVersion = "expected_run_version"
    case expectedRunStatus = "expected_run_status"
    case messageID = "message_id"
    case messageMetadata = "message_metadata"
}

private func encodeUpdate(
    type: String,
    input: LocalAgentConversationTurnUpdateInput,
    to encoder: Encoder,
    extra: ((inout KeyedEncodingContainer<UpdateKeys>) throws -> Void)? = nil
) throws {
    var container = encoder.container(keyedBy: UpdateKeys.self)
    try container.encode(type, forKey: .type)
    try container.encode(input.ownerUserID, forKey: .ownerUserID)
    try container.encode(input.conversationID, forKey: .conversationID)
    try container.encode(input.expectedConversationVersion, forKey: .expectedConversationVersion)
    try container.encode(input.turnID, forKey: .turnID)
    try container.encodeIfPresent(input.expectedRunVersion, forKey: .expectedRunVersion)
    try container.encode(input.messageID, forKey: .messageID)
    try container.encode(input.message, forKey: .message)
    try container.encode(input.messageMetadata, forKey: .messageMetadata)
    try container.encode(input.attachments, forKey: .attachments)
    try extra?(&container)
}

private struct TurnMutationResult: Decodable, Sendable {
    let type: String
    let result: LocalAgentConversationTurnMutation
}
