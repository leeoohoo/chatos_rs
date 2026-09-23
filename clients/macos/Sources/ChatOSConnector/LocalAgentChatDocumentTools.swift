import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

extension LocalAgentChatToolProvider {
    func createDocument(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let rawName = try Self.requiredString(arguments, key: "name")
        let title = try Self.requiredString(arguments, key: "title")
        let markdown = try Self.requiredString(arguments, key: "markdown")
        guard let name = Self.sanitizedMarkdownDocumentName(rawName) else {
            await recordDocumentCreation(.invalidName)
            return Self.structuredFailure(
                code: "invalid_document_name",
                field: "name",
                message: "文档名称不能为空；客户端会自动清洗路径字符并补充 .md 后缀。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            )
        }
        guard !title.isEmpty,
              title == title.trimmingCharacters(in: .whitespacesAndNewlines),
              title.count <= 512,
              title.rangeOfCharacter(from: .controlCharacters) == nil else {
            await recordDocumentCreation(.invalidTitle)
            return Self.structuredFailure(
                code: "invalid_document_title",
                field: "title",
                message: "文档标题必须是 1～512 个字符，且不能包含控制字符或首尾空白。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            )
        }
        guard !markdown.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            await recordDocumentCreation(.empty)
            return Self.structuredFailure(
                code: "empty_document",
                field: "markdown",
                message: "Markdown 文档不能为空。",
                retryable: true,
                nextTool: Self.createDocumentToolName
            )
        }
        let data = Data(markdown.utf8)
        do {
            let created = try await references.createDocument(
                name: name,
                title: title,
                data: data
            )
            await recordDocumentCreation(.succeeded, bytes: created.size)
            return try Self.outcome(DocumentCreateResponse(
                documentReference: created.reference,
                name: name,
                title: title,
                size: created.size,
                mimeType: "text/markdown",
                sha256: created.sha256,
                instruction: "请在下一次发送消息时通过 document_refs 附加该文档。"
            ))
        } catch let failure as LocalAgentRunReferenceVault.DocumentCreateFailure {
            switch failure {
            case .empty:
                await recordDocumentCreation(.empty)
                return Self.structuredFailure(
                    code: "empty_document",
                    field: "markdown",
                    message: "Markdown 文档不能为空。",
                    retryable: true,
                    nextTool: Self.createDocumentToolName
                )
            case .tooLarge:
                await recordDocumentCreation(.tooLarge, bytes: data.count)
                return Self.structuredFailure(
                    code: "document_too_large",
                    field: "markdown",
                    message: "单个文档超过 \(AgentCommunicationPolicy.standard.maximumDocumentBytes) 字节，请拆分为少量有意义的 Markdown 文档。",
                    retryable: true,
                    nextTool: Self.createDocumentToolName
                )
            case .tooMany:
                await recordDocumentCreation(.tooMany, bytes: data.count)
                return Self.structuredFailure(
                    code: "too_many_documents",
                    field: nil,
                    message: "当前 Run 已达到最多 \(AgentCommunicationPolicy.standard.maximumDocumentsPerRun) 个文档。",
                    retryable: false
                )
            case .runTooLarge:
                await recordDocumentCreation(.runLimitExceeded, bytes: data.count)
                return Self.structuredFailure(
                    code: "document_run_limit_exceeded",
                    field: "markdown",
                    message: "当前 Run 创建的文档总量超过 \(AgentCommunicationPolicy.standard.maximumDocumentBytesPerRun) 字节。",
                    retryable: false
                )
            case .storage:
                await recordDocumentCreation(.storageFailed, bytes: data.count)
                return Self.structuredFailure(
                    code: "document_storage_failed",
                    field: nil,
                    message: "客户端无法安全保存本地文档草稿。",
                    retryable: true,
                    nextTool: Self.createDocumentToolName
                )
            }
        }
    }

    func markRead(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let throughReference = try Self.requiredString(arguments, key: "through_message_ref")
        guard let authority = await references.messageAuthority(reference: throughReference),
              authority.roomID == context.roomID else {
            return Self.structuredFailure(
                code: "invalid_message_ref",
                field: "through_message_ref",
                message: "已读消息引用无效或已经过期，请重新读取当前会话未读。",
                retryable: true,
                nextTool: Self.readUnreadToolName
            )
        }
        let cursor = try await store.markMessagesRead(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            throughMessageID: authority.messageID,
            nowUnixMs: now()
        )
        let remaining = try await store.listUnreadMessages(
            ownerUserID: context.ownerUserID,
            roomID: context.roomID,
            agentID: context.agentID,
            limit: 1
        )
        let nextUnreadMessageReference: String?
        if let messageID = remaining.messages.first?.id {
            nextUnreadMessageReference = await references.messageReference(
                roomID: context.roomID,
                messageID: messageID
            )
        } else { nextUnreadMessageReference = nil }
        return try Self.outcome(MarkReadResponse(
            throughMessageReference: await references.messageReference(
                roomID: context.roomID,
                messageID: cursor.messageID
            ),
            hasUnread: !remaining.messages.isEmpty,
            nextUnreadMessageReference: nextUnreadMessageReference
        ))
    }

}
