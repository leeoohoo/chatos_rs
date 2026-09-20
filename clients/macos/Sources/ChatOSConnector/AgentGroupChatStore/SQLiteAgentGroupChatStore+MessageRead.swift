import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func listMessages(
        ownerUserID: String,
        roomID: String,
        afterUnixMs: Int64? = nil,
        limit: Int = 100
    ) throws -> [ProjectAgentMessage] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        if let afterUnixMs {
            guard afterUnixMs >= 0 else { throw AgentGroupChatError.invalidField("afterUnixMs") }
        }
        return try AgentMessageRepository.list(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            afterUnixMs: afterUnixMs,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessage
        )
    }

    public func pageMessages(
        ownerUserID: String,
        roomID: String,
        afterMessageID: String? = nil,
        limit: Int = 100
    ) throws -> ProjectAgentMessagePage {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        var messageCursor: AgentMessageRepository.Cursor?
        if let afterMessageID {
            try AgentGroupChatValidation.identifier(afterMessageID, field: "afterMessageID")
            guard let cursor = try readMessage(ownerUserID: ownerUserID, messageID: afterMessageID),
                  cursor.roomID == roomID else {
                throw AgentGroupChatError.notFound
            }
            messageCursor = .init(
                createdAtUnixMs: cursor.createdAtUnixMs,
                messageID: cursor.id
            )
        }
        let loaded = try AgentMessageRepository.pageForward(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            after: messageCursor,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessage
        )
        let messages = Array(loaded.prefix(limit))
        return .init(
            messages: messages,
            nextCursorMessageID: messages.last?.id,
            hasMore: loaded.count > limit
        )
    }

    public func pageRecentMessages(
        ownerUserID: String,
        roomID: String,
        beforeMessageID: String? = nil,
        limit: Int = 100
    ) throws -> ProjectAgentMessagePage {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        var messageCursor: AgentMessageRepository.Cursor?
        if let beforeMessageID {
            try AgentGroupChatValidation.identifier(beforeMessageID, field: "beforeMessageID")
            guard let cursor = try readMessage(ownerUserID: ownerUserID, messageID: beforeMessageID),
                  cursor.roomID == roomID else {
                throw AgentGroupChatError.notFound
            }
            messageCursor = .init(
                createdAtUnixMs: cursor.createdAtUnixMs,
                messageID: cursor.id
            )
        }
        let loaded = try AgentMessageRepository.pageBackward(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            before: messageCursor,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessage
        )
        let newestFirst = Array(loaded.prefix(limit))
        let messages = Array(newestFirst.reversed())
        return .init(
            messages: messages,
            nextCursorMessageID: messages.first?.id,
            hasMore: loaded.count > limit
        )
    }

    public func listUnreadMessages(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        limit: Int = 50
    ) throws -> ProjectAgentUnreadPage {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        guard (1...100).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
              try readMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID
              )?.status == .active else {
            throw AgentGroupChatError.notMember
        }
        let cursor = try readCursor(
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agentID
        )
        let messageCursor = cursor.map {
            AgentMessageRepository.Cursor(
                createdAtUnixMs: $0.messageCreatedAtUnixMs,
                messageID: $0.messageID
            )
        }
        let loaded = try AgentMessageRepository.listUnread(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agentID,
            after: messageCursor,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessage
        )
        let messages = Array(loaded.prefix(limit))
        return .init(
            messages: messages,
            nextCursorMessageID: messages.last?.id,
            hasMore: loaded.count > limit,
            readThroughMessageID: cursor?.messageID
        )
    }

    public func markMessagesRead(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        throughMessageID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentReadCursor {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        try AgentGroupChatValidation.identifier(throughMessageID, field: "throughMessageID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agentID
                  )?.status == .active else {
                throw AgentGroupChatError.notMember
            }
            guard let message = try readMessage(
                ownerUserID: ownerUserID,
                messageID: throughMessageID
            ), message.roomID == roomID else {
                throw AgentGroupChatError.notFound
            }
            if let existing = try readCursor(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID
            ), existing.messageCreatedAtUnixMs > message.createdAtUnixMs
                || (existing.messageCreatedAtUnixMs == message.createdAtUnixMs
                    && existing.messageID >= message.id) {
                return existing
            }
            let updatedAt = max(nowUnixMs, message.createdAtUnixMs)
            try execute(
                """
                INSERT INTO project_agent_read_cursors (
                    owner_user_id, room_id, agent_id, message_id,
                    message_created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(owner_user_id, room_id, agent_id) DO UPDATE SET
                    message_id = excluded.message_id,
                    message_created_at_unix_ms = excluded.message_created_at_unix_ms,
                    updated_at_unix_ms = excluded.updated_at_unix_ms
                """,
                [
                    .text(ownerUserID), .text(roomID), .text(agentID), .text(message.id),
                    .integer(message.createdAtUnixMs), .integer(updatedAt),
                ]
            )
            return .init(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID,
                messageID: message.id,
                messageCreatedAtUnixMs: message.createdAtUnixMs,
                updatedAtUnixMs: updatedAt
            )
        }
    }

    public func readAllUnreadMessagesAndMarkRead(
        ownerUserID: String,
        agentID: String,
        limit: Int = 200,
        nowUnixMs: Int64
    ) throws -> [LocalAgentUnreadConversation] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard (1...500).contains(limit), nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("limit")
        }
        return try transaction {
            guard try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let messages = try AgentMessageRepository.listAllUnread(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                limit: limit,
                preparedStatement: recordPreparedStatement,
                row: readMessage
            )
            guard !messages.isEmpty else { return [] }

            var lastMessageByRoom: [String: ProjectAgentMessage] = [:]
            var roomOrder: [String] = []
            var grouped: [String: [ProjectAgentMessage]] = [:]
            for message in messages {
                if grouped[message.roomID] == nil { roomOrder.append(message.roomID) }
                grouped[message.roomID, default: []].append(message)
                lastMessageByRoom[message.roomID] = message
            }
            for (roomID, message) in lastMessageByRoom {
                try execute(
                    """
                    INSERT INTO project_agent_read_cursors (
                        owner_user_id, room_id, agent_id, message_id,
                        message_created_at_unix_ms, updated_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?)
                    ON CONFLICT(owner_user_id, room_id, agent_id) DO UPDATE SET
                        message_id = excluded.message_id,
                        message_created_at_unix_ms = excluded.message_created_at_unix_ms,
                        updated_at_unix_ms = excluded.updated_at_unix_ms
                    """,
                    [
                        .text(ownerUserID), .text(roomID), .text(agentID), .text(message.id),
                        .integer(message.createdAtUnixMs),
                        .integer(max(nowUnixMs, message.createdAtUnixMs)),
                    ]
                )
            }
            return try roomOrder.map { roomID in
                guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID) else {
                    throw AgentGroupChatError.storage("unread conversation is missing")
                }
                return .init(room: room, messages: grouped[roomID] ?? [])
            }
        }
    }

}
