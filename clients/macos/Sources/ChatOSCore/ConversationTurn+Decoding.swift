import Foundation

extension ConversationTurn {
    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            id: try values.decode(String.self, forKey: .id),
            sessionID: try values.decode(String.self, forKey: .sessionID),
            sequence: try values.decode(Int64.self, forKey: .sequence),
            revision: try values.decode(Int64.self, forKey: .revision),
            userMessage: try values.decode(ChatMessage.self, forKey: .userMessage),
            additionalUserMessages: try values.decodeIfPresent(
                [ChatMessage].self, forKey: .additionalUserMessages
            ) ?? [],
            processEvents: try values.decode([TurnProcessEvent].self, forKey: .processEvents),
            finalAssistantMessage: try values.decodeIfPresent(ChatMessage.self, forKey: .finalAssistantMessage),
            assistantReplies: try values.decode([ConversationAssistantReply].self, forKey: .assistantReplies),
            messageTaskLookup: try values.decodeIfPresent(MessageTaskLookup.self, forKey: .messageTaskLookup),
            isTaskGraphAvailable: try values.decode(Bool.self, forKey: .isTaskGraphAvailable),
            status: try values.decode(TurnStatus.self, forKey: .status),
            startedAt: try values.decode(Date.self, forKey: .startedAt),
            completedAt: try values.decodeIfPresent(Date.self, forKey: .completedAt)
        )
    }
}
