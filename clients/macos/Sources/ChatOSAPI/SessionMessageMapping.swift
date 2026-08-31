import ChatOSCore
import Foundation

extension SessionMessageDTO {
    var resolvedTurnID: String {
        turnID?.nonEmpty
            ?? metadata.value(at: "historyProcess", "turnId")?.stringValue
            ?? metadata.value(at: "conversation_turn_id")?.stringValue
            ?? metadata.value(at: "task_runner_async", "source_turn_id")?.stringValue
            ?? id
    }

    var finalTurnID: String? {
        metadata.value(at: "historyFinalForTurnId")?.stringValue
            ?? metadata.value(at: "conversation_turn_id")?.stringValue
            ?? metadata.value(at: "task_runner_async", "source_turn_id")?.stringValue
            ?? turnID?.nonEmpty
    }

    var resolvedRevision: Int64 {
        if let protocolRevision, protocolRevision > 0 {
            return protocolRevision
        }
        let date = DateParser.parse(updatedAt ?? createdAt)
        return max(1, Int64((date?.timeIntervalSince1970 ?? 0) * 1_000))
    }

    func domainMessage(role: ChatMessage.Role, fallbackDate: Date) -> ChatMessage {
        ChatMessage(
            id: id,
            role: role,
            text: content,
            createdAt: DateParser.parse(createdAt) ?? fallbackDate,
            attachments: resolvedAttachments
        )
    }

    private var resolvedAttachments: [ConversationAttachmentReference] {
        guard case let .array(values) = metadata.value(at: "attachments") else { return [] }
        return values.enumerated().compactMap { index, value in
            guard case let .object(object) = value else { return nil }
            let name = object.string("name") ?? "附件 \(index + 1)"
            let mimeType = object.string("mimeType", "mime") ?? "application/octet-stream"
            let kind = ConversationAttachmentKind(
                rawValue: object.string("type") ?? "file"
            ) ?? .file
            return ConversationAttachmentReference(
                id: object.string("id") ?? "\(id)-attachment-\(index)",
                name: name,
                mimeType: mimeType,
                size: object.integer("size") ?? 0,
                kind: kind,
                storageProvider: object.string("storageProvider", "storage_provider"),
                bucket: object.string("bucket"),
                objectKey: object.string("objectKey", "object_key"),
                url: object.string("url"),
                viewURL: object.string("viewUrl", "view_url")
            )
        }
    }
}

enum DateParser {
    static func parse(_ value: String?) -> Date? {
        guard let value = value?.nonEmpty else { return nil }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = fractional.date(from: value) {
            return date
        }
        return ISO8601DateFormatter().date(from: value)
    }
}

private extension String {
    var nonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}

private extension Dictionary where Key == String, Value == JSONValue {
    func string(_ keys: String...) -> String? {
        for key in keys {
            if let value = self[key]?.stringValue { return value }
        }
        return nil
    }

    func integer(_ key: String) -> Int? {
        self[key]?.intValue
    }
}
