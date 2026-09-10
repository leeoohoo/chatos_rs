import ChatOSCore
import Foundation

extension SessionMessageDTO {
    var messageTaskLookup: MessageTaskLookup? {
        let taskRunner = metadata.object(at: "task_runner_async")
        let sourceUserMessageID = taskRunner?["source_user_message_id"]?.stringValue
        let usableSourceID = sourceUserMessageID?.hasPrefix("temp_") == true
            ? nil
            : sourceUserMessageID
        let sourceTurnID = metadata["conversation_turn_id"]?.stringValue
            ?? taskRunner?["source_turn_id"]?.stringValue
        guard usableSourceID != nil || sourceTurnID != nil else { return nil }
        return MessageTaskLookup(
            sessionID: conversationID,
            turnID: sourceTurnID,
            sourceUserMessageID: usableSourceID
        )
    }
}

private extension Dictionary where Key == String, Value == JSONValue {
    func object(at key: String) -> [String: JSONValue]? {
        guard case let .object(value) = self[key] else { return nil }
        return value
    }
}
