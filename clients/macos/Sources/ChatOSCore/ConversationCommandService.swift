import Foundation

public struct ConversationSendCommand: Sendable, Equatable {
    public var sessionID: String
    public var turnID: String
    public var messageID: String
    public var content: String
    public var attachments: [ConversationAttachmentDraft]
    public var reasoningEnabled: Bool?

    public init(
        sessionID: String,
        turnID: String,
        messageID: String,
        content: String,
        attachments: [ConversationAttachmentDraft] = [],
        reasoningEnabled: Bool? = nil
    ) {
        self.sessionID = sessionID
        self.turnID = turnID
        self.messageID = messageID
        self.content = content
        self.attachments = attachments
        self.reasoningEnabled = reasoningEnabled
    }
}

public struct ConversationCommandAck: Sendable, Equatable {
    public var operationID: String
    public var runID: String
    public var turnID: String
    public var userMessageID: String

    public init(operationID: String, runID: String, turnID: String, userMessageID: String) {
        self.operationID = operationID
        self.runID = runID
        self.turnID = turnID
        self.userMessageID = userMessageID
    }
}

public protocol ConversationCommandServicing: Sendable {
    func sendNewTurn(_ command: ConversationSendCommand) async throws -> ConversationCommandAck
    func cancelRun(runID: String) async throws
}
