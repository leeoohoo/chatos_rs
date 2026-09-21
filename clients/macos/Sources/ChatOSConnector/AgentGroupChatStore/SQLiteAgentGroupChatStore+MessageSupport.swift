import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    /// Persists the Human's proposal decision as ordinary unread communication for the Agent
    /// that submitted it. The message and its delivery are written in the same transaction as
    /// the proposal state change, so the Agent cannot miss a successful decision after a crash.
    func enqueueProposalResolutionNotification(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        proposalID: String,
        proposalLabel: String,
        approved: Bool,
        nowUnixMs: Int64
    ) throws {
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
              try readMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: proposerAgentID
              )?.status == .active else { return }

        let decision = approved ? "批准" : "拒绝"
        let followUp = approved
            ? "该决定已经生效。请重新读取工作区快照确认最新状态，并在当前会话回复 Human 后继续处理需要跟进的事项；不要等待 Human 再次提醒。"
            : "请在当前会话确认你已知晓，不要再次提交相同提案；如需替代方案，先根据 Human 的最新要求调整。"
        let content = "Human 已\(decision)你提交的\(proposalLabel)（proposal_id: \(proposalID)）。\(followUp)"
        let messageID = UUID().uuidString.lowercased()
        try execute(
            """
            INSERT INTO project_agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_id, content,
                reply_to_message_id, source_run_id, causation_id, root_message_id,
                hop_count, created_at_unix_ms
            ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, ?, ?, 0, ?)
            """,
            [
                .text(ownerUserID), .text(messageID), .text(roomID), .text(content),
                .text(proposalID), .text(messageID), .integer(nowUnixMs),
            ]
        )
        try execute(
            """
            INSERT INTO project_agent_message_mentions (
                owner_user_id, message_id, agent_id, position
            ) VALUES (?, ?, ?, 0)
            """,
            [.text(ownerUserID), .text(messageID), .text(proposerAgentID)]
        )
        let deliveryID = UUID().uuidString.lowercased()
        try execute(
            """
            INSERT INTO project_agent_deliveries (
                owner_user_id, id, room_id, message_id, root_message_id,
                target_agent_id, trigger_kind, status, attempt, hop_count,
                deduplication_key, response_message_id, last_error,
                claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, ?, 'mention', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
            """,
            [
                .text(ownerUserID), .text(deliveryID), .text(roomID), .text(messageID),
                .text(messageID), .text(proposerAgentID),
                .text("proposal-resolution:\(proposalID):\(proposerAgentID)"),
                .integer(nowUnixMs),
            ]
        )
    }

    func requireMessage(ownerUserID: String, roomID: String, messageID: String) throws {
        guard try readMessage(ownerUserID: ownerUserID, messageID: messageID)?.roomID == roomID else {
            throw AgentGroupChatError.invalidField("messageReference")
        }
    }

    func validateSender(
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft
    ) throws {
        switch draft.senderKind {
        case .human:
            guard draft.senderID == ownerUserID else { throw AgentGroupChatError.permissionDenied }
        case .agent:
            guard try readMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: draft.senderID
            )?.status == .active else { throw AgentGroupChatError.notMember }
        case .system:
            guard draft.senderID == "system" else { throw AgentGroupChatError.permissionDenied }
        }
    }

    func validateOwnerRoomAgent(ownerUserID: String, roomID: String, agentID: String) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
    }

    func validateDeliveryMutation(
        ownerUserID: String,
        deliveryID: String,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
    }

    func readMessage(_ statement: OpaquePointer) throws -> ProjectAgentMessage {
        guard let senderKind = ProjectAgentMessageSenderKind(rawValue: Self.string(statement, 3)) else {
            throw AgentGroupChatError.storage("invalid message sender kind")
        }
        let messageID = Self.string(statement, 1)
        let ownerUserID = Self.string(statement, 0)
        let mentions = try AgentMessageRepository.mentions(
            database,
            ownerUserID: ownerUserID,
            messageID: messageID,
            preparedStatement: recordPreparedStatement
        )
        let attachments = try AgentMessageRepository.attachments(
            database,
            ownerUserID: ownerUserID,
            messageID: messageID,
            preparedStatement: recordPreparedStatement
        )
        let draft = ProjectAgentMessageDraft(
            senderKind: senderKind,
            senderID: Self.string(statement, 4),
            content: Self.string(statement, 5),
            mentionedAgentIDs: mentions,
            replyToMessageID: Self.optionalString(statement, 6),
            sourceRunID: Self.optionalString(statement, 7),
            causationID: Self.optionalString(statement, 8),
            rootMessageID: Self.string(statement, 9),
            hopCount: Int(sqlite3_column_int64(statement, 10))
        )
        return ProjectAgentMessage(
            id: messageID,
            ownerUserID: ownerUserID,
            roomID: Self.string(statement, 2),
            draft: draft,
            rootMessageID: Self.string(statement, 9),
            attachments: attachments,
            createdAtUnixMs: sqlite3_column_int64(statement, 11)
        )
    }

    func persistMessageAttachments(
        ownerUserID: String,
        messageID: String,
        drafts: [ProjectAgentMessageAttachmentDraft],
        queueAgentArtifacts: Bool
    ) throws -> [ProjectAgentMessageAttachment] {
        guard !drafts.isEmpty else { return [] }
        let directory = attachmentDirectoryURL(messageID: messageID)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        var attachments: [ProjectAgentMessageAttachment] = []
        for (position, draft) in drafts.enumerated() {
            let attachmentID = UUID().uuidString.lowercased()
            let relativePath = "\(messageID)/\(attachmentID)"
            let fileURL = try attachmentFileURL(relativePath: relativePath)
            try draft.data.write(to: fileURL, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: fileURL.path
            )
            let sha256 = Self.sha256(draft.data)
            let shouldQueue = queueAgentArtifacts
                && draft.mimeType.lowercased().hasPrefix("text/markdown")
                && draft.data.count <= AgentCommunicationPolicy.standard.maximumDocumentBytes
            let syncStatus: ProjectAgentMessageAttachmentSyncStatus = shouldQueue
                ? .queued
                : .localOnly
            let attachment = ProjectAgentMessageAttachment(
                id: attachmentID,
                name: draft.name,
                mimeType: draft.mimeType,
                size: draft.data.count,
                kind: draft.kind,
                origin: draft.origin,
                sha256: sha256,
                syncStatus: syncStatus
            )
            try execute(
                """
                INSERT INTO project_agent_message_attachments (
                    owner_user_id, message_id, id, position, name, mime_type,
                    size_bytes, kind, origin, relative_path, sha256, sync_status,
                    upload_attempt, next_retry_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0)
                """,
                [
                    .text(ownerUserID), .text(messageID), .text(attachmentID),
                    .integer(Int64(position)), .text(draft.name), .text(draft.mimeType),
                    .integer(Int64(draft.data.count)), .text(draft.kind.rawValue),
                    .text(draft.origin.rawValue), .text(relativePath), .text(sha256),
                    .text(syncStatus.rawValue),
                ]
            )
            attachments.append(attachment)
        }
        return attachments
    }

    func attachmentDirectoryURL(messageID: String) -> URL {
        attachmentsRootURL.appendingPathComponent(messageID, isDirectory: true)
    }

    func attachmentFileURL(relativePath: String) throws -> URL {
        guard !relativePath.isEmpty,
              !relativePath.hasPrefix("/"),
              !relativePath.split(separator: "/").contains("..") else {
            throw AgentGroupChatError.storage("invalid message attachment path")
        }
        let root = attachmentsRootURL.standardizedFileURL
        let candidate = root.appendingPathComponent(relativePath).standardizedFileURL
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        guard candidate.path.hasPrefix(prefix) else {
            throw AgentGroupChatError.storage("invalid message attachment path")
        }
        return candidate
    }

    static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

}
