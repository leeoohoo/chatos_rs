import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

struct ProjectAgentConversationListSnapshot: Sendable, Equatable {
    let activeMemberCountByRoomID: [String: Int]
    let latestMessageByRoomID: [String: ProjectAgentMessage]
}

extension SQLiteAgentGroupChatStore {
    /// Loads the two relation-backed values used by every row in the companion conversation
    /// list with a constant number of SQLite statements. The former per-room implementation
    /// performed a room lookup, member query, room lookup, message query and two relation queries
    /// for every row, which made list refresh cost grow linearly by six statements per room.
    func activeConversationListSnapshot(
        ownerUserID: String
    ) throws -> ProjectAgentConversationListSnapshot {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        let memberCounts = try query(
            """
            SELECT member.room_id, COUNT(*)
            FROM project_agent_room_members member
            JOIN project_agent_rooms room
              ON room.owner_user_id = member.owner_user_id AND room.id = member.room_id
            WHERE member.owner_user_id = ? AND member.status = 'active'
              AND room.status = 'active'
            GROUP BY member.room_id
            """,
            [.text(ownerUserID)]
        ) { statement in
            (Self.string(statement, 0), Int(sqlite3_column_int64(statement, 1)))
        }
        let latestMessageRows = try query(
            """
            WITH ranked_messages AS (
                SELECT msg.owner_user_id, msg.id, msg.room_id,
                       ROW_NUMBER() OVER (
                           PARTITION BY msg.room_id
                           ORDER BY msg.created_at_unix_ms DESC, msg.id DESC
                       ) AS row_number
                FROM project_agent_messages msg
                JOIN project_agent_rooms room
                  ON room.owner_user_id = msg.owner_user_id AND room.id = msg.room_id
                WHERE msg.owner_user_id = ? AND room.status = 'active'
                  AND NOT (
                    msg.sender_kind = 'system'
                    AND msg.causation_id IN ('heartbeat', 'todo', 'todo_status')
                  )
            )
            SELECT msg.owner_user_id, msg.id, msg.room_id, msg.sender_kind,
                   msg.sender_id, msg.content, msg.reply_to_message_id,
                   msg.source_run_id, msg.causation_id, msg.root_message_id,
                   msg.hop_count, msg.created_at_unix_ms
            FROM ranked_messages ranked
            JOIN project_agent_messages msg
              ON msg.owner_user_id = ranked.owner_user_id AND msg.id = ranked.id
            WHERE ranked.row_number = 1
            ORDER BY msg.room_id
            """,
            [.text(ownerUserID)],
            row: readMessageBase
        )
        let latestMessages = try hydrateMessageRelations(
            latestMessageRows,
            ownerUserID: ownerUserID
        )
        return .init(
            activeMemberCountByRoomID: Dictionary(
                uniqueKeysWithValues: memberCounts
            ),
            latestMessageByRoomID: Dictionary(
                uniqueKeysWithValues: latestMessages.map { ($0.roomID, $0) }
            )
        )
    }

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
        let messages = try AgentMessageRepository.list(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            afterUnixMs: afterUnixMs,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessageBase
        )
        return try hydrateMessageRelations(messages, ownerUserID: ownerUserID)
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
        let loaded = try hydrateMessageRelations(AgentMessageRepository.pageForward(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            after: messageCursor,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessageBase
        ), ownerUserID: ownerUserID)
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
        let loaded = try hydrateMessageRelations(AgentMessageRepository.pageBackward(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            before: messageCursor,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessageBase
        ), ownerUserID: ownerUserID)
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
        let loaded = try hydrateMessageRelations(AgentMessageRepository.listUnread(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agentID,
            after: messageCursor,
            limit: limit,
            preparedStatement: recordPreparedStatement,
            row: readMessageBase
        ), ownerUserID: ownerUserID)
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
            let cursor = ProjectAgentReadCursor(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID,
                messageID: message.id,
                messageCreatedAtUnixMs: message.createdAtUnixMs,
                updatedAtUnixMs: updatedAt
            )
            try AgentReadCursorRepository.upsert(
                database,
                cursors: [cursor],
                preparedStatement: recordPreparedStatement
            )
            return cursor
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
            let messages = try hydrateMessageRelations(AgentMessageRepository.listAllUnread(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                limit: limit,
                preparedStatement: recordPreparedStatement,
                row: readMessageBase
            ), ownerUserID: ownerUserID)
            guard !messages.isEmpty else { return [] }

            var lastMessageByRoom: [String: ProjectAgentMessage] = [:]
            var roomOrder: [String] = []
            var grouped: [String: [ProjectAgentMessage]] = [:]
            for message in messages {
                if grouped[message.roomID] == nil { roomOrder.append(message.roomID) }
                grouped[message.roomID, default: []].append(message)
                lastMessageByRoom[message.roomID] = message
            }
            let cursors = lastMessageByRoom.map { roomID, message in
                ProjectAgentReadCursor(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agentID,
                    messageID: message.id,
                    messageCreatedAtUnixMs: message.createdAtUnixMs,
                    updatedAtUnixMs: max(nowUnixMs, message.createdAtUnixMs)
                )
            }
            try AgentReadCursorRepository.upsert(
                database,
                cursors: cursors,
                preparedStatement: recordPreparedStatement
            )
            let rooms = try AgentConversationRepository.rooms(
                database,
                ownerUserID: ownerUserID,
                roomIDs: roomOrder,
                preparedStatement: recordPreparedStatement
            )
            let roomsByID = Dictionary(uniqueKeysWithValues: rooms.map { ($0.id, $0) })
            return try roomOrder.map { roomID in
                guard let room = roomsByID[roomID] else {
                    throw AgentGroupChatError.storage("unread conversation is missing")
                }
                return .init(room: room, messages: grouped[roomID] ?? [])
            }
        }
    }

}
