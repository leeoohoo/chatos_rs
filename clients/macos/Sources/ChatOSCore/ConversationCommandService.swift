import Foundation

public struct ConversationSendCommand: Sendable, Equatable {
    public var sessionID: String
    public var turnID: String
    public var content: String
    public var attachments: [ConversationAttachmentDraft]
    public var reasoningEnabled: Bool?
    public var planModeEnabled: Bool?

    public init(
        sessionID: String,
        turnID: String,
        content: String,
        attachments: [ConversationAttachmentDraft] = [],
        reasoningEnabled: Bool? = nil,
        planModeEnabled: Bool? = nil
    ) {
        self.sessionID = sessionID
        self.turnID = turnID
        self.content = content
        self.attachments = attachments
        self.reasoningEnabled = reasoningEnabled
        self.planModeEnabled = planModeEnabled
    }
}

public struct ConversationCommandAck: Sendable, Equatable {
    public var accepted: Bool
    public var turnID: String
    public var userMessageID: String?

    public init(accepted: Bool, turnID: String, userMessageID: String?) {
        self.accepted = accepted
        self.turnID = turnID
        self.userMessageID = userMessageID
    }
}

public enum ConversationCommandError: Error, Sendable, Equatable {
    case guidanceTargetInactive
}

extension ConversationCommandError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .guidanceTargetInactive:
            "当前执行轮次已经结束，将作为一条新消息发送。"
        }
    }
}

public protocol ConversationCommandServicing: Sendable {
    func sendNewTurn(_ command: ConversationSendCommand) async throws -> ConversationCommandAck
    func sendGuidance(_ command: ConversationSendCommand) async throws -> ConversationCommandAck
}
