import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension LocalAgentChatToolProvider {
    static let maximumReadableAttachmentBytes = 20 * 1_024 * 1_024

    func readMessages(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let beforeReference = try Self.optionalString(arguments, key: "before_message_ref")
        let beforeMessageID: String?
        if let beforeReference {
            guard let authority = await references.messageAuthority(reference: beforeReference),
                  authority.roomID == context.roomID else {
                return Self.structuredFailure(
                    code: "invalid_message_ref",
                    field: "before_message_ref",
                    message: "消息游标无效或已经过期，请从最近一页重新读取。",
                    retryable: true,
                    nextTool: Self.readMessagesToolName
                )
            }
            beforeMessageID = authority.messageID
        } else {
            beforeMessageID = nil
        }
        let limit = try Self.optionalInteger(arguments, key: "limit").map(Int.init) ?? 50
        let page = try await store.pageRecentMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            beforeMessageID: beforeMessageID,
            limit: limit
        )
        let profiles = try await store.listAgents(
            ownerUserID: context.ownerUserID,
            includeArchived: true
        )
        let profilesByID = Dictionary(uniqueKeysWithValues: profiles.map { ($0.id, $0) })
        var messages: [MessageResponse] = []
        for message in page.messages {
            messages.append(await messageResponse(message, profiles: profilesByID))
        }
        let nextReference: String?
        if page.hasMore, let messageID = page.nextCursorMessageID {
            nextReference = await references.messageReference(
                roomID: context.roomID,
                messageID: messageID
            )
        } else { nextReference = nil }
        return try Self.outcome(MessagePageResponse(
            messages: messages,
            nextCursorReference: nextReference,
            hasMore: page.hasMore,
            readThroughReference: nil
        ))
    }

    func readAttachment(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let messageReference = try Self.requiredString(arguments, key: "message_ref")
        let attachmentReference = try Self.requiredString(arguments, key: "attachment_ref")
        guard let messageAuthority = await references.messageAuthority(
            reference: messageReference
        ), let attachmentAuthority = await references.attachmentAuthority(
            reference: attachmentReference
        ),
        attachmentAuthority.roomID == messageAuthority.roomID,
        attachmentAuthority.messageID == messageAuthority.messageID else {
            return Self.structuredFailure(
                code: "invalid_attachment_ref",
                field: "attachment_ref",
                message: "附件引用无效、已经过期或不属于所选消息，请重新读取消息。",
                retryable: true,
                nextTool: Self.readMessagesToolName
            )
        }
        let messageID = messageAuthority.messageID
        let attachmentID = attachmentAuthority.attachmentID
        let offset = max(0, Int(try Self.optionalInteger(arguments, key: "offset") ?? 0))
        let limit = min(
            12_000,
            max(1, Int(try Self.optionalInteger(arguments, key: "limit") ?? 12_000))
        )
        guard let payload = try await store.messageAttachment(
            ownerUserID: context.ownerUserID,
            roomID: messageAuthority.roomID,
            messageID: messageID,
            attachmentID: attachmentID
        ) else { throw AgentGroupChatError.notFound }
        let data = try Self.boundedAttachmentData(
            at: payload.localFileURL,
            expectedBytes: payload.attachment.size
        )
        var response: [String: NativeJSONValue] = [
            "message_ref": .string(messageReference),
            "attachment_ref": .string(attachmentReference),
            "name": .string(payload.attachment.name),
            "mime_type": .string(payload.attachment.mimeType),
            "kind": .string(payload.attachment.kind.rawValue),
            "size": .number(Double(payload.attachment.size)),
        ]
        if !data.prefix(8_000).contains(0), let text = String(data: data, encoding: .utf8) {
            let slice = Self.attachmentTextSlice(text, offset: offset, limit: limit)
            response["content"] = .string(slice.content)
            response["offset"] = .number(Double(slice.offset))
            response["next_offset"] = slice.hasMore ? .number(Double(slice.nextOffset)) : .null
            response["has_more"] = .bool(slice.hasMore)
        } else {
            response["content"] = .null
            response["multimodal_on_trigger"] = .bool(messageID == context.triggerMessageID)
            response["note"] = .string(
                messageID == context.triggerMessageID
                    ? "该二进制附件已作为当前触发消息的多模态输入提供给模型。"
                    : "该二进制附件不能作为文本读取；请让 Human 在新消息中重新附带，或使用匹配的本机 Plugin。"
            )
        }
        return try Self.outcome(response)
    }

    static func attachmentTextSlice(
        _ text: String,
        offset: Int,
        limit: Int
    ) -> (content: String, offset: Int, nextOffset: Int, hasMore: Bool) {
        let requestedOffset = max(0, offset)
        let start = text.index(
            text.startIndex,
            offsetBy: requestedOffset,
            limitedBy: text.endIndex
        ) ?? text.endIndex
        let actualOffset = start == text.endIndex
            ? text.distance(from: text.startIndex, to: text.endIndex)
            : requestedOffset
        let end = text.index(
            start,
            offsetBy: max(1, limit),
            limitedBy: text.endIndex
        ) ?? text.endIndex
        let nextOffset = actualOffset + text.distance(from: start, to: end)
        return (
            content: String(text[start..<end]),
            offset: actualOffset,
            nextOffset: nextOffset,
            hasMore: end < text.endIndex
        )
    }

    private static func boundedAttachmentData(at url: URL, expectedBytes: Int) throws -> Data {
        guard expectedBytes > 0,
              expectedBytes <= maximumReadableAttachmentBytes,
              let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
              values.isRegularFile == true,
              values.fileSize == expectedBytes,
              let handle = try? FileHandle(forReadingFrom: url) else {
            throw AgentGroupChatError.invalidField("attachment.data")
        }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: maximumReadableAttachmentBytes + 1),
              data.count == expectedBytes else {
            throw AgentGroupChatError.invalidField("attachment.data")
        }
        return data
    }

}
