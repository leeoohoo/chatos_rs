import Foundation

public extension ConversationTurn {
    var resolvedMessageTaskLookup: MessageTaskLookup {
        MessageTaskLookup(
            sessionID: messageTaskLookup?.sessionID ?? sessionID,
            turnID: messageTaskLookup?.turnID ?? id,
            sourceUserMessageID: messageTaskLookup?.sourceUserMessageID
                ?? userMessage.id
        )
    }
}
