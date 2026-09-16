import ChatOSAgentRuntime
import ChatOSCore
import Foundation
import SQLite3

/// Account- and project-scoped local authority for Agent rooms. The transcript and delivery
/// queue remain usable without the network, Memory Engine, Plugin Management or Codex CLI.
public actor SQLiteAgentGroupChatStore: AgentGroupChatStore, LocalAgentGroupChatRunStoring {
    private nonisolated(unsafe) var database: OpaquePointer?

    public init(databaseURL: URL) throws {
        try FileManager.default.createDirectory(
            at: databaseURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        var handle: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &handle,
            SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK else {
            let message = handle.map { String(cString: sqlite3_errmsg($0)) } ?? "open failed"
            sqlite3_close(handle)
            throw AgentGroupChatError.storage(message)
        }
        do {
            sqlite3_busy_timeout(handle, 5_000)
            guard sqlite3_exec(handle, Self.schema, nil, nil, nil) == SQLITE_OK else {
                throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(handle)))
            }
            database = handle
        } catch {
            sqlite3_close(handle)
            throw error
        }
    }

    deinit { sqlite3_close(database) }

    public func createAgent(
        ownerUserID: String,
        draft: LocalAgentProfileDraft
    ) throws -> LocalAgentProfile {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try draft.validate()
        let now = Self.now()
        let record = LocalAgentProfile(
            id: UUID().uuidString.lowercased(),
            ownerUserID: ownerUserID,
            draft: draft,
            createdAtUnixMs: now,
            updatedAtUnixMs: now
        )
        try record.validate()
        try execute(
            """
            INSERT INTO local_agent_profiles (
                owner_user_id, id, name, description, role_prompt, model_config_id,
                default_plugin_ids_json, default_skill_ids_json, status,
                created_at_unix_ms, updated_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [
                .text(ownerUserID), .text(record.id), .text(draft.name),
                .text(draft.description), .text(draft.rolePrompt), .text(draft.modelConfigID),
                .text(try encodeStrings(draft.defaultPluginIDs)),
                .text(try encodeStrings(draft.defaultSkillIDs)), .text(record.status.rawValue),
                .integer(now), .integer(now),
            ]
        )
        return record
    }

    public func listAgents(
        ownerUserID: String,
        includeArchived: Bool = false
    ) throws -> [LocalAgentProfile] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try query(
            "SELECT \(Self.agentColumns) FROM local_agent_profiles WHERE owner_user_id = ?"
                + (includeArchived ? "" : " AND status = 'active'")
                + " ORDER BY name, id",
            [.text(ownerUserID)],
            row: readAgent
        )
    }

    public func createRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try draft.validate()
        return try transaction {
            guard try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID) == nil else {
                throw AgentGroupChatError.conflict
            }
            let now = Self.now()
            let room = ProjectAgentRoom(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: draft,
                createdAtUnixMs: now,
                updatedAtUnixMs: now
            )
            try room.validate()
            try execute(
                """
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, NULL, 'active', ?, ?)
                """,
                [
                    .text(ownerUserID), .text(room.id), .text(projectID), .text(draft.name),
                    .text(draft.goal), .integer(now), .integer(now),
                ]
            )
            return room
        }
    }

    public func activeRoom(ownerUserID: String, projectID: String) throws -> ProjectAgentRoom? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        return try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID)
    }

    public func addMember(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        draft: ProjectAgentRoomMemberDraft
    ) throws -> ProjectAgentRoomMember {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        try draft.validate()
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            guard try readMember(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID) == nil else {
                throw AgentGroupChatError.conflict
            }
            let member = ProjectAgentRoomMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID,
                draft: draft,
                joinedAtUnixMs: Self.now()
            )
            try member.validate()
            try execute(
                """
                INSERT INTO project_agent_room_members (
                    owner_user_id, room_id, agent_id, role, responsibility,
                    plugin_allowlist_json, status, joined_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, 'active', ?)
                """,
                [
                    .text(ownerUserID), .text(roomID), .text(agentID), .text(draft.role),
                    .text(draft.responsibility), .text(try encodeStrings(draft.pluginAllowlist)),
                    .integer(member.joinedAtUnixMs),
                ]
            )
            return member
        }
    }

    public func listMembers(
        ownerUserID: String,
        roomID: String
    ) throws -> [ProjectAgentRoomMember] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try query(
            """
            SELECT \(Self.memberColumns) FROM project_agent_room_members
            WHERE owner_user_id = ? AND room_id = ? AND status = 'active'
            ORDER BY joined_at_unix_ms, agent_id
            """,
            [.text(ownerUserID), .text(roomID)],
            row: readMember
        )
    }

    public func setDefaultAgent(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentRoom {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active else {
                throw AgentGroupChatError.notFound
            }
            guard try readMember(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notMember
            }
            let now = max(Self.now(), room.updatedAtUnixMs)
            try execute(
                """
                UPDATE project_agent_rooms SET default_agent_id = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [.text(agentID), .integer(now), .text(ownerUserID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readRoom(ownerUserID: ownerUserID, roomID: roomID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    public func postMessage(
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft,
        limits: AgentGroupChatRoutingLimits = .init()
    ) throws -> AgentGroupChatPostResult {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try draft.validate()
        try limits.validate()
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active else {
                throw AgentGroupChatError.notFound
            }
            try validateSender(ownerUserID: ownerUserID, roomID: roomID, draft: draft)
            if let replyToMessageID = draft.replyToMessageID {
                try requireMessage(ownerUserID: ownerUserID, roomID: roomID, messageID: replyToMessageID)
            }
            if let rootMessageID = draft.rootMessageID {
                try requireMessage(ownerUserID: ownerUserID, roomID: roomID, messageID: rootMessageID)
            }
            for agentID in draft.mentionedAgentIDs {
                guard try readMember(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)?.status == .active else {
                    throw AgentGroupChatError.notMember
                }
            }

            let messageID = UUID().uuidString.lowercased()
            let rootMessageID = draft.rootMessageID ?? messageID
            let now = Self.now()
            try execute(
                """
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(messageID), .text(roomID),
                    .text(draft.senderKind.rawValue), .text(draft.senderID), .text(draft.content),
                    .optionalText(draft.replyToMessageID), .optionalText(draft.sourceRunID),
                    .optionalText(draft.causationID), .text(rootMessageID),
                    .integer(Int64(draft.hopCount)), .integer(now),
                ]
            )
            for (position, agentID) in draft.mentionedAgentIDs.enumerated() {
                try execute(
                    """
                    INSERT INTO project_agent_message_mentions (
                        owner_user_id, message_id, agent_id, position
                    ) VALUES (?, ?, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(messageID), .text(agentID),
                        .integer(Int64(position)),
                    ]
                )
            }

            let message = ProjectAgentMessage(
                id: messageID,
                ownerUserID: ownerUserID,
                roomID: roomID,
                draft: draft,
                rootMessageID: rootMessageID,
                createdAtUnixMs: now
            )
            let candidates: [String]
            if !draft.mentionedAgentIDs.isEmpty {
                candidates = draft.mentionedAgentIDs
            } else if draft.senderKind == .human, let defaultAgentID = room.defaultAgentID {
                candidates = [defaultAgentID]
            } else {
                candidates = []
            }
            let targets = candidates.filter {
                !(draft.senderKind == .agent && $0 == draft.senderID)
            }
            let existingRunCount = try scalarInt64(
                """
                SELECT COUNT(*) FROM project_agent_deliveries
                WHERE owner_user_id = ? AND root_message_id = ?
                """,
                [.text(ownerUserID), .text(rootMessageID)]
            )
            let stopReason: String?
            if draft.hopCount > limits.maximumHopCount {
                stopReason = "本轮 Agent 协作已达到最大唤醒深度。"
            } else if existingRunCount + Int64(targets.count) > Int64(limits.maximumAgentRunsPerRootMessage) {
                stopReason = "本轮 Agent 协作已达到最大运行次数。"
            } else {
                stopReason = nil
            }

            var deliveries: [ProjectAgentDelivery] = []
            if stopReason == nil {
                for targetAgentID in targets {
                    let triggerKind: ProjectAgentDeliveryTriggerKind = if draft.mentionedAgentIDs.isEmpty {
                        .defaultAgent
                    } else if draft.senderKind == .agent {
                        .agentMention
                    } else {
                        .mention
                    }
                    let deliveryID = UUID().uuidString.lowercased()
                    let deduplicationKey = "\(messageID):\(targetAgentID)"
                    try execute(
                        """
                        INSERT INTO project_agent_deliveries (
                            owner_user_id, id, room_id, message_id, root_message_id,
                            target_agent_id, trigger_kind, status, attempt, hop_count,
                            deduplication_key, response_message_id, last_error,
                            claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', 0, ?, ?, NULL, NULL, NULL, NULL, ?)
                        """,
                        [
                            .text(ownerUserID), .text(deliveryID), .text(roomID), .text(messageID),
                            .text(rootMessageID), .text(targetAgentID), .text(triggerKind.rawValue),
                            .integer(Int64(draft.hopCount)), .text(deduplicationKey), .integer(now),
                        ]
                    )
                    guard let delivery = try readDelivery(
                        ownerUserID: ownerUserID,
                        deliveryID: deliveryID
                    ) else { throw AgentGroupChatError.storage("delivery insert was not readable") }
                    deliveries.append(delivery)
                }
            }
            return AgentGroupChatPostResult(
                message: message,
                deliveries: deliveries,
                routingStopReason: stopReason
            )
        }
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
        var values: [Value] = [.text(ownerUserID), .text(roomID)]
        var predicate = "owner_user_id = ? AND room_id = ?"
        if let afterUnixMs {
            guard afterUnixMs >= 0 else { throw AgentGroupChatError.invalidField("afterUnixMs") }
            predicate += " AND created_at_unix_ms > ?"
            values.append(.integer(afterUnixMs))
        }
        values.append(.integer(Int64(limit)))
        return try query(
            """
            SELECT \(Self.messageColumns) FROM project_agent_messages
            WHERE \(predicate) ORDER BY created_at_unix_ms, id LIMIT ?
            """,
            values,
            row: readMessage
        )
    }

    public func message(
        ownerUserID: String,
        roomID: String,
        messageID: String
    ) throws -> ProjectAgentMessage? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(messageID, field: "messageID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        let message = try readMessage(ownerUserID: ownerUserID, messageID: messageID)
        guard message?.roomID == roomID else { return nil }
        return message
    }

    public func claimNextDelivery(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            let activeCount = try scalarInt64(
                """
                SELECT COUNT(*) FROM project_agent_deliveries
                WHERE owner_user_id = ? AND target_agent_id = ? AND status = 'running'
                """,
                [.text(ownerUserID), .text(agentID)]
            )
            guard activeCount == 0 else { return nil }
            let ids: [String] = try query(
                """
                SELECT d.id FROM project_agent_deliveries d
                JOIN project_agent_rooms r
                  ON r.owner_user_id = d.owner_user_id AND r.id = d.room_id
                JOIN project_agent_room_members m
                  ON m.owner_user_id = d.owner_user_id
                 AND m.room_id = d.room_id AND m.agent_id = d.target_agent_id
                WHERE d.owner_user_id = ? AND d.target_agent_id = ?
                  AND d.status = 'pending' AND r.status = 'active' AND m.status = 'active'
                ORDER BY d.created_at_unix_ms, d.id LIMIT 1
                """,
                [.text(ownerUserID), .text(agentID)]
            ) { Self.string($0, 0) }
            guard let id = ids.first else { return nil }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'running', attempt = attempt + 1, claimed_at_unix_ms = ?,
                    completed_at_unix_ms = NULL, last_error = NULL
                WHERE owner_user_id = ? AND id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(id)]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            return try readDelivery(ownerUserID: ownerUserID, deliveryID: id)
        }
    }

    public func delivery(
        ownerUserID: String,
        deliveryID: String
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        return try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID)
    }

    public func completeDelivery(
        ownerUserID: String,
        deliveryID: String,
        responseMessageID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        try validateDeliveryMutation(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            nowUnixMs: nowUnixMs
        )
        try AgentGroupChatValidation.identifier(responseMessageID, field: "responseMessageID")
        return try transaction {
            guard let delivery = try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID),
                  delivery.status == .running else {
                throw AgentGroupChatError.conflict
            }
            guard let response = try readMessage(ownerUserID: ownerUserID, messageID: responseMessageID),
                  response.roomID == delivery.roomID,
                  response.senderKind == .agent,
                  response.senderID == delivery.targetAgentID else {
                throw AgentGroupChatError.invalidField("responseMessageID")
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'completed', response_message_id = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'running'
                """,
                [
                    .text(responseMessageID), .integer(nowUnixMs),
                    .text(ownerUserID), .text(deliveryID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    public func failDelivery(
        ownerUserID: String,
        deliveryID: String,
        error: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        try validateDeliveryMutation(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            nowUnixMs: nowUnixMs
        )
        try AgentGroupChatValidation.text(error, field: "error", maximumLength: 8_000)
        return try transaction {
            guard try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID)?.status == .running else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'failed', last_error = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'running'
                """,
                [.text(error), .integer(nowUnixMs), .text(ownerUserID), .text(deliveryID)]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    /// Atomically stops every queued or running delivery in one room. Pending work is cancelled;
    /// running work is failed and any durable Run is closed in the same SQLite transaction.
    public func stopOutstandingDeliveries(
        ownerUserID: String,
        roomID: String,
        reason: String,
        nowUnixMs: Int64
    ) throws -> Int {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.text(reason, field: "reason", maximumLength: 8_000)
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
                throw AgentGroupChatError.notFound
            }
            let deliveryIDs: [String] = try query(
                """
                SELECT id FROM project_agent_deliveries
                WHERE owner_user_id = ? AND room_id = ? AND status IN ('pending', 'running')
                ORDER BY created_at_unix_ms, id
                """,
                [.text(ownerUserID), .text(roomID)]
            ) { Self.string($0, 0) }
            guard !deliveryIDs.isEmpty else { return 0 }

            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            for deliveryID in deliveryIDs {
                guard var run = try readRun(ownerUserID: ownerUserID, deliveryID: deliveryID) else {
                    continue
                }
                run.checkpoint.status = .failed
                run.checkpoint.stopReason = reason
                run.events.append(.init(
                    kind: "stopped_all",
                    detail: reason,
                    modelCalls: run.checkpoint.modelCalls
                ))
                run.updatedAtUnixMs = max(nowUnixMs, run.updatedAtUnixMs)
                try run.validate()
                let json = String(decoding: try encoder.encode(run), as: UTF8.self)
                try execute(
                    """
                    UPDATE local_agent_group_chat_runs
                    SET status = 'failed', run_json = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND id = ?
                    """,
                    [
                        .text(json), .integer(run.updatedAtUnixMs), .text(ownerUserID),
                        .text(run.id.uuidString.lowercased()),
                    ]
                )
                guard sqlite3_changes(database) == 1 else {
                    throw AgentGroupChatError.conflict
                }
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = CASE status WHEN 'running' THEN 'failed' ELSE 'cancelled' END,
                    last_error = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND room_id = ? AND status IN ('pending', 'running')
                """,
                [.text(reason), .integer(nowUnixMs), .text(ownerUserID), .text(roomID)]
            )
            guard sqlite3_changes(database) == Int32(deliveryIDs.count) else {
                throw AgentGroupChatError.conflict
            }
            return deliveryIDs.count
        }
    }

    public func saveRun(_ run: LocalAgentGroupChatRun) throws {
        try run.validate()
        let context = run.context
        guard let delivery = try readDelivery(
            ownerUserID: context.ownerUserID,
            deliveryID: context.deliveryID
        ), delivery.roomID == context.roomID,
           delivery.targetAgentID == context.agentID,
           delivery.messageID == context.triggerMessageID,
           delivery.rootMessageID == context.rootMessageID else {
            throw AgentGroupChatError.conflict
        }
        try transaction {
            if let existing = try readRun(
                ownerUserID: context.ownerUserID,
                deliveryID: context.deliveryID
            ), existing.id != run.id || existing.context != context
                || existing.createdAtUnixMs != run.createdAtUnixMs {
                throw AgentGroupChatError.conflict
            }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let json = String(decoding: try encoder.encode(run), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_agent_group_chat_runs (
                    owner_user_id, id, delivery_id, room_id, project_id, agent_id,
                    status, run_json, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(owner_user_id, id) DO UPDATE SET
                    status = excluded.status,
                    run_json = excluded.run_json,
                    updated_at_unix_ms = excluded.updated_at_unix_ms
                """,
                [
                    .text(context.ownerUserID), .text(run.id.uuidString.lowercased()),
                    .text(context.deliveryID), .text(context.roomID), .text(context.projectID),
                    .text(context.agentID), .text(run.checkpoint.status.rawValue), .text(json),
                    .integer(run.createdAtUnixMs), .integer(run.updatedAtUnixMs),
                ]
            )
        }
    }

    public func run(
        ownerUserID: String,
        deliveryID: String
    ) throws -> LocalAgentGroupChatRun? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        return try readRun(ownerUserID: ownerUserID, deliveryID: deliveryID)
    }

    public func listUnfinishedRuns(
        ownerUserID: String,
        projectID: String,
        limit: Int
    ) throws -> [LocalAgentGroupChatRun] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        let values: [String] = try query(
            """
            SELECT run_json FROM local_agent_group_chat_runs
            WHERE owner_user_id = ? AND project_id = ?
              AND status NOT IN ('completed', 'failed')
            ORDER BY updated_at_unix_ms DESC, id DESC LIMIT ?
            """,
            [.text(ownerUserID), .text(projectID), .integer(Int64(limit))]
        ) { Self.string($0, 0) }
        return try values.map(Self.decodeRun)
    }

    private func readActiveRoom(ownerUserID: String, projectID: String) throws -> ProjectAgentRoom? {
        try query(
            """
            SELECT \(Self.roomColumns) FROM project_agent_rooms
            WHERE owner_user_id = ? AND project_id = ? AND status = 'active' LIMIT 1
            """,
            [.text(ownerUserID), .text(projectID)],
            row: readRoom
        ).first
    }

    private func readRoom(ownerUserID: String, roomID: String) throws -> ProjectAgentRoom? {
        try query(
            "SELECT \(Self.roomColumns) FROM project_agent_rooms WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(roomID)],
            row: readRoom
        ).first
    }

    private func readAgent(ownerUserID: String, agentID: String) throws -> LocalAgentProfile? {
        try query(
            "SELECT \(Self.agentColumns) FROM local_agent_profiles WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(agentID)],
            row: readAgent
        ).first
    }

    private func readMember(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentRoomMember? {
        try query(
            """
            SELECT \(Self.memberColumns) FROM project_agent_room_members
            WHERE owner_user_id = ? AND room_id = ? AND agent_id = ?
            """,
            [.text(ownerUserID), .text(roomID), .text(agentID)],
            row: readMember
        ).first
    }

    private func readMessage(ownerUserID: String, messageID: String) throws -> ProjectAgentMessage? {
        try query(
            "SELECT \(Self.messageColumns) FROM project_agent_messages WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(messageID)],
            row: readMessage
        ).first
    }

    private func readDelivery(ownerUserID: String, deliveryID: String) throws -> ProjectAgentDelivery? {
        try query(
            "SELECT \(Self.deliveryColumns) FROM project_agent_deliveries WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(deliveryID)],
            row: readDelivery
        ).first
    }

    private func readRun(
        ownerUserID: String,
        deliveryID: String
    ) throws -> LocalAgentGroupChatRun? {
        let values: [String] = try query(
            """
            SELECT run_json FROM local_agent_group_chat_runs
            WHERE owner_user_id = ? AND delivery_id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(deliveryID)]
        ) { Self.string($0, 0) }
        guard let json = values.first else { return nil }
        return try Self.decodeRun(json)
    }

    private static func decodeRun(_ json: String) throws -> LocalAgentGroupChatRun {
        do {
            let run = try JSONDecoder().decode(LocalAgentGroupChatRun.self, from: Data(json.utf8))
            try run.validate()
            return run
        } catch let error as AgentGroupChatError {
            throw error
        } catch {
            throw AgentGroupChatError.storage("invalid Agent run record")
        }
    }

    private func requireMessage(ownerUserID: String, roomID: String, messageID: String) throws {
        guard try readMessage(ownerUserID: ownerUserID, messageID: messageID)?.roomID == roomID else {
            throw AgentGroupChatError.invalidField("messageReference")
        }
    }

    private func validateSender(
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

    private func validateOwnerRoomAgent(ownerUserID: String, roomID: String, agentID: String) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
    }

    private func validateDeliveryMutation(
        ownerUserID: String,
        deliveryID: String,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
    }

    private func readAgent(_ statement: OpaquePointer) throws -> LocalAgentProfile {
        guard let status = LocalAgentProfileStatus(rawValue: Self.string(statement, 8)) else {
            throw AgentGroupChatError.storage("invalid agent status")
        }
        let profile = LocalAgentProfile(
            id: Self.string(statement, 1),
            ownerUserID: Self.string(statement, 0),
            draft: .init(
                name: Self.string(statement, 2),
                description: Self.string(statement, 3),
                rolePrompt: Self.string(statement, 4),
                modelConfigID: Self.string(statement, 5),
                defaultPluginIDs: try decodeStrings(Self.string(statement, 6)),
                defaultSkillIDs: try decodeStrings(Self.string(statement, 7))
            ),
            status: status,
            createdAtUnixMs: sqlite3_column_int64(statement, 9),
            updatedAtUnixMs: sqlite3_column_int64(statement, 10)
        )
        try profile.validate()
        return profile
    }

    private func readRoom(_ statement: OpaquePointer) throws -> ProjectAgentRoom {
        guard let status = ProjectAgentRoomStatus(rawValue: Self.string(statement, 6)) else {
            throw AgentGroupChatError.storage("invalid room status")
        }
        let room = ProjectAgentRoom(
            id: Self.string(statement, 1),
            ownerUserID: Self.string(statement, 0),
            projectID: Self.string(statement, 2),
            draft: .init(name: Self.string(statement, 3), goal: Self.string(statement, 4)),
            defaultAgentID: Self.optionalString(statement, 5),
            status: status,
            createdAtUnixMs: sqlite3_column_int64(statement, 7),
            updatedAtUnixMs: sqlite3_column_int64(statement, 8)
        )
        try room.validate()
        return room
    }

    private func readMember(_ statement: OpaquePointer) throws -> ProjectAgentRoomMember {
        guard let status = ProjectAgentRoomMemberStatus(rawValue: Self.string(statement, 6)) else {
            throw AgentGroupChatError.storage("invalid member status")
        }
        let member = ProjectAgentRoomMember(
            ownerUserID: Self.string(statement, 0),
            roomID: Self.string(statement, 1),
            agentID: Self.string(statement, 2),
            draft: .init(
                role: Self.string(statement, 3),
                responsibility: Self.string(statement, 4),
                pluginAllowlist: try decodeStrings(Self.string(statement, 5))
            ),
            status: status,
            joinedAtUnixMs: sqlite3_column_int64(statement, 7)
        )
        try member.validate()
        return member
    }

    private func readMessage(_ statement: OpaquePointer) throws -> ProjectAgentMessage {
        guard let senderKind = ProjectAgentMessageSenderKind(rawValue: Self.string(statement, 3)) else {
            throw AgentGroupChatError.storage("invalid message sender kind")
        }
        let messageID = Self.string(statement, 1)
        let ownerUserID = Self.string(statement, 0)
        let mentions: [String] = try query(
            """
            SELECT agent_id FROM project_agent_message_mentions
            WHERE owner_user_id = ? AND message_id = ? ORDER BY position
            """,
            [.text(ownerUserID), .text(messageID)]
        ) { Self.string($0, 0) }
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
            createdAtUnixMs: sqlite3_column_int64(statement, 11)
        )
    }

    private func readDelivery(_ statement: OpaquePointer) throws -> ProjectAgentDelivery {
        guard let triggerKind = ProjectAgentDeliveryTriggerKind(rawValue: Self.string(statement, 6)),
              let status = ProjectAgentDeliveryStatus(rawValue: Self.string(statement, 7)) else {
            throw AgentGroupChatError.storage("invalid delivery state")
        }
        return ProjectAgentDelivery(
            id: Self.string(statement, 1),
            ownerUserID: Self.string(statement, 0),
            roomID: Self.string(statement, 2),
            messageID: Self.string(statement, 3),
            rootMessageID: Self.string(statement, 4),
            targetAgentID: Self.string(statement, 5),
            triggerKind: triggerKind,
            status: status,
            attempt: Int(sqlite3_column_int64(statement, 8)),
            hopCount: Int(sqlite3_column_int64(statement, 9)),
            deduplicationKey: Self.string(statement, 10),
            responseMessageID: Self.optionalString(statement, 11),
            lastError: Self.optionalString(statement, 12),
            claimedAtUnixMs: Self.optionalInt64(statement, 13),
            completedAtUnixMs: Self.optionalInt64(statement, 14),
            createdAtUnixMs: sqlite3_column_int64(statement, 15)
        )
    }

    private enum Value {
        case text(String)
        case integer(Int64)
        case null

        static func optionalText(_ value: String?) -> Self { value.map(Self.text) ?? .null }
    }

    private func query<T>(
        _ sql: String,
        _ values: [Value] = [],
        row: (OpaquePointer) throws -> T
    ) throws -> [T] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else { throw storageError() }
        defer { sqlite3_finalize(statement) }
        for (offset, value) in values.enumerated() {
            let index = Int32(offset + 1)
            let result: Int32
            switch value {
            case let .text(text):
                result = sqlite3_bind_text(
                    statement,
                    index,
                    text,
                    -1,
                    unsafeBitCast(-1, to: sqlite3_destructor_type.self)
                )
            case let .integer(number):
                result = sqlite3_bind_int64(statement, index, number)
            case .null:
                result = sqlite3_bind_null(statement, index)
            }
            guard result == SQLITE_OK else { throw storageError() }
        }
        var rows: [T] = []
        while true {
            switch sqlite3_step(statement) {
            case SQLITE_ROW: rows.append(try row(statement))
            case SQLITE_DONE: return rows
            default: throw storageError()
            }
        }
    }

    private func execute(_ sql: String, _ values: [Value] = []) throws {
        let _: [Int] = try query(sql, values) { _ in 0 }
    }

    private func scalarInt64(_ sql: String, _ values: [Value]) throws -> Int64 {
        try query(sql, values) { sqlite3_column_int64($0, 0) }.first ?? 0
    }

    private func transaction<T>(_ body: () throws -> T) throws -> T {
        try execute("BEGIN IMMEDIATE")
        do {
            let result = try body()
            try execute("COMMIT")
            return result
        } catch {
            try? execute("ROLLBACK")
            throw error
        }
    }

    private func encodeStrings(_ values: [String]) throws -> String {
        String(decoding: try JSONEncoder().encode(values), as: UTF8.self)
    }

    private func decodeStrings(_ value: String) throws -> [String] {
        do {
            return try JSONDecoder().decode([String].self, from: Data(value.utf8))
        } catch {
            throw AgentGroupChatError.storage("invalid string list")
        }
    }

    private func storageError() -> AgentGroupChatError {
        .storage(String(cString: sqlite3_errmsg(database)))
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }

    private static func optionalString(_ statement: OpaquePointer, _ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL,
              let value = sqlite3_column_text(statement, index) else { return nil }
        return String(cString: value)
    }

    private static func optionalInt64(_ statement: OpaquePointer, _ index: Int32) -> Int64? {
        sqlite3_column_type(statement, index) == SQLITE_NULL
            ? nil
            : sqlite3_column_int64(statement, index)
    }

    private static func now() -> Int64 { Int64(Date().timeIntervalSince1970 * 1_000) }

    private static let agentColumns = "owner_user_id, id, name, description, role_prompt, model_config_id, default_plugin_ids_json, default_skill_ids_json, status, created_at_unix_ms, updated_at_unix_ms"
    private static let roomColumns = "owner_user_id, id, project_id, name, goal, default_agent_id, status, created_at_unix_ms, updated_at_unix_ms"
    private static let memberColumns = "owner_user_id, room_id, agent_id, role, responsibility, plugin_allowlist_json, status, joined_at_unix_ms"
    private static let messageColumns = "owner_user_id, id, room_id, sender_kind, sender_id, content, reply_to_message_id, source_run_id, causation_id, root_message_id, hop_count, created_at_unix_ms"
    private static let deliveryColumns = "owner_user_id, id, room_id, message_id, root_message_id, target_agent_id, trigger_kind, status, attempt, hop_count, deduplication_key, response_message_id, last_error, claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms"

    private static let schema = """
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        BEGIN IMMEDIATE;

        CREATE TABLE IF NOT EXISTS local_agent_profiles (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            role_prompt TEXT NOT NULL,
            model_config_id TEXT NOT NULL,
            default_plugin_ids_json TEXT NOT NULL,
            default_skill_ids_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
            created_at_unix_ms INTEGER NOT NULL,
            updated_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, id)
        );

        CREATE TABLE IF NOT EXISTS project_agent_rooms (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            project_id TEXT NOT NULL,
            name TEXT NOT NULL,
            goal TEXT NOT NULL,
            default_agent_id TEXT,
            status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
            created_at_unix_ms INTEGER NOT NULL,
            updated_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, id),
            FOREIGN KEY(owner_user_id, default_agent_id)
                REFERENCES local_agent_profiles(owner_user_id, id)
        );
        CREATE UNIQUE INDEX IF NOT EXISTS one_active_agent_room_per_project
            ON project_agent_rooms(owner_user_id, project_id) WHERE status = 'active';

        CREATE TABLE IF NOT EXISTS project_agent_room_members (
            owner_user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            role TEXT NOT NULL,
            responsibility TEXT NOT NULL,
            plugin_allowlist_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('active', 'removed')),
            joined_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, room_id)
                REFERENCES project_agent_rooms(owner_user_id, id),
            FOREIGN KEY(owner_user_id, agent_id)
                REFERENCES local_agent_profiles(owner_user_id, id)
        );

        CREATE TABLE IF NOT EXISTS project_agent_messages (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            sender_kind TEXT NOT NULL CHECK(sender_kind IN ('human', 'agent', 'system')),
            sender_id TEXT NOT NULL,
            content TEXT NOT NULL,
            reply_to_message_id TEXT,
            source_run_id TEXT,
            causation_id TEXT,
            root_message_id TEXT NOT NULL,
            hop_count INTEGER NOT NULL CHECK(hop_count >= 0),
            created_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, id),
            FOREIGN KEY(owner_user_id, room_id)
                REFERENCES project_agent_rooms(owner_user_id, id),
            FOREIGN KEY(owner_user_id, reply_to_message_id)
                REFERENCES project_agent_messages(owner_user_id, id),
            FOREIGN KEY(owner_user_id, root_message_id)
                REFERENCES project_agent_messages(owner_user_id, id)
                DEFERRABLE INITIALLY DEFERRED
        );
        CREATE INDEX IF NOT EXISTS project_agent_messages_room_order
            ON project_agent_messages(owner_user_id, room_id, created_at_unix_ms, id);

        CREATE TABLE IF NOT EXISTS project_agent_message_mentions (
            owner_user_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            position INTEGER NOT NULL CHECK(position >= 0),
            PRIMARY KEY(owner_user_id, message_id, agent_id),
            FOREIGN KEY(owner_user_id, message_id)
                REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE,
            FOREIGN KEY(owner_user_id, agent_id)
                REFERENCES local_agent_profiles(owner_user_id, id)
        );

        CREATE TABLE IF NOT EXISTS project_agent_deliveries (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            root_message_id TEXT NOT NULL,
            target_agent_id TEXT NOT NULL,
            trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('mention', 'default_agent', 'agent_mention')),
            status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
            attempt INTEGER NOT NULL CHECK(attempt >= 0),
            hop_count INTEGER NOT NULL CHECK(hop_count >= 0),
            deduplication_key TEXT NOT NULL,
            response_message_id TEXT,
            last_error TEXT,
            claimed_at_unix_ms INTEGER,
            completed_at_unix_ms INTEGER,
            created_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, deduplication_key),
            FOREIGN KEY(owner_user_id, room_id)
                REFERENCES project_agent_rooms(owner_user_id, id),
            FOREIGN KEY(owner_user_id, message_id)
                REFERENCES project_agent_messages(owner_user_id, id),
            FOREIGN KEY(owner_user_id, root_message_id)
                REFERENCES project_agent_messages(owner_user_id, id),
            FOREIGN KEY(owner_user_id, room_id, target_agent_id)
                REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, response_message_id)
                REFERENCES project_agent_messages(owner_user_id, id)
        );
        CREATE INDEX IF NOT EXISTS project_agent_deliveries_agent_queue
            ON project_agent_deliveries(owner_user_id, target_agent_id, status, created_at_unix_ms, id);
        CREATE UNIQUE INDEX IF NOT EXISTS one_running_delivery_per_agent
            ON project_agent_deliveries(owner_user_id, target_agent_id) WHERE status = 'running';

        CREATE TABLE IF NOT EXISTS local_agent_group_chat_runs (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            delivery_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            project_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN (
                'ready', 'running', 'paused', 'completed', 'failed', 'needsReview', 'limitReached'
            )),
            run_json TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL,
            updated_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, delivery_id),
            FOREIGN KEY(owner_user_id, delivery_id)
                REFERENCES project_agent_deliveries(owner_user_id, id),
            FOREIGN KEY(owner_user_id, room_id, agent_id)
                REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id)
        );
        CREATE INDEX IF NOT EXISTS local_agent_group_chat_runs_status
            ON local_agent_group_chat_runs(owner_user_id, status, updated_at_unix_ms);

        CREATE TABLE IF NOT EXISTS local_agent_group_chat_schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL
        );
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (1);
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (2);
        COMMIT;
        """
}
