import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

public struct ProjectAgentArtifactUploadJob: Sendable, Equatable {
    public let attachmentID: String
    public let roomID: String
    public let attempt: Int
    public let request: AgentArtifactUploadRequest

    public init(
        attachmentID: String,
        roomID: String,
        attempt: Int,
        request: AgentArtifactUploadRequest
    ) {
        self.attachmentID = attachmentID
        self.roomID = roomID
        self.attempt = attempt
        self.request = request
    }
}

/// Account- and project-scoped local authority for Agent rooms. The transcript and delivery
/// queue remain usable without the network, Memory Engine, Plugin Management or Codex CLI.
public actor SQLiteAgentGroupChatStore: AgentGroupChatStore, LocalAgentGroupChatRunStoring {
    private nonisolated(unsafe) var database: OpaquePointer?
    private let attachmentsRootURL: URL
    private let agentArtifactService: (any AgentArtifactRemoteServing)?
#if DEBUG
    private var debugPreparedStatementCount = 0
#endif

    public init(
        databaseURL: URL,
        agentArtifactService: (any AgentArtifactRemoteServing)? = nil
    ) throws {
        attachmentsRootURL = databaseURL.deletingLastPathComponent()
            .appendingPathComponent("AgentGroupChatAttachments", isDirectory: true)
        self.agentArtifactService = agentArtifactService
        database = try AgentGroupChatDatabase.open(at: databaseURL)
    }

    deinit { AgentGroupChatDatabase.close(database) }

#if DEBUG
    /// Test-only counter for repeatable Store baselines. Release builds do not carry the counter.
    func preparedStatementCountForTesting() -> Int {
        debugPreparedStatementCount
    }

    func totalDatabaseChangesForTesting() -> Int64 {
        Int64(sqlite3_total_changes(database))
    }
#endif

    /// Creates a Run-scoped staging directory beneath the existing protected attachment root.
    /// The opaque directory name is never exposed to the model and is removed with the Run vault.
    public func createAgentDocumentDraftDirectory() throws -> URL {
        let draftsRoot = attachmentsRootURL.appendingPathComponent(".drafts", isDirectory: true)
        try FileManager.default.createDirectory(
            at: draftsRoot,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let directory = draftsRoot.appendingPathComponent(
            UUID().uuidString.lowercased(),
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        return directory
    }

    public func createAgent(
        ownerUserID: String,
        draft: LocalAgentProfileDraft
    ) throws -> LocalAgentProfile {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try draft.validate()
        let now = Self.now()
        let nextHeartbeatAtUnixMs = draft.heartbeatEnabled
            ? now + Int64(draft.heartbeatIntervalSeconds) * 1_000
            : nil
        let record = LocalAgentProfile(
            id: UUID().uuidString.lowercased(),
            ownerUserID: ownerUserID,
            draft: draft,
            createdAtUnixMs: now,
            updatedAtUnixMs: now,
            nextHeartbeatAtUnixMs: nextHeartbeatAtUnixMs
        )
        try record.validate()
        try execute(
            """
            INSERT INTO local_agent_profiles (
                owner_user_id, id, name, description, role_prompt, model_config_id,
                thinking_level, profession_key, default_plugin_ids_json, default_skill_ids_json,
                heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
                last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
                status, created_at_unix_ms, updated_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?)
            """,
            [
                .text(ownerUserID), .text(record.id), .text(draft.name),
                .text(draft.description), .text(draft.rolePrompt), .text(draft.modelConfigID),
                draft.thinkingLevel.map(Value.text) ?? .null,
                .text(draft.professionKey),
                .text(try encodeStrings(draft.defaultPluginIDs)),
                .text(try encodeStrings(draft.defaultSkillIDs)),
                .integer(draft.heartbeatEnabled ? 1 : 0),
                .integer(Int64(draft.heartbeatIntervalSeconds)), .text(draft.heartbeatPrompt),
                nextHeartbeatAtUnixMs.map(Value.integer) ?? .null, .text(record.status.rawValue),
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
        return try AgentProfileRepository.list(
            database,
            ownerUserID: ownerUserID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
    }

    public func nextAgentHeartbeatDue(ownerUserID: String) throws -> Int64? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentProfileRepository.nextHeartbeatDue(
            database,
            ownerUserID: ownerUserID,
            preparedStatement: recordPreparedStatement
        )
    }

    /// Creates exactly one account-wide inbox wake-up for each due Agent. The wake-up is anchored
    /// to the Agent's Human direct conversation only because the existing durable run queue needs
    /// a room authority; Relay reads every unread conversation in that one run.
    public func enqueueDueAgentHeartbeats(
        ownerUserID: String,
        nowUnixMs: Int64,
        agentLimit: Int = 16
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard (1...128).contains(agentLimit) else {
            throw AgentGroupChatError.invalidField("agentLimit")
        }
        return try transaction {
            struct DueAgent {
                let id: String
                let intervalSeconds: Int64
                let prompt: String
                let scheduledAtUnixMs: Int64
            }
            let dueAgents: [DueAgent] = try query(
                """
                SELECT id, heartbeat_interval_seconds, heartbeat_prompt,
                       next_heartbeat_at_unix_ms
                FROM local_agent_profiles
                WHERE owner_user_id = ? AND status = 'active' AND heartbeat_enabled = 1
                  AND next_heartbeat_at_unix_ms IS NOT NULL
                  AND next_heartbeat_at_unix_ms <= ?
                ORDER BY next_heartbeat_at_unix_ms, id
                LIMIT ?
                """,
                [.text(ownerUserID), .integer(nowUnixMs), .integer(Int64(agentLimit))]
            ) { statement in
                DueAgent(
                    id: Self.string(statement, 0),
                    intervalSeconds: sqlite3_column_int64(statement, 1),
                    prompt: Self.string(statement, 2),
                    scheduledAtUnixMs: sqlite3_column_int64(statement, 3)
                )
            }
            var deliveries: [ProjectAgentDelivery] = []
            for agent in dueAgents {
                let outstanding = try AgentDeliveryRepository.outstandingCount(
                    database,
                    ownerUserID: ownerUserID,
                    targetAgentID: agent.id,
                    triggerKind: .heartbeat,
                    preparedStatement: recordPreparedStatement
                )
                if outstanding == 0 {
                    let directKey = "human:\(ownerUserID)|agent:\(agent.id)"
                    let room: ProjectAgentRoom
                    if let existing = try readDirectRoom(
                        ownerUserID: ownerUserID,
                        directKey: directKey
                    ) {
                        room = existing
                    } else {
                        guard let profile = try readAgent(
                            ownerUserID: ownerUserID,
                            agentID: agent.id
                        ) else { throw AgentGroupChatError.notFound }
                        let roomID = UUID().uuidString.lowercased()
                        room = ProjectAgentRoom(
                            id: roomID,
                            ownerUserID: ownerUserID,
                            projectID: "direct:\(roomID)",
                            draft: .init(name: profile.draft.name),
                            defaultAgentID: profile.id,
                            conversationKind: .humanAgentDirect,
                            directKey: directKey,
                            createdAtUnixMs: nowUnixMs,
                            updatedAtUnixMs: nowUnixMs
                        )
                        try room.validate()
                        try insertConversation(room)
                        try insertDirectMember(
                            ownerUserID: ownerUserID,
                            roomID: room.id,
                            agent: profile,
                            nowUnixMs: nowUnixMs
                        )
                    }
                    let messageID = UUID().uuidString.lowercased()
                    let deliveryID = UUID().uuidString.lowercased()
                    let content = agent.prompt.isEmpty
                        ? "主动巡检：读取全部未读消息，整理并继续处理自己的 TodoList。"
                        : agent.prompt
                    try execute(
                        """
                        INSERT INTO project_agent_messages (
                            owner_user_id, id, room_id, sender_kind, sender_id, content,
                            reply_to_message_id, source_run_id, causation_id, root_message_id,
                            hop_count, created_at_unix_ms
                        ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'heartbeat', ?, 0, ?)
                        """,
                        [
                            .text(ownerUserID), .text(messageID), .text(room.id), .text(content),
                            .text(messageID), .integer(nowUnixMs),
                        ]
                    )
                    try execute(
                        """
                        INSERT INTO project_agent_deliveries (
                            owner_user_id, id, room_id, message_id, root_message_id,
                            target_agent_id, trigger_kind, status, attempt, hop_count,
                            deduplication_key, response_message_id, last_error,
                            claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                        ) VALUES (?, ?, ?, ?, ?, ?, 'heartbeat', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
                        """,
                        [
                            .text(ownerUserID), .text(deliveryID), .text(room.id), .text(messageID),
                            .text(messageID), .text(agent.id),
                            .text("heartbeat:\(agent.id):\(agent.scheduledAtUnixMs)"),
                            .integer(nowUnixMs),
                        ]
                    )
                    guard let delivery = try readDelivery(
                        ownerUserID: ownerUserID,
                        deliveryID: deliveryID
                    ) else { throw AgentGroupChatError.storage("heartbeat delivery insert failed") }
                    deliveries.append(delivery)
                }
                let next = nowUnixMs + agent.intervalSeconds * 1_000
                try execute(
                    """
                    UPDATE local_agent_profiles
                    SET last_heartbeat_at_unix_ms = ?, next_heartbeat_at_unix_ms = ?
                    WHERE owner_user_id = ? AND id = ? AND status = 'active'
                      AND heartbeat_enabled = 1
                    """,
                    [.integer(nowUnixMs), .integer(next), .text(ownerUserID), .text(agent.id)]
                )
            }
            return deliveries
        }
    }

    /// Queues at most one highest-priority sourced Todo per Agent. The ordinary per-Agent delivery
    /// lock keeps Todo execution serial, while the source room restores the correct project tools.
    public func enqueuePendingAgentTodos(
        ownerUserID: String,
        nowUnixMs: Int64,
        agentLimit: Int = 32
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard nowUnixMs >= 0, (1...128).contains(agentLimit) else {
            throw AgentGroupChatError.invalidField("agentLimit")
        }
        return try transaction {
            let agentIDs: [String] = try query(
                """
                SELECT DISTINCT t.agent_id
                FROM local_agent_todos t
                JOIN local_agent_profiles a
                  ON a.owner_user_id = t.owner_user_id AND a.id = t.agent_id
                WHERE t.owner_user_id = ? AND t.status = 'pending'
                  AND t.team_room_id IS NOT NULL AND a.status = 'active'
                ORDER BY t.agent_id
                LIMIT ?
                """,
                [.text(ownerUserID), .integer(Int64(agentLimit))]
            ) { Self.string($0, 0) }
            var deliveries: [ProjectAgentDelivery] = []
            for agentID in agentIDs {
                let outstanding = try AgentDeliveryRepository.outstandingCount(
                    database,
                    ownerUserID: ownerUserID,
                    targetAgentID: agentID,
                    triggerKind: .todo,
                    preparedStatement: recordPreparedStatement
                )
                guard outstanding == 0 else { continue }
                guard let todo = try query(
                    """
                    SELECT \(Self.todoColumns) FROM local_agent_todos t
                    WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.status = 'pending'
                      AND EXISTS (
                        SELECT 1 FROM project_agent_rooms r
                        JOIN project_agent_room_members m
                          ON m.owner_user_id = r.owner_user_id AND m.room_id = r.id
                        WHERE r.owner_user_id = t.owner_user_id AND r.id = t.team_room_id
                          AND r.status = 'active' AND m.agent_id = t.agent_id
                          AND m.status = 'active'
                      )
                      AND NOT EXISTS (
                        SELECT 1
                        FROM local_agent_todo_dependencies dependency
                        JOIN local_agent_todos prerequisite
                          ON prerequisite.owner_user_id = dependency.owner_user_id
                         AND prerequisite.id = dependency.prerequisite_todo_id
                        WHERE dependency.owner_user_id = t.owner_user_id
                          AND dependency.todo_id = t.id
                          AND prerequisite.status != 'completed'
                      )
                    ORDER BY t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
                    LIMIT 1
                    """,
                    [.text(ownerUserID), .text(agentID)],
                    row: AgentGroupChatRowMapper.todo
                ).first else { continue }
                let roomID = todo.teamRoomID
                let messageID = UUID().uuidString.lowercased()
                let deliveryID = UUID().uuidString.lowercased()
                let rootMessageID = messageID
                try execute(
                    """
                    INSERT INTO project_agent_messages (
                        owner_user_id, id, room_id, sender_kind, sender_id, content,
                        reply_to_message_id, source_run_id, causation_id, root_message_id,
                        hop_count, created_at_unix_ms
                    ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'todo', ?, 0, ?)
                    """,
                    [
                        .text(ownerUserID), .text(messageID), .text(roomID),
                        .text(todo.detail.isEmpty ? todo.title : "\(todo.title)\n\n\(todo.detail)"),
                        .text(rootMessageID), .integer(nowUnixMs),
                    ]
                )
                try execute(
                    """
                    INSERT INTO project_agent_deliveries (
                        owner_user_id, id, room_id, message_id, root_message_id,
                        target_agent_id, trigger_kind, status, attempt, hop_count,
                        deduplication_key, response_message_id, last_error,
                        claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?, 'todo', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
                    """,
                    [
                        .text(ownerUserID), .text(deliveryID), .text(roomID), .text(messageID),
                        .text(rootMessageID), .text(agentID), .text("todo:\(todo.id)"),
                        .integer(nowUnixMs),
                    ]
                )
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET status = 'in_progress', updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND agent_id = ? AND id = ? AND status = 'pending'
                    """,
                    [.integer(nowUnixMs), .text(ownerUserID), .text(agentID), .text(todo.id)]
                )
                guard let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: deliveryID
                ) else { throw AgentGroupChatError.storage("todo delivery insert failed") }
                deliveries.append(delivery)
            }
            return deliveries
        }
    }

    public func agentTodoScheduleState(
        ownerUserID: String,
        agentID: String
    ) throws -> LocalAgentTodoScheduleState {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        return try transaction {
            guard try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let running = try query(
                """
                SELECT \(Self.todoColumns) FROM local_agent_todos t
                WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.status = 'in_progress'
                ORDER BY t.updated_at_unix_ms, t.id
                LIMIT 1
                """,
                [.text(ownerUserID), .text(agentID)],
                row: AgentGroupChatRowMapper.todo
            ).first
            let ready = try query(
                """
                SELECT \(Self.todoColumns) FROM local_agent_todos t
                WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.status = 'pending'
                  AND EXISTS (
                    SELECT 1 FROM project_agent_rooms r
                    JOIN project_agent_room_members m
                      ON m.owner_user_id = r.owner_user_id AND m.room_id = r.id
                    WHERE r.owner_user_id = t.owner_user_id AND r.id = t.team_room_id
                      AND r.status = 'active' AND m.agent_id = t.agent_id
                      AND m.status = 'active'
                  )
                  AND NOT EXISTS (
                    SELECT 1
                    FROM local_agent_todo_dependencies dependency
                    JOIN local_agent_todos prerequisite
                      ON prerequisite.owner_user_id = dependency.owner_user_id
                     AND prerequisite.id = dependency.prerequisite_todo_id
                    WHERE dependency.owner_user_id = t.owner_user_id
                      AND dependency.todo_id = t.id
                      AND prerequisite.status != 'completed'
                  )
                ORDER BY t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
                LIMIT 1
                """,
                [.text(ownerUserID), .text(agentID)],
                row: AgentGroupChatRowMapper.todo
            ).first
            return .init(runningTodo: running, readyTodo: ready)
        }
    }

    /// Explicit manager-cycle scheduling entry point. Unlike the legacy account-wide enqueue
    /// helper, this starts work only for the authenticated current Agent and does so atomically.
    public func startNextReadyAgentTodo(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let outstanding = try AgentDeliveryRepository.outstandingCount(
                database,
                ownerUserID: ownerUserID,
                targetAgentID: agentID,
                triggerKind: .todo,
                preparedStatement: recordPreparedStatement
            )
            let runningTodoCount = try scalarInt64(
                """
                SELECT COUNT(*) FROM local_agent_todos
                WHERE owner_user_id = ? AND agent_id = ? AND status = 'in_progress'
                """,
                [.text(ownerUserID), .text(agentID)]
            )
            guard outstanding == 0, runningTodoCount == 0 else { return nil }
            guard let todo = try query(
                """
                SELECT \(Self.todoColumns) FROM local_agent_todos t
                WHERE t.owner_user_id = ? AND t.agent_id = ? AND t.status = 'pending'
                  AND EXISTS (
                    SELECT 1 FROM project_agent_rooms r
                    JOIN project_agent_room_members m
                      ON m.owner_user_id = r.owner_user_id AND m.room_id = r.id
                    WHERE r.owner_user_id = t.owner_user_id AND r.id = t.team_room_id
                      AND r.status = 'active' AND m.agent_id = t.agent_id
                      AND m.status = 'active'
                  )
                  AND NOT EXISTS (
                    SELECT 1
                    FROM local_agent_todo_dependencies dependency
                    JOIN local_agent_todos prerequisite
                      ON prerequisite.owner_user_id = dependency.owner_user_id
                     AND prerequisite.id = dependency.prerequisite_todo_id
                    WHERE dependency.owner_user_id = t.owner_user_id
                      AND dependency.todo_id = t.id
                      AND prerequisite.status != 'completed'
                  )
                ORDER BY t.priority DESC, t.sort_order, t.created_at_unix_ms, t.id
                LIMIT 1
                """,
                [.text(ownerUserID), .text(agentID)],
                row: AgentGroupChatRowMapper.todo
            ).first else { return nil }

            let messageID = UUID().uuidString.lowercased()
            let deliveryID = UUID().uuidString.lowercased()
            try execute(
                """
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'todo', ?, 0, ?)
                """,
                [
                    .text(ownerUserID), .text(messageID), .text(todo.teamRoomID),
                    .text(todo.detail.isEmpty ? todo.title : "\(todo.title)\n\n\(todo.detail)"),
                    .text(messageID), .integer(nowUnixMs),
                ]
            )
            try execute(
                """
                INSERT INTO project_agent_deliveries (
                    owner_user_id, id, room_id, message_id, root_message_id,
                    target_agent_id, trigger_kind, status, attempt, hop_count,
                    deduplication_key, response_message_id, last_error,
                    claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, 'todo', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
                """,
                [
                    .text(ownerUserID), .text(deliveryID), .text(todo.teamRoomID),
                    .text(messageID), .text(messageID), .text(agentID),
                    .text("todo:\(todo.id)"), .integer(nowUnixMs),
                ]
            )
            try execute(
                """
                INSERT OR IGNORE INTO local_agent_todo_asset_snapshots (
                    owner_user_id, todo_id, asset_id, team_room_id, category,
                    title, markdown, revision, captured_at_unix_ms
                )
                SELECT owner_user_id, ?, id, team_room_id, category,
                       title, markdown, revision, ?
                FROM local_agent_team_assets
                WHERE owner_user_id = ? AND team_room_id = ? AND status = 'active'
                """,
                [
                    .text(todo.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(todo.teamRoomID),
                ]
            )
            try execute(
                """
                UPDATE local_agent_todos
                SET status = 'in_progress', updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND agent_id = ? AND id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(agentID), .text(todo.id)]
            )
            guard sqlite3_changes(database) == 1,
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: deliveryID
                  ) else { throw AgentGroupChatError.conflict }
            return delivery
        }
    }

    public func updateAgentProfile(
        ownerUserID: String,
        agentID: String,
        draft: LocalAgentProfileDraft
    ) throws -> LocalAgentProfile {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try draft.validate()
        return try transaction {
            guard let existing = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  existing.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = max(Self.now(), existing.updatedAtUnixMs)
            let nextHeartbeatAtUnixMs: Int64?
            if draft.heartbeatEnabled {
                if !existing.draft.heartbeatEnabled
                    || existing.draft.heartbeatIntervalSeconds != draft.heartbeatIntervalSeconds {
                    nextHeartbeatAtUnixMs = now + Int64(draft.heartbeatIntervalSeconds) * 1_000
                } else {
                    nextHeartbeatAtUnixMs = existing.nextHeartbeatAtUnixMs
                        ?? now + Int64(draft.heartbeatIntervalSeconds) * 1_000
                }
            } else {
                nextHeartbeatAtUnixMs = nil
            }
            try execute(
                """
                UPDATE local_agent_profiles
                SET name = ?, description = ?, role_prompt = ?, model_config_id = ?,
                    thinking_level = ?, profession_key = ?, default_plugin_ids_json = ?,
                    default_skill_ids_json = ?, heartbeat_enabled = ?,
                    heartbeat_interval_seconds = ?, heartbeat_prompt = ?,
                    next_heartbeat_at_unix_ms = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(draft.name), .text(draft.description), .text(draft.rolePrompt),
                    .text(draft.modelConfigID), draft.thinkingLevel.map(Value.text) ?? .null,
                    .text(draft.professionKey), .text(try encodeStrings(draft.defaultPluginIDs)),
                    .text(try encodeStrings(draft.defaultSkillIDs)),
                    .integer(draft.heartbeatEnabled ? 1 : 0),
                    .integer(Int64(draft.heartbeatIntervalSeconds)), .text(draft.heartbeatPrompt),
                    nextHeartbeatAtUnixMs.map(Value.integer) ?? .null, .integer(now),
                    .text(ownerUserID), .text(agentID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readAgent(ownerUserID: ownerUserID, agentID: agentID) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }

    public func updateAgentMembership(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        profileDraft: LocalAgentProfileDraft,
        memberDraft: ProjectAgentRoomMemberDraft
    ) throws -> LocalAgentMembershipUpdateResult {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        try profileDraft.validate()
        try memberDraft.validate()
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active,
                  let profile = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  profile.status == .active,
                  let member = try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agentID
                  ), member.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = max(Self.now(), profile.updatedAtUnixMs)
            try execute(
                """
                UPDATE local_agent_profiles
                SET name = ?, description = ?, role_prompt = ?, model_config_id = ?,
                    thinking_level = ?, profession_key = ?, default_plugin_ids_json = ?,
                    default_skill_ids_json = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(profileDraft.name), .text(profileDraft.description),
                    .text(profileDraft.rolePrompt), .text(profileDraft.modelConfigID),
                    profileDraft.thinkingLevel.map(Value.text) ?? .null,
                    .text(profileDraft.professionKey),
                    .text(try encodeStrings(profileDraft.defaultPluginIDs)),
                    .text(try encodeStrings(profileDraft.defaultSkillIDs)), .integer(now),
                    .text(ownerUserID), .text(agentID),
                ]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            try execute(
                """
                UPDATE project_agent_room_members
                SET role = ?, responsibility = ?, plugin_allowlist_json = ?
                WHERE owner_user_id = ? AND room_id = ? AND agent_id = ? AND status = 'active'
                """,
                [
                    .text(memberDraft.role), .text(memberDraft.responsibility),
                    .text(try encodeStrings(memberDraft.pluginAllowlist)), .text(ownerUserID),
                    .text(roomID), .text(agentID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let updatedProfile = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  let updatedMember = try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agentID
                  ) else {
                throw AgentGroupChatError.conflict
            }
            return .init(profile: updatedProfile, member: updatedMember)
        }
    }

    public func createAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentCreationProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  let profile = try readAgent(
                    ownerUserID: ownerUserID,
                    agentID: proposerAgentID
                  ), LocalAgentPermission.canManageStaff(profile.draft.defaultSkillIDs),
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == roomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readProposal(
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalAgentCreationProposal(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                draft: draft,
                createdAtUnixMs: nowUnixMs
            )
            try proposal.validate()
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(decoding: try encoder.encode(draft), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_agent_creation_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_agent_id,
                    created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(proposal.id), .text(roomID),
                    .text(proposerAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(draftJSON), .integer(nowUnixMs),
                ]
            )
            return proposal
        }
    }

    public func listAgentProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentCreationProposalStatus? = nil
    ) throws -> [LocalAgentCreationProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listCreationProposals(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentProposalApproval {
        try approveAgentProposal(
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            nowUnixMs: nowUnixMs,
            draftOverride: nil
        )
    }

    public func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64,
        resolvedDraft: LocalAgentDraft
    ) throws -> LocalAgentProposalApproval {
        try approveAgentProposal(
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            nowUnixMs: nowUnixMs,
            draftOverride: resolvedDraft
        )
    }

    private func approveAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64,
        draftOverride: LocalAgentDraft?
    ) throws -> LocalAgentProposalApproval {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active,
                  let proposal = try readProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ), proposal.status == .pending else {
                throw AgentGroupChatError.conflict
            }
            let approvedDraft = draftOverride ?? proposal.draft
            if draftOverride != nil, approvedDraft != proposal.draft {
                let storedModelConfigID = proposal.draft.modelConfigID
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                let expectedDraft = LocalAgentDraft(
                    name: proposal.draft.name,
                    role: proposal.draft.role,
                    responsibility: proposal.draft.responsibility,
                    rolePrompt: proposal.draft.rolePrompt,
                    modelConfigID: approvedDraft.modelConfigID,
                    thinkingLevel: approvedDraft.thinkingLevel,
                    professionKey: proposal.draft.professionKey,
                    rationale: proposal.draft.rationale
                )
                guard LocalAgentBuilderService.usesProposerModel(storedModelConfigID),
                      approvedDraft == expectedDraft else {
                    throw AgentGroupChatError.conflict
                }
            }
            try approvedDraft.validate()
            let agent = LocalAgentProfile(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                draft: approvedDraft.profileDraft,
                createdAtUnixMs: nowUnixMs,
                updatedAtUnixMs: nowUnixMs
            )
            try agent.validate()
            try execute(
                """
                INSERT INTO local_agent_profiles (
                    owner_user_id, id, name, description, role_prompt, model_config_id,
                    thinking_level, profession_key, default_plugin_ids_json,
                    default_skill_ids_json, heartbeat_enabled, heartbeat_interval_seconds,
                    heartbeat_prompt, last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
                    status, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 900, '', NULL, NULL, 'active', ?, ?)
                """,
                [
                    .text(ownerUserID), .text(agent.id), .text(agent.draft.name),
                    .text(agent.draft.description), .text(agent.draft.rolePrompt),
                    .text(agent.draft.modelConfigID),
                    agent.draft.thinkingLevel.map(Value.text) ?? .null,
                    .text(agent.draft.professionKey),
                    .text(try encodeStrings(agent.draft.defaultPluginIDs)),
                    .text(try encodeStrings(agent.draft.defaultSkillIDs)),
                    .integer(nowUnixMs), .integer(nowUnixMs),
                ]
            )
            var member: ProjectAgentRoomMember?
            if room.conversationKind == .projectTeam {
                let createdMember = ProjectAgentRoomMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: agent.id,
                    draft: approvedDraft.memberDraft,
                    joinedAtUnixMs: nowUnixMs
                )
                try createdMember.validate()
                try execute(
                    """
                    INSERT INTO project_agent_room_members (
                        owner_user_id, room_id, agent_id, role, responsibility,
                        plugin_allowlist_json, status, joined_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?, 'active', ?)
                    """,
                    [
                        .text(ownerUserID), .text(roomID), .text(agent.id),
                        .text(createdMember.draft.role), .text(createdMember.draft.responsibility),
                        .text(try encodeStrings(createdMember.draft.pluginAllowlist)), .integer(nowUnixMs),
                    ]
                )
                member = createdMember
                if room.defaultAgentID == nil {
                    try execute(
                        """
                        UPDATE project_agent_rooms SET default_agent_id = ?, updated_at_unix_ms = ?
                        WHERE owner_user_id = ? AND id = ? AND status = 'active'
                        """,
                        [.text(agent.id), .integer(nowUnixMs), .text(ownerUserID), .text(roomID)]
                    )
                }
            }
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(
                decoding: try encoder.encode(approvedDraft),
                as: UTF8.self
            )
            try execute(
                """
                UPDATE local_agent_creation_proposals
                SET draft_json = ?, status = 'approved', created_agent_id = ?, resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [
                    .text(draftJSON), .text(agent.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(proposalID), .text(roomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return .init(proposal: approved, agent: agent, member: member)
        }
    }

    public func rejectAgentProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_creation_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

    public func createAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRemovalProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentRemovalProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard proposerAgentID != draft.targetAgentID else {
            throw AgentGroupChatError.permissionDenied
        }
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: draft.targetAgentID
                  )?.status == .active,
                  let profile = try readAgent(
                    ownerUserID: ownerUserID,
                    agentID: proposerAgentID
                  ), LocalAgentPermission.canManageStaff(profile.draft.defaultSkillIDs),
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == roomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readRemovalProposal(
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalAgentRemovalProposal(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                draft: draft,
                createdAtUnixMs: nowUnixMs
            )
            try proposal.validate()
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(decoding: try encoder.encode(draft), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_agent_removal_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(proposal.id), .text(roomID),
                    .text(proposerAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(draftJSON), .integer(nowUnixMs),
                ]
            )
            return proposal
        }
    }

    public func listAgentRemovalProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalAgentRemovalProposalStatus? = nil
    ) throws -> [LocalAgentRemovalProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listRemovalProposals(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentRemovalProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active,
                  let proposal = try readRemovalProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ), proposal.status == .pending,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: proposal.draft.targetAgentID
                  )?.status == .active else {
                throw AgentGroupChatError.conflict
            }
            guard room.projectManagerAgentID != proposal.draft.targetAgentID else {
                // Project-management authority must be handed over explicitly before removal.
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = CASE WHEN status = 'pending' THEN 'cancelled' ELSE 'failed' END,
                    last_error = ?, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND room_id = ? AND target_agent_id = ?
                  AND status IN ('pending', 'running')
                """,
                [
                    .text("成员已由 Human 确认移出当前团队。"), .integer(nowUnixMs),
                    .text(ownerUserID), .text(roomID), .text(proposal.draft.targetAgentID),
                ]
            )
            try execute(
                """
                UPDATE project_agent_room_members SET status = 'removed'
                WHERE owner_user_id = ? AND room_id = ? AND agent_id = ? AND status = 'active'
                """,
                [.text(ownerUserID), .text(roomID), .text(proposal.draft.targetAgentID)]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            if room.defaultAgentID == proposal.draft.targetAgentID {
                let replacement: String? = try query(
                    """
                    SELECT agent_id FROM project_agent_room_members
                    WHERE owner_user_id = ? AND room_id = ? AND status = 'active'
                    ORDER BY joined_at_unix_ms, agent_id LIMIT 1
                    """,
                    [.text(ownerUserID), .text(roomID)]
                ) { Self.string($0, 0) }.first
                try execute(
                    """
                    UPDATE project_agent_rooms SET default_agent_id = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND id = ? AND status = 'active'
                    """,
                    [
                        .optionalText(replacement), .integer(nowUnixMs),
                        .text(ownerUserID), .text(roomID),
                    ]
                )
            }
            try execute(
                """
                UPDATE local_agent_removal_proposals
                SET status = 'approved', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readRemovalProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return approved
        }
    }

    public func rejectAgentRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentRemovalProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_removal_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readRemovalProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

    public func createMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentMembershipProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentMembershipProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: sourceRoomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: sourceRoomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: sourceRoomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  let proposer = try readAgent(
                    ownerUserID: ownerUserID,
                    agentID: proposerAgentID
                  ), proposer.status == .active,
                  LocalAgentPermission.canManageStaff(proposer.draft.defaultSkillIDs),
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == sourceRoomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            guard let targetRoom = try readRoom(
                ownerUserID: ownerUserID,
                roomID: draft.targetTeamRoomID
            ), targetRoom.status == .active,
               targetRoom.conversationKind == .projectTeam,
               try readAgent(
                ownerUserID: ownerUserID,
                agentID: draft.targetAgentID
               )?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            guard try readMember(
                ownerUserID: ownerUserID,
                roomID: draft.targetTeamRoomID,
                agentID: draft.targetAgentID
            )?.status != .active else { throw AgentGroupChatError.conflict }

            let proposal = LocalAgentMembershipProposal(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                draft: draft,
                createdAtUnixMs: nowUnixMs
            )
            try proposal.validate()
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(decoding: try encoder.encode(draft), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_agent_membership_proposals (
                    owner_user_id, id, source_room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(proposal.id), .text(sourceRoomID),
                    .text(proposerAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(draftJSON), .integer(nowUnixMs),
                ]
            )
            return proposal
        }
    }

    public func listMembershipProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentMembershipProposalStatus? = nil
    ) throws -> [LocalAgentMembershipProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: sourceRoomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listMembershipProposals(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentMembershipProposalApproval {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let proposal = try readMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposalID: proposalID
            ), proposal.status == .pending,
               let targetRoom = try readRoom(
                ownerUserID: ownerUserID,
                roomID: proposal.draft.targetTeamRoomID
               ), targetRoom.status == .active,
                  targetRoom.conversationKind == .projectTeam,
               let targetAgent = try readAgent(
                ownerUserID: ownerUserID,
                agentID: proposal.draft.targetAgentID
               ), targetAgent.status == .active else {
                throw AgentGroupChatError.conflict
            }
            let memberDraft = ProjectAgentRoomMemberDraft(
                role: proposal.draft.role,
                responsibility: proposal.draft.responsibility
            )
            let member: ProjectAgentRoomMember
            if let existing = try readMember(
                ownerUserID: ownerUserID,
                roomID: targetRoom.id,
                agentID: targetAgent.id
            ) {
                guard existing.status == .removed else { throw AgentGroupChatError.conflict }
                try execute(
                    """
                    UPDATE project_agent_room_members
                    SET role = ?, responsibility = ?, plugin_allowlist_json = '[]',
                        status = 'active', joined_at_unix_ms = ?
                    WHERE owner_user_id = ? AND room_id = ? AND agent_id = ?
                      AND status = 'removed'
                    """,
                    [
                        .text(memberDraft.role), .text(memberDraft.responsibility),
                        .integer(nowUnixMs), .text(ownerUserID), .text(targetRoom.id),
                        .text(targetAgent.id),
                    ]
                )
                guard sqlite3_changes(database) == 1,
                      let restored = try readMember(
                        ownerUserID: ownerUserID,
                        roomID: targetRoom.id,
                        agentID: targetAgent.id
                      ) else { throw AgentGroupChatError.conflict }
                member = restored
            } else {
                let created = ProjectAgentRoomMember(
                    ownerUserID: ownerUserID,
                    roomID: targetRoom.id,
                    agentID: targetAgent.id,
                    draft: memberDraft,
                    joinedAtUnixMs: nowUnixMs
                )
                try created.validate()
                try execute(
                    """
                    INSERT INTO project_agent_room_members (
                        owner_user_id, room_id, agent_id, role, responsibility,
                        plugin_allowlist_json, status, joined_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, '[]', 'active', ?)
                    """,
                    [
                        .text(ownerUserID), .text(targetRoom.id), .text(targetAgent.id),
                        .text(memberDraft.role), .text(memberDraft.responsibility),
                        .integer(nowUnixMs),
                    ]
                )
                member = created
            }
            let shouldAssignManager = targetRoom.projectManagerAgentID == nil
                && targetAgent.draft.professionKey == "project_manager"
            try execute(
                """
                UPDATE project_agent_rooms
                SET default_agent_id = COALESCE(default_agent_id, ?),
                    project_manager_agent_id = CASE
                        WHEN project_manager_agent_id IS NULL AND ? = 1 THEN ?
                        ELSE project_manager_agent_id
                    END,
                    updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'active'
                """,
                [
                    .text(targetAgent.id), .integer(shouldAssignManager ? 1 : 0),
                    .text(targetAgent.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(targetRoom.id),
                ]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            try execute(
                """
                UPDATE local_agent_membership_proposals
                SET status = 'approved', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ?
                  AND status = 'pending'
                """,
                [
                    .integer(nowUnixMs), .text(ownerUserID), .text(proposalID),
                    .text(sourceRoomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readMembershipProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ), let updatedRoom = try readRoom(
                    ownerUserID: ownerUserID,
                    roomID: targetRoom.id
                  ) else { throw AgentGroupChatError.conflict }
            return .init(proposal: approved, member: member, room: updatedRoom)
        }
    }

    public func rejectMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentMembershipProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_membership_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ?
                  AND status = 'pending'
                """,
                [
                    .integer(nowUnixMs), .text(ownerUserID), .text(proposalID),
                    .text(sourceRoomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readMembershipProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

    public func createTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentTeamCreationProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamCreationProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: sourceRoomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: sourceRoomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: sourceRoomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  let profile = try readAgent(
                    ownerUserID: ownerUserID,
                    agentID: proposerAgentID
                  ), LocalAgentPermission.canAccessLocalProjects(profile.draft.defaultSkillIDs),
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == sourceRoomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalAgentTeamCreationProposal(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                draft: draft,
                createdAtUnixMs: nowUnixMs
            )
            try proposal.validate()
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(decoding: try encoder.encode(draft), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_agent_team_creation_proposals (
                    owner_user_id, id, source_room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_room_id,
                    created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(proposal.id), .text(sourceRoomID),
                    .text(proposerAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(draftJSON), .integer(nowUnixMs),
                ]
            )
            return proposal
        }
    }

    public func listTeamProposals(
        ownerUserID: String,
        sourceRoomID: String,
        status: LocalAgentTeamCreationProposalStatus? = nil
    ) throws -> [LocalAgentTeamCreationProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: sourceRoomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listTeamProposals(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        resolvedProjectID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamProposalApproval {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        try AgentGroupChatValidation.identifier(resolvedProjectID, field: "resolvedProjectID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let proposal = try readTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: sourceRoomID,
                proposalID: proposalID
            ), proposal.status == .pending else {
                throw AgentGroupChatError.conflict
            }
            if let existingProjectID = proposal.draft.existingProjectID {
                guard existingProjectID == resolvedProjectID else {
                    throw AgentGroupChatError.permissionDenied
                }
            } else {
                guard proposal.draft.newProjectName != nil else {
                    throw AgentGroupChatError.conflict
                }
            }
            guard try readActiveRoom(
                ownerUserID: ownerUserID,
                projectID: resolvedProjectID
            ) == nil else { throw AgentGroupChatError.conflict }
            let room = ProjectAgentRoom(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                projectID: resolvedProjectID,
                draft: .init(
                    name: proposal.draft.teamName,
                    goal: proposal.draft.teamGoal
                ),
                createdAtUnixMs: nowUnixMs,
                updatedAtUnixMs: nowUnixMs
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
                    .text(ownerUserID), .text(room.id), .text(resolvedProjectID),
                    .text(room.draft.name), .text(room.draft.goal),
                    .integer(nowUnixMs), .integer(nowUnixMs),
                ]
            )
            try execute(
                """
                UPDATE local_agent_team_creation_proposals
                SET status = 'approved', created_room_id = ?, resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ? AND status = 'pending'
                """,
                [
                    .text(room.id), .integer(nowUnixMs), .text(ownerUserID),
                    .text(proposalID), .text(sourceRoomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readTeamProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return .init(proposal: approved, room: room)
        }
    }

    public func rejectTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(sourceRoomID, field: "sourceRoomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_team_creation_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND source_room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(sourceRoomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readTeamProposal(
                    ownerUserID: ownerUserID,
                    sourceRoomID: sourceRoomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
    }

    public func createProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalProjectCreationProposalDraft,
        nowUnixMs: Int64
    ) throws -> LocalProjectCreationProposal {
        try validateOwnerRoomAgent(
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: proposerAgentID
        )
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readRoom(ownerUserID: ownerUserID, roomID: roomID)?.status == .active,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: proposerAgentID
                  )?.status == .active,
                  let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: sourceDeliveryID
                  ), delivery.roomID == roomID,
                     delivery.targetAgentID == proposerAgentID,
                     delivery.status == .running else {
                throw AgentGroupChatError.permissionDenied
            }
            if let existing = try readProjectProposal(
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let proposal = LocalProjectCreationProposal(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                roomID: roomID,
                proposerAgentID: proposerAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                draft: draft,
                createdAtUnixMs: nowUnixMs
            )
            try proposal.validate()
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            let draftJSON = String(decoding: try encoder.encode(draft), as: UTF8.self)
            try execute(
                """
                INSERT INTO local_project_creation_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_project_id,
                    created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(proposal.id), .text(roomID),
                    .text(proposerAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(draftJSON), .integer(nowUnixMs),
                ]
            )
            return proposal
        }
    }

    public func listProjectProposals(
        ownerUserID: String,
        roomID: String,
        status: LocalProjectCreationProposalStatus? = nil
    ) throws -> [LocalProjectCreationProposal] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: roomID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentProposalRepository.listProjectProposals(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func approveProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        createdProjectID: String,
        nowUnixMs: Int64
    ) throws -> LocalProjectCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        try AgentGroupChatValidation.identifier(createdProjectID, field: "createdProjectID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_project_creation_proposals
                SET status = 'approved', created_project_id = ?, resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [
                    .text(createdProjectID), .integer(nowUnixMs), .text(ownerUserID),
                    .text(proposalID), .text(roomID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let approved = try readProjectProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return approved
        }
    }

    public func rejectProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String,
        nowUnixMs: Int64
    ) throws -> LocalProjectCreationProposal {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(proposalID, field: "proposalID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            try execute(
                """
                UPDATE local_project_creation_proposals
                SET status = 'rejected', resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND room_id = ? AND status = 'pending'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(proposalID), .text(roomID)]
            )
            guard sqlite3_changes(database) == 1,
                  let rejected = try readProjectProposal(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    proposalID: proposalID
                  ) else { throw AgentGroupChatError.conflict }
            return rejected
        }
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

    public func createManagedRoom(
        ownerUserID: String,
        projectID: String,
        draft: ProjectAgentRoomDraft,
        projectManagerAgentID: String
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(
            projectManagerAgentID,
            field: "projectManagerAgentID"
        )
        try draft.validate()
        return try transaction {
            guard try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID) == nil else {
                throw AgentGroupChatError.conflict
            }
            guard let manager = try readAgent(
                ownerUserID: ownerUserID,
                agentID: projectManagerAgentID
            ), manager.status == .active,
            manager.draft.professionKey == "project_manager" else {
                throw AgentGroupChatError.invalidField("projectManagerProfession")
            }
            let timestamp = Self.now()
            let room = ProjectAgentRoom(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: draft,
                defaultAgentID: projectManagerAgentID,
                projectManagerAgentID: projectManagerAgentID,
                createdAtUnixMs: timestamp,
                updatedAtUnixMs: timestamp
            )
            try room.validate()
            try insertConversation(room)
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agent: manager,
                nowUnixMs: timestamp
            )
            return room
        }
    }

    public func openHumanAgentDirect(
        ownerUserID: String,
        agentID: String
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        let directKey = "human:\(ownerUserID)|agent:\(agentID)"
        try AgentGroupChatValidation.identifier(directKey, field: "directKey")
        return try transaction {
            if let existing = try readDirectRoom(ownerUserID: ownerUserID, directKey: directKey) {
                return existing
            }
            guard let agent = try readAgent(ownerUserID: ownerUserID, agentID: agentID),
                  agent.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = Self.now()
            let roomID = UUID().uuidString.lowercased()
            let room = ProjectAgentRoom(
                id: roomID,
                ownerUserID: ownerUserID,
                projectID: "direct:\(roomID)",
                draft: .init(name: agent.draft.name),
                defaultAgentID: agentID,
                conversationKind: .humanAgentDirect,
                directKey: directKey,
                createdAtUnixMs: now,
                updatedAtUnixMs: now
            )
            try room.validate()
            try insertConversation(room)
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agent: agent,
                nowUnixMs: now
            )
            return room
        }
    }

    public func openAgentDirect(
        ownerUserID: String,
        initiatingAgentID: String,
        targetAgentID: String
    ) throws -> ProjectAgentRoom {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(initiatingAgentID, field: "initiatingAgentID")
        try AgentGroupChatValidation.identifier(targetAgentID, field: "targetAgentID")
        guard initiatingAgentID != targetAgentID else {
            throw AgentGroupChatError.invalidField("targetAgentID")
        }
        let pair = [initiatingAgentID, targetAgentID].sorted()
        let directKey = "agent:\(pair[0])|agent:\(pair[1])"
        try AgentGroupChatValidation.identifier(directKey, field: "directKey")
        return try transaction {
            if let existing = try readDirectRoom(ownerUserID: ownerUserID, directKey: directKey) {
                return existing
            }
            guard let first = try readAgent(ownerUserID: ownerUserID, agentID: pair[0]),
                  first.status == .active,
                  let second = try readAgent(ownerUserID: ownerUserID, agentID: pair[1]),
                  second.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let now = Self.now()
            let roomID = UUID().uuidString.lowercased()
            let room = ProjectAgentRoom(
                id: roomID,
                ownerUserID: ownerUserID,
                projectID: "direct:\(roomID)",
                draft: .init(name: "\(first.draft.name) · \(second.draft.name)"),
                conversationKind: .agentAgentDirect,
                directKey: directKey,
                createdAtUnixMs: now,
                updatedAtUnixMs: now
            )
            try room.validate()
            try insertConversation(room)
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agent: first,
                nowUnixMs: now
            )
            try insertDirectMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agent: second,
                nowUnixMs: now
            )
            return room
        }
    }

    public func room(ownerUserID: String, roomID: String) throws -> ProjectAgentRoom? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        return try readRoom(ownerUserID: ownerUserID, roomID: roomID)
    }

    public func activeRoom(ownerUserID: String, projectID: String) throws -> ProjectAgentRoom? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        return try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID)
    }

    public func listRooms(
        ownerUserID: String,
        includeArchived: Bool = false
    ) throws -> [ProjectAgentRoom] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentConversationRepository.listProjectRooms(
            database,
            ownerUserID: ownerUserID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listDirectConversations(
        ownerUserID: String,
        includeArchived: Bool = false
    ) throws -> [ProjectAgentRoom] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentConversationRepository.listDirectRooms(
            database,
            ownerUserID: ownerUserID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
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
            if let existing = try readMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID
            ) {
                guard existing.status == .removed else { throw AgentGroupChatError.conflict }
                let joinedAtUnixMs = Self.now()
                try execute(
                    """
                    UPDATE project_agent_room_members
                    SET role = ?, responsibility = ?, plugin_allowlist_json = ?,
                        status = 'active', joined_at_unix_ms = ?
                    WHERE owner_user_id = ? AND room_id = ? AND agent_id = ? AND status = 'removed'
                    """,
                    [
                        .text(draft.role), .text(draft.responsibility),
                        .text(try encodeStrings(draft.pluginAllowlist)), .integer(joinedAtUnixMs),
                        .text(ownerUserID), .text(roomID), .text(agentID),
                    ]
                )
                guard sqlite3_changes(database) == 1,
                      let restored = try readMember(
                        ownerUserID: ownerUserID,
                        roomID: roomID,
                        agentID: agentID
                      ) else { throw AgentGroupChatError.conflict }
                return restored
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
        return try AgentConversationRepository.listMembers(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            preparedStatement: recordPreparedStatement
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

    public func setProjectManager(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentRoom {
        try validateOwnerRoomAgent(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)
        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active, room.conversationKind == .projectTeam else {
                throw AgentGroupChatError.notFound
            }
            guard try readMember(
                ownerUserID: ownerUserID,
                roomID: roomID,
                agentID: agentID
            )?.status == .active else { throw AgentGroupChatError.notMember }
            guard try readAgent(
                ownerUserID: ownerUserID,
                agentID: agentID
            )?.draft.professionKey == "project_manager" else {
                throw AgentGroupChatError.invalidField("projectManagerProfession")
            }
            let now = max(Self.now(), room.updatedAtUnixMs)
            try execute(
                """
                UPDATE project_agent_rooms
                SET project_manager_agent_id = ?, updated_at_unix_ms = ?
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
        var attachmentDirectoryToRemove: URL?
        do {
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

            if !draft.attachmentItems.isEmpty {
                attachmentDirectoryToRemove = attachmentDirectoryURL(messageID: messageID)
            }
            let persistedAttachments = try persistMessageAttachments(
                ownerUserID: ownerUserID,
                messageID: messageID,
                drafts: draft.attachmentItems,
                queueAgentArtifacts: draft.senderKind == .agent
            )

            let message = ProjectAgentMessage(
                id: messageID,
                ownerUserID: ownerUserID,
                roomID: roomID,
                draft: draft,
                rootMessageID: rootMessageID,
                attachments: persistedAttachments,
                createdAtUnixMs: now
            )
            let candidates: [String]
            if room.conversationKind.isDirect {
                candidates = try query(
                    """
                    SELECT agent_id FROM project_agent_room_members
                    WHERE owner_user_id = ? AND room_id = ? AND status = 'active'
                    ORDER BY joined_at_unix_ms, agent_id
                    """,
                    [.text(ownerUserID), .text(roomID)]
                ) { Self.string($0, 0) }
            } else if !draft.mentionedAgentIDs.isEmpty {
                candidates = draft.mentionedAgentIDs
            } else if draft.senderKind == .human, let defaultAgentID = room.defaultAgentID {
                candidates = [defaultAgentID]
            } else {
                candidates = []
            }
            let targets = candidates.filter {
                !(draft.senderKind == .agent && $0 == draft.senderID)
            }
            let existingRunCount = try AgentDeliveryRepository.countForRootMessage(
                database,
                ownerUserID: ownerUserID,
                rootMessageID: rootMessageID,
                preparedStatement: recordPreparedStatement
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
        } catch {
            if let attachmentDirectoryToRemove {
                try? FileManager.default.removeItem(at: attachmentDirectoryToRemove)
            }
            throw error
        }
    }

    public func messageAttachment(
        ownerUserID: String,
        roomID: String,
        messageID: String,
        attachmentID: String
    ) async throws -> ProjectAgentMessageAttachmentPayload? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(messageID, field: "messageID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        guard let stored = try AgentAttachmentRepository.messageAttachment(
            database,
            ownerUserID: ownerUserID,
            messageID: messageID,
            attachmentID: attachmentID,
            roomID: roomID,
            preparedStatement: recordPreparedStatement
        ) else { return nil }
        let fileURL = try attachmentFileURL(relativePath: stored.relativePath)
        if !FileManager.default.fileExists(atPath: fileURL.path) {
            guard stored.attachment.syncStatus == .synced,
                  let artifactID = stored.attachment.artifactID,
                  let expectedSHA256 = stored.attachment.sha256,
                  let agentArtifactService else {
                throw AgentGroupChatError.storage("message attachment file is missing")
            }
            let data = try await agentArtifactService.download(artifactID: artifactID)
            guard data.count == stored.attachment.size,
                  Self.sha256(data) == expectedSHA256 else {
                throw AgentGroupChatError.storage("restored message attachment failed integrity check")
            }
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try data.write(to: fileURL, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: fileURL.path
            )
        }
        return ProjectAgentMessageAttachmentPayload(
            attachment: stored.attachment,
            localFileURL: fileURL
        )
    }

    public func claimNextAgentArtifactUpload(
        ownerUserID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentArtifactUploadJob? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard let candidate: AgentArtifactUploadCandidate = try transaction({
            guard let candidate = try AgentAttachmentRepository.nextUploadCandidate(
                database,
                ownerUserID: ownerUserID,
                nowUnixMs: nowUnixMs,
                preparedStatement: recordPreparedStatement
            ) else { return nil }
            try execute(
                """
                UPDATE project_agent_message_attachments
                SET sync_status = 'uploading', upload_attempt = ?, upload_error = NULL,
                    next_retry_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ?
                  AND sync_status IN ('queued', 'failed', 'uploading')
                  AND next_retry_at_unix_ms <= ?
                """,
                [
                    .integer(Int64(candidate.attempt)), .integer(nowUnixMs + 300_000),
                    .text(ownerUserID), .text(candidate.id), .integer(nowUnixMs),
                ]
            )
            return sqlite3_changes(database) == 1 ? candidate : nil
        }) else { return nil }

        let fileURL = try attachmentFileURL(relativePath: candidate.relativePath)
        guard let data = try? Data(contentsOf: fileURL, options: [.mappedIfSafe]),
              data.count == candidate.size,
              Self.sha256(data) == candidate.sha256 else {
            try markAgentArtifactUploadFailed(
                ownerUserID: ownerUserID,
                attachmentID: candidate.id,
                attempt: candidate.attempt,
                error: "本地文档缺失或完整性校验失败。",
                nowUnixMs: nowUnixMs
            )
            return nil
        }
        return ProjectAgentArtifactUploadJob(
            attachmentID: candidate.id,
            roomID: candidate.roomID,
            attempt: candidate.attempt,
            request: .init(
                name: candidate.name,
                mimeType: candidate.mimeType,
                data: data,
                sha256: candidate.sha256,
                idempotencyKey: "agent-attachment:\(candidate.id)"
            )
        )
    }

    public func markAgentArtifactUploadSynced(
        ownerUserID: String,
        attachmentID: String,
        metadata: AgentArtifactRemoteMetadata,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        try execute(
            """
            UPDATE project_agent_message_attachments
            SET sync_status = 'synced', artifact_id = ?, storage_provider = ?, bucket = ?,
                object_key = ?, remote_view_path = ?, upload_error = NULL,
                synced_at_unix_ms = ?, next_retry_at_unix_ms = 0
            WHERE owner_user_id = ? AND id = ? AND sync_status = 'uploading'
              AND sha256 = ? AND size_bytes = ?
            """,
            [
                .text(metadata.artifactID), .optionalText(metadata.storageProvider),
                .optionalText(metadata.bucket), .optionalText(metadata.objectKey),
                .optionalText(metadata.remoteViewPath), .integer(nowUnixMs),
                .text(ownerUserID), .text(attachmentID), .text(metadata.sha256),
                .integer(Int64(metadata.size)),
            ]
        )
        guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
    }

    public func markAgentArtifactUploadFailed(
        ownerUserID: String,
        attachmentID: String,
        attempt: Int,
        error: String,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        guard attempt > 0, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("artifactUploadFailure")
        }
        let exponent = min(10, attempt - 1)
        let delay = min(Int64(3_600_000), Int64(5_000) * Int64(1 << exponent))
        let safeError = String(error.prefix(512))
        try execute(
            """
            UPDATE project_agent_message_attachments
            SET sync_status = 'failed', upload_error = ?, next_retry_at_unix_ms = ?
            WHERE owner_user_id = ? AND id = ? AND sync_status = 'uploading'
            """,
            [
                .text(safeError), .integer(nowUnixMs + delay),
                .text(ownerUserID), .text(attachmentID),
            ]
        )
    }

    public func retryAgentArtifactUpload(
        ownerUserID: String,
        attachmentID: String
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        try execute(
            """
            UPDATE project_agent_message_attachments
            SET sync_status = 'queued', upload_error = NULL, next_retry_at_unix_ms = 0
            WHERE owner_user_id = ? AND id = ? AND sync_status = 'failed'
            """,
            [.text(ownerUserID), .text(attachmentID)]
        )
    }

    public func nextAgentArtifactSyncDue(ownerUserID: String) throws -> Int64? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentAttachmentRepository.nextSyncDue(
            database,
            ownerUserID: ownerUserID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func recordAgentMessageAttempt(
        ownerUserID: String,
        characterCount: Int,
        rejected: Bool,
        nowUnixMs: Int64
    ) throws {
        let policy = AgentCommunicationPolicy.standard
        let dimension: String
        switch characterCount {
        case ...policy.conciseMessageCharacters: dimension = "concise"
        case ...policy.recommendedMessageCharacters: dimension = "recommended"
        case ...policy.maximumMessageCharacters: dimension = "extended"
        default: dimension = "over_limit"
        }
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "message_length",
            dimension: dimension,
            value: Int64(max(characterCount, 0)),
            nowUnixMs: nowUnixMs
        )
        if rejected {
            try recordAgentToolRejection(
                ownerUserID: ownerUserID,
                reason: .messageTooLong,
                nowUnixMs: nowUnixMs
            )
        }
    }

    public func recordAgentDocumentCreation(
        ownerUserID: String,
        outcome: AgentDocumentCreationMetricOutcome,
        bytes: Int = 0,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "document_create",
            dimension: outcome.rawValue,
            value: Int64(max(bytes, 0)),
            nowUnixMs: nowUnixMs
        )
    }

    public func recordAgentArtifactUpload(
        ownerUserID: String,
        outcome: AgentArtifactUploadMetricOutcome,
        bytes: Int,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "artifact_upload",
            dimension: outcome.rawValue,
            value: Int64(max(bytes, 0)),
            nowUnixMs: nowUnixMs
        )
    }

    public func recordAgentDocumentPreview(
        ownerUserID: String,
        outcome: AgentDocumentPreviewMetricOutcome,
        durationMilliseconds: Int64,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "document_preview",
            dimension: outcome.rawValue,
            value: max(durationMilliseconds, 0),
            nowUnixMs: nowUnixMs
        )
    }

    public func recordAgentToolRejection(
        ownerUserID: String,
        reason: AgentCommunicationRejectionMetricReason,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "tool_rejection",
            dimension: reason.rawValue,
            value: 1,
            nowUnixMs: nowUnixMs
        )
    }

    public func agentCommunicationMetricSnapshot(
        ownerUserID: String
    ) throws -> [AgentCommunicationMetricRow] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try query(
            """
            SELECT metric_name, dimension, event_count, total_value, maximum_value,
                   updated_at_unix_ms
            FROM local_agent_communication_metrics
            WHERE owner_user_id = ?
            ORDER BY metric_name, dimension
            """,
            [.text(ownerUserID)]
        ) { statement in
            AgentCommunicationMetricRow(
                name: Self.string(statement, 0),
                dimension: Self.string(statement, 1),
                count: sqlite3_column_int64(statement, 2),
                totalValue: sqlite3_column_int64(statement, 3),
                maximumValue: sqlite3_column_int64(statement, 4),
                updatedAtUnixMs: sqlite3_column_int64(statement, 5)
            )
        }
    }

    private func recordAgentCommunicationMetric(
        ownerUserID: String,
        name: String,
        dimension: String,
        value: Int64,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard value >= 0, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("communicationMetric")
        }
        try execute(
            """
            INSERT INTO local_agent_communication_metrics (
                owner_user_id, metric_name, dimension, event_count, total_value,
                maximum_value, updated_at_unix_ms
            ) VALUES (?, ?, ?, 1, ?, ?, ?)
            ON CONFLICT(owner_user_id, metric_name, dimension) DO UPDATE SET
                event_count = event_count + 1,
                total_value = total_value + excluded.total_value,
                maximum_value = MAX(maximum_value, excluded.maximum_value),
                updated_at_unix_ms = excluded.updated_at_unix_ms
            """,
            [
                .text(ownerUserID), .text(name), .text(dimension), .integer(value),
                .integer(value), .integer(nowUnixMs),
            ]
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

    public func listTeamAssets(
        ownerUserID: String,
        teamRoomID: String,
        includeArchived: Bool = false
    ) throws -> [LocalAgentTeamAsset] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: teamRoomID)?.conversationKind
            == .projectTeam else { throw AgentGroupChatError.notFound }
        return try AgentTeamAssetRepository.list(
            database,
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
    }

    public func teamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String
    ) throws -> LocalAgentTeamAsset? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        try AgentGroupChatValidation.identifier(assetID, field: "teamAssetID")
        return try AgentTeamAssetRepository.find(
            database,
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            assetID: assetID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func upsertTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String?,
        editorAgentID: String?,
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String,
        expectedRevision: Int?,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamAsset {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        if let assetID { try AgentGroupChatValidation.identifier(assetID, field: "teamAssetID") }
        if let editorAgentID {
            try AgentGroupChatValidation.identifier(editorAgentID, field: "editorAgentID")
        }
        try AgentGroupChatValidation.text(title, field: "teamAssetTitle", maximumLength: 240)
        try AgentGroupChatValidation.optionalText(
            markdown,
            field: "teamAssetMarkdown",
            maximumLength: 128_000
        )
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let team = try readRoom(ownerUserID: ownerUserID, roomID: teamRoomID),
                  team.status == .active, team.conversationKind == .projectTeam else {
                throw AgentGroupChatError.notFound
            }
            if let editorAgentID {
                guard team.projectManagerAgentID == editorAgentID,
                      try readMember(
                        ownerUserID: ownerUserID,
                        roomID: teamRoomID,
                        agentID: editorAgentID
                      )?.status == .active else {
                    throw AgentGroupChatError.permissionDenied
                }
            }
            let resolvedID = assetID ?? UUID().uuidString.lowercased()
            let existing = try AgentTeamAssetRepository.find(
                database,
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                assetID: resolvedID,
                preparedStatement: recordPreparedStatement
            )
            let asset: LocalAgentTeamAsset
            if let existing {
                guard existing.status == .active,
                      let expectedRevision,
                      expectedRevision == existing.revision else {
                    throw AgentGroupChatError.conflict
                }
                asset = .init(
                    id: existing.id,
                    ownerUserID: existing.ownerUserID,
                    teamRoomID: existing.teamRoomID,
                    category: category,
                    title: title,
                    markdown: markdown,
                    revision: existing.revision + 1,
                    status: .active,
                    createdByAgentID: existing.createdByAgentID,
                    updatedByAgentID: editorAgentID,
                    createdAtUnixMs: existing.createdAtUnixMs,
                    updatedAtUnixMs: max(nowUnixMs, existing.createdAtUnixMs)
                )
                try execute(
                    """
                    UPDATE local_agent_team_assets
                    SET category = ?, title = ?, markdown = ?, revision = ?,
                        updated_by_agent_id = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND team_room_id = ? AND id = ?
                      AND status = 'active' AND revision = ?
                    """,
                    [
                        .text(category.rawValue), .text(title), .text(markdown),
                        .integer(Int64(asset.revision)), .optionalText(editorAgentID),
                        .integer(asset.updatedAtUnixMs), .text(ownerUserID), .text(teamRoomID),
                        .text(resolvedID), .integer(Int64(existing.revision)),
                    ]
                )
                guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            } else {
                guard assetID == nil, expectedRevision == nil else {
                    throw AgentGroupChatError.notFound
                }
                asset = .init(
                    id: resolvedID,
                    ownerUserID: ownerUserID,
                    teamRoomID: teamRoomID,
                    category: category,
                    title: title,
                    markdown: markdown,
                    revision: 1,
                    createdByAgentID: editorAgentID,
                    updatedByAgentID: editorAgentID,
                    createdAtUnixMs: nowUnixMs,
                    updatedAtUnixMs: nowUnixMs
                )
                try execute(
                    """
                    INSERT INTO local_agent_team_assets (
                        owner_user_id, id, team_room_id, category, title, markdown,
                        revision, status, created_by_agent_id, updated_by_agent_id,
                        created_at_unix_ms, updated_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?, 1, 'active', ?, ?, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(resolvedID), .text(teamRoomID),
                        .text(category.rawValue), .text(title), .text(markdown),
                        .optionalText(editorAgentID), .optionalText(editorAgentID),
                        .integer(nowUnixMs), .integer(nowUnixMs),
                    ]
                )
            }
            try asset.validate()
            try execute(
                """
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(asset.id), .integer(Int64(asset.revision)),
                    .text(asset.title), .text(asset.markdown),
                    .optionalText(editorAgentID), .integer(asset.updatedAtUnixMs),
                ]
            )
            return asset
        }
    }

    public func archiveTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        editorAgentID: String?,
        expectedRevision: Int,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamAsset {
        guard let existing = try teamAsset(
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            assetID: assetID
        ), existing.status == .active, existing.revision == expectedRevision else {
            throw AgentGroupChatError.conflict
        }
        if let editorAgentID {
            guard try readRoom(
                ownerUserID: ownerUserID,
                roomID: teamRoomID
            )?.projectManagerAgentID == editorAgentID else {
                throw AgentGroupChatError.permissionDenied
            }
        }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_team_assets
                SET status = 'archived', revision = revision + 1,
                    updated_by_agent_id = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND team_room_id = ? AND id = ?
                  AND status = 'active' AND revision = ?
                """,
                [
                    .optionalText(editorAgentID), .integer(nowUnixMs), .text(ownerUserID),
                    .text(teamRoomID), .text(assetID), .integer(Int64(expectedRevision)),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let archived = try teamAsset(
                    ownerUserID: ownerUserID,
                    teamRoomID: teamRoomID,
                    assetID: assetID
                  ) else { throw AgentGroupChatError.conflict }
            try execute(
                """
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(assetID), .integer(Int64(archived.revision)),
                    .text(archived.title), .text(archived.markdown),
                    .optionalText(editorAgentID), .integer(nowUnixMs),
                ]
            )
            return archived
        }
    }

    public func listTeamAssetRevisions(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        limit: Int = 100
    ) throws -> [LocalAgentTeamAssetRevision] {
        guard try teamAsset(
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            assetID: assetID
        ) != nil, (1...500).contains(limit) else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTeamAssetRepository.listRevisions(
            database,
            ownerUserID: ownerUserID,
            assetID: assetID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listTodoTeamAssetSnapshots(
        ownerUserID: String,
        todoID: String
    ) throws -> [LocalAgentTodoTeamAssetSnapshot] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        return try AgentTeamAssetRepository.listTodoSnapshots(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func todoTeamAssetSnapshot(
        ownerUserID: String,
        todoID: String,
        assetID: String,
        revision: Int
    ) throws -> LocalAgentTodoTeamAssetSnapshot? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        try AgentGroupChatValidation.identifier(assetID, field: "teamAssetID")
        guard revision > 0 else { throw AgentGroupChatError.invalidField("teamAssetRevision") }
        return try AgentTeamAssetRepository.todoSnapshot(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            assetID: assetID,
            revision: revision,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listAgentTodos(
        ownerUserID: String,
        agentID: String,
        includeTerminal: Bool = false
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        return try AgentTodoRepository.listForAgent(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            includeTerminal: includeTerminal,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        includeTerminal: Bool = false
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: teamRoomID)?.conversationKind
            == .projectTeam else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTodoRepository.listForTeam(
            database,
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            includeTerminal: includeTerminal,
            preparedStatement: recordPreparedStatement
        )
    }

    public func createAgentTodo(
        ownerUserID: String,
        agentID: String,
        requestKey: String,
        draft: LocalAgentTodoDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentTodo {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try AgentGroupChatValidation.text(draft.title, field: "todoTitle", maximumLength: 500)
        try AgentGroupChatValidation.optionalText(draft.detail, field: "todoDetail", maximumLength: 16_000)
        try draft.executionPlan.validate()
        let executionContract = draft.executionContract.normalized(
            title: draft.title,
            detail: draft.detail
        )
        try executionContract.validate()
        guard (0...100).contains(draft.priority), nowUnixMs >= 0,
              draft.dependencies.count <= 64,
              Set(draft.dependencies.map(\.prerequisiteTodoID)).count == draft.dependencies.count else {
            throw AgentGroupChatError.invalidField("todoPriority")
        }
        for dependency in draft.dependencies {
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteTodoID,
                field: "todoDependency"
            )
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteAgentID,
                field: "todoDependencyAgent"
            )
        }
        return try transaction {
            if let existing = try AgentTodoRepository.find(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                requestKey: requestKey,
                preparedStatement: recordPreparedStatement
            ) { return existing }
            guard try readAgent(ownerUserID: ownerUserID, agentID: agentID)?.status == .active else {
                throw AgentGroupChatError.notFound
            }
            let creatorAgentID = draft.creatorAgentID ?? agentID
            guard try readAgent(
                ownerUserID: ownerUserID,
                agentID: creatorAgentID
            )?.status == .active else { throw AgentGroupChatError.notFound }
            if let roomID = draft.sourceRoomID {
                guard try readMember(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    agentID: creatorAgentID
                )?.status == .active else { throw AgentGroupChatError.notMember }
                if let messageID = draft.sourceMessageID {
                    guard try readMessage(ownerUserID: ownerUserID, messageID: messageID)?.roomID == roomID else {
                        throw AgentGroupChatError.notFound
                    }
                }
            } else if draft.sourceMessageID != nil {
                throw AgentGroupChatError.invalidField("sourceMessageID")
            }
            for source in draft.additionalSources {
                try AgentGroupChatValidation.identifier(source.roomID, field: "sourceConversation")
                try AgentGroupChatValidation.identifier(source.messageID, field: "sourceMessage")
                guard try readMember(
                    ownerUserID: ownerUserID,
                    roomID: source.roomID,
                    agentID: creatorAgentID
                )?.status == .active,
                try readMessage(
                    ownerUserID: ownerUserID,
                    messageID: source.messageID
                )?.roomID == source.roomID else {
                    throw AgentGroupChatError.invalidField("sourceMessageRefs")
                }
            }
            let resolvedTeamRoomID: String
            if let requested = draft.teamRoomID {
                resolvedTeamRoomID = requested
            } else if let sourceRoomID = draft.sourceRoomID,
                      try readRoom(
                        ownerUserID: ownerUserID,
                        roomID: sourceRoomID
                      )?.conversationKind == .projectTeam {
                // Compatibility for trusted native callers. Model-facing tools always require a
                // team_ref and never infer the execution boundary from arbitrary message text.
                resolvedTeamRoomID = sourceRoomID
            } else {
                throw AgentGroupChatError.invalidField("teamRef")
            }
            guard let team = try readRoom(
                ownerUserID: ownerUserID,
                roomID: resolvedTeamRoomID
            ), team.status == .active, team.conversationKind == .projectTeam,
               (draft.creatorAgentID == nil
                   || team.projectManagerAgentID == creatorAgentID),
               try readMember(
                   ownerUserID: ownerUserID,
                   roomID: resolvedTeamRoomID,
                   agentID: agentID
               )?.status == .active else {
                throw AgentGroupChatError.invalidField("teamRef")
            }
            let nextOrder = (try query(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM local_agent_todos WHERE owner_user_id = ? AND agent_id = ?",
                [.text(ownerUserID), .text(agentID)]
            ) { sqlite3_column_int64($0, 0) }.first) ?? 0
            let todo = LocalAgentTodo(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                agentID: agentID,
                teamRoomID: resolvedTeamRoomID,
                sourceRoomID: draft.sourceRoomID,
                sourceMessageID: draft.sourceMessageID,
                title: draft.title,
                detail: draft.detail,
                priority: draft.priority,
                sortOrder: nextOrder,
                executionContract: executionContract,
                executionPlan: draft.executionPlan,
                createdAtUnixMs: nowUnixMs,
                updatedAtUnixMs: nowUnixMs
            )
            try todo.validate()
            try execute(
                """
                INSERT INTO local_agent_todos (
                    owner_user_id, id, agent_id, team_room_id, source_room_id, source_message_id,
                    request_key, title, detail, priority, sort_order, status,
                    blocked_reason, result, created_at_unix_ms, updated_at_unix_ms,
                    execution_plan_json, execution_contract_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', '', '', ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(todo.id), .text(agentID),
                    .text(resolvedTeamRoomID),
                    draft.sourceRoomID.map(Value.text) ?? .null,
                    draft.sourceMessageID.map(Value.text) ?? .null,
                    .text(requestKey), .text(draft.title), .text(draft.detail),
                    .integer(Int64(draft.priority)), .integer(nextOrder),
                    .integer(nowUnixMs), .integer(nowUnixMs),
                    .text(try encodeJSON(draft.executionPlan)),
                    .text(try encodeJSON(executionContract)),
                ]
            )
            var sources = draft.additionalSources
            if let roomID = draft.sourceRoomID, let messageID = draft.sourceMessageID {
                sources.insert(.init(roomID: roomID, messageID: messageID), at: 0)
            }
            var inserted = Set<String>()
            for source in sources {
                let key = "\(source.roomID)\u{0}\(source.messageID)\u{0}\(source.relation.rawValue)"
                guard inserted.insert(key).inserted else { continue }
                try execute(
                    """
                    INSERT INTO local_agent_todo_sources (
                        owner_user_id, todo_id, conversation_id, message_id, relation,
                        created_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(todo.id), .text(source.roomID),
                        .text(source.messageID), .text(source.relation.rawValue),
                        .integer(nowUnixMs),
                    ]
                )
            }
            _ = try replaceAgentTodoDependencies(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todo.id,
                dependencies: draft.dependencies,
                nowUnixMs: nowUnixMs
            )
            return todo
        }
    }

    public func updateAgentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        update: LocalAgentTodoUpdate,
        nowUnixMs: Int64
    ) throws -> LocalAgentTodo {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let existing = try readTodo(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID
            ) else { throw AgentGroupChatError.notFound }
            let title = update.title ?? existing.title
            let detail = update.detail ?? existing.detail
            let priority = update.priority ?? existing.priority
            let status = update.status ?? existing.status
            let blockedReason = update.blockedReason ?? existing.blockedReason
            let result = update.result ?? existing.result
            let executionContract = update.executionContract ?? existing.executionContract
            let revised = LocalAgentTodo(
                id: existing.id,
                ownerUserID: existing.ownerUserID,
                agentID: existing.agentID,
                teamRoomID: existing.teamRoomID,
                sourceRoomID: existing.sourceRoomID,
                sourceMessageID: existing.sourceMessageID,
                title: title,
                detail: detail,
                priority: priority,
                sortOrder: existing.sortOrder,
                status: status,
                blockedReason: blockedReason,
                result: result,
                executionContract: executionContract,
                executionPlan: existing.executionPlan,
                createdAtUnixMs: existing.createdAtUnixMs,
                updatedAtUnixMs: max(nowUnixMs, existing.createdAtUnixMs)
            )
            try revised.validate()
            let transitionAllowed: Bool
            if status == existing.status {
                transitionAllowed = true
            } else {
                transitionAllowed = switch (existing.status, status) {
                case (.pending, .inProgress), (.pending, .cancelled),
                     (.inProgress, .completed), (.inProgress, .blocked), (.inProgress, .cancelled),
                     (.blocked, .pending), (.blocked, .cancelled),
                     (.completed, .pending), (.cancelled, .pending):
                    true
                default:
                    false
                }
            }
            guard transitionAllowed else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE local_agent_todos
                SET title = ?, detail = ?, priority = ?, status = ?, blocked_reason = ?,
                    result = ?, execution_contract_json = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND agent_id = ? AND id = ? AND status = ?
                """,
                [
                    .text(title), .text(detail), .integer(Int64(priority)),
                    .text(status.rawValue), .text(blockedReason), .text(result),
                    .text(try encodeJSON(executionContract)), .integer(revised.updatedAtUnixMs),
                    .text(ownerUserID), .text(agentID),
                    .text(todoID), .text(existing.status.rawValue),
                ]
            )
            guard sqlite3_changes(database) == 1 else {
                throw AgentGroupChatError.conflict
            }
            if status == .cancelled, existing.status != .cancelled {
                try execute(
                    """
                    UPDATE project_agent_deliveries
                    SET status = 'cancelled', last_error = ?, completed_at_unix_ms = ?
                    WHERE owner_user_id = ? AND target_agent_id = ?
                      AND deduplication_key = ? AND trigger_kind = 'todo'
                      AND status IN ('pending', 'running')
                    """,
                    [
                        .text("Todo 已由项目经理停止。"), .integer(revised.updatedAtUnixMs),
                        .text(ownerUserID), .text(agentID), .text("todo:\(todoID)"),
                    ]
                )
            }
            return revised
        }
    }

    public func reorderAgentTodos(
        ownerUserID: String,
        agentID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifiers(todoIDs, field: "todoIDs", maximumCount: 500)
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            let active = try listAgentTodos(
                ownerUserID: ownerUserID,
                agentID: agentID,
                includeTerminal: false
            )
            let byID = Dictionary(uniqueKeysWithValues: active.map { ($0.id, $0) })
            guard todoIDs.allSatisfy({ byID[$0] != nil }) else {
                throw AgentGroupChatError.notFound
            }
            let requested = Set(todoIDs)
            let ordered = todoIDs + active.map(\.id).filter { !requested.contains($0) }
            for (offset, id) in ordered.enumerated() {
                let reorderedPriority = max(0, 100 - offset)
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET priority = ?, sort_order = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND agent_id = ? AND id = ?
                    """,
                    [
                        .integer(Int64(reorderedPriority)), .integer(Int64(offset)),
                        .integer(nowUnixMs), .text(ownerUserID), .text(agentID), .text(id),
                    ]
                )
            }
            return try listAgentTodos(
                ownerUserID: ownerUserID,
                agentID: agentID,
                includeTerminal: false
            )
        }
    }

    public func reorderTeamTodos(
        ownerUserID: String,
        teamRoomID: String,
        todoIDs: [String],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodo] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        try AgentGroupChatValidation.identifiers(todoIDs, field: "todoIDs", maximumCount: 500)
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            let active = try listTeamTodos(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                includeTerminal: false
            )
            let byID = Dictionary(uniqueKeysWithValues: active.map { ($0.id, $0) })
            guard todoIDs.allSatisfy({ byID[$0] != nil }) else {
                throw AgentGroupChatError.notFound
            }
            let requested = Set(todoIDs)
            let ordered = todoIDs + active.map(\.id).filter { !requested.contains($0) }
            for (offset, id) in ordered.enumerated() {
                let reorderedPriority = max(0, 100 - offset)
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET priority = ?, sort_order = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND team_room_id = ? AND id = ?
                    """,
                    [
                        .integer(Int64(reorderedPriority)), .integer(Int64(offset)),
                        .integer(nowUnixMs), .text(ownerUserID), .text(teamRoomID), .text(id),
                    ]
                )
            }
            return try listTeamTodos(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                includeTerminal: false
            )
        }
    }

    public func listAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> [LocalAgentTodoSourceLink] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTodoRepository.listSources(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> [LocalAgentTodoDependency] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTodoRepository.listDependencies(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func setAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        dependencies: [LocalAgentTodoDependencyDraft],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodoDependency] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0, dependencies.count <= 64,
              Set(dependencies.map(\.prerequisiteTodoID)).count == dependencies.count else {
            throw AgentGroupChatError.invalidField("todoDependencies")
        }
        for dependency in dependencies {
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteTodoID,
                field: "todoDependency"
            )
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteAgentID,
                field: "todoDependencyAgent"
            )
        }
        return try transaction {
            try replaceAgentTodoDependencies(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID,
                dependencies: dependencies,
                nowUnixMs: nowUnixMs
            )
        }
    }

    private func replaceAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        dependencies: [LocalAgentTodoDependencyDraft],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodoDependency] {
        guard let todo = try readTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        ) else { throw AgentGroupChatError.notFound }
        try execute(
            "DELETE FROM local_agent_todo_dependencies WHERE owner_user_id = ? AND todo_id = ?",
            [.text(ownerUserID), .text(todoID)]
        )
        for dependency in dependencies {
            guard dependency.prerequisiteTodoID != todoID,
                  let prerequisite = try readTodo(
                      ownerUserID: ownerUserID,
                      agentID: dependency.prerequisiteAgentID,
                      todoID: dependency.prerequisiteTodoID
                  ), prerequisite.teamRoomID == todo.teamRoomID else {
                throw AgentGroupChatError.invalidField("todoDependencies")
            }
            let createsCycle = try scalarInt64(
                """
                WITH RECURSIVE ancestors(id) AS (
                    SELECT prerequisite_todo_id
                    FROM local_agent_todo_dependencies
                    WHERE owner_user_id = ? AND todo_id = ?
                    UNION
                    SELECT dependency.prerequisite_todo_id
                    FROM local_agent_todo_dependencies dependency
                    JOIN ancestors ON dependency.todo_id = ancestors.id
                    WHERE dependency.owner_user_id = ?
                )
                SELECT COUNT(*) FROM ancestors WHERE id = ?
                """,
                [
                    .text(ownerUserID), .text(dependency.prerequisiteTodoID),
                    .text(ownerUserID), .text(todoID),
                ]
            ) > 0
            guard !createsCycle else {
                throw AgentGroupChatError.invalidField("todoDependencyCycle")
            }
            try execute(
                """
                INSERT INTO local_agent_todo_dependencies (
                    owner_user_id, todo_id, prerequisite_todo_id, created_at_unix_ms
                ) VALUES (?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(todoID),
                    .text(dependency.prerequisiteTodoID), .integer(nowUnixMs),
                ]
            )
        }
        return try listAgentTodoDependencies(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        )
    }

    public func linkAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        sources: [LocalAgentTodoSourceDraft],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodoSourceLink] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard !sources.isEmpty, sources.count <= 64, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("sourceMessageRefs")
        }
        return try transaction {
            guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
                throw AgentGroupChatError.notFound
            }
            for source in sources {
                try AgentGroupChatValidation.identifier(
                    source.roomID,
                    field: "sourceConversation"
                )
                try AgentGroupChatValidation.identifier(source.messageID, field: "sourceMessage")
                guard try readMessage(
                    ownerUserID: ownerUserID,
                    messageID: source.messageID
                )?.roomID == source.roomID else {
                    throw AgentGroupChatError.invalidField("sourceMessageRefs")
                }
                try execute(
                    """
                    INSERT OR IGNORE INTO local_agent_todo_sources (
                        owner_user_id, todo_id, conversation_id, message_id, relation,
                        created_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(todoID), .text(source.roomID),
                        .text(source.messageID), .text(source.relation.rawValue),
                        .integer(nowUnixMs),
                    ]
                )
            }
            return try listAgentTodoSources(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID
            )
        }
    }

    public func listAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        limit: Int
    ) throws -> [LocalAgentTodoProgress] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard (1...500).contains(limit),
              try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
            throw AgentGroupChatError.invalidField("todoProgressLimit")
        }
        return try AgentTodoRepository.listProgress(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    public func appendAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        kind: LocalAgentTodoProgressKind,
        runID: String?,
        stage: String,
        detail: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentTodoProgress {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        if let runID { try AgentGroupChatValidation.identifier(runID, field: "runID") }
        try AgentGroupChatValidation.optionalText(stage, field: "todoProgressStage", maximumLength: 240)
        try AgentGroupChatValidation.text(detail, field: "todoProgressDetail", maximumLength: 16_000)
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
                throw AgentGroupChatError.notFound
            }
            let sequence = (try query(
                """
                SELECT COALESCE(MAX(sequence), 0) + 1
                FROM local_agent_todo_events WHERE owner_user_id = ? AND todo_id = ?
                """,
                [.text(ownerUserID), .text(todoID)]
            ) { sqlite3_column_int64($0, 0) }.first) ?? 1
            let progress = LocalAgentTodoProgress(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID,
                sequence: sequence,
                kind: kind,
                runID: runID,
                stage: stage,
                detail: detail,
                createdAtUnixMs: nowUnixMs
            )
            try execute(
                """
                INSERT INTO local_agent_todo_events (
                    owner_user_id, id, todo_id, sequence, run_id, kind, stage, detail,
                    created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(progress.id), .text(todoID), .integer(sequence),
                    runID.map(Value.text) ?? .null, .text(kind.rawValue), .text(stage),
                    .text(detail), .integer(nowUnixMs),
                ]
            )
            return progress
        }
    }

    public func agentTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> LocalAgentTodo? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        return try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID)
    }

    public func todoForDelivery(
        ownerUserID: String,
        deliveryID: String
    ) throws -> LocalAgentTodo? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        guard let delivery = try readDelivery(ownerUserID: ownerUserID, deliveryID: deliveryID),
              delivery.triggerKind == .todo,
              delivery.deduplicationKey.hasPrefix("todo:") else { return nil }
        return try readTodo(
            ownerUserID: ownerUserID,
            agentID: delivery.targetAgentID,
            todoID: String(delivery.deduplicationKey.dropFirst("todo:".count))
        )
    }

    public func enqueueAgentTodoReady(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard let candidate = try readTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        ), candidate.status == .pending else { return nil }
        let room = try openHumanAgentDirect(ownerUserID: ownerUserID, agentID: agentID)
        return try transaction {
            guard let todo = try readTodo(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID
            ), todo.status == .pending else { return nil }
            let incomplete = try scalarInt64(
                """
                SELECT COUNT(*)
                FROM local_agent_todo_dependencies dependency
                JOIN local_agent_todos prerequisite
                  ON prerequisite.owner_user_id = dependency.owner_user_id
                 AND prerequisite.id = dependency.prerequisite_todo_id
                WHERE dependency.owner_user_id = ? AND dependency.todo_id = ?
                  AND prerequisite.status != 'completed'
                """,
                [.text(ownerUserID), .text(todoID)]
            )
            guard incomplete == 0 else { return nil }
            let eventKey = "ready:\(todo.id):\(todo.updatedAtUnixMs)"
            let key = "todo-ready:\(todo.id):\(todo.updatedAtUnixMs):\(agentID)"
            if let existing = try readDelivery(
                ownerUserID: ownerUserID,
                deduplicationKey: key
            ) {
                try insertTodoEventRecipient(
                    ownerUserID: ownerUserID,
                    eventKey: eventKey,
                    todoID: todo.id,
                    eventKind: "ready",
                    recipientAgentID: agentID,
                    deliveryID: existing.id,
                    messageID: existing.messageID,
                    nowUnixMs: nowUnixMs
                )
                return existing
            }
            let messageID = UUID().uuidString.lowercased()
            let deliveryID = UUID().uuidString.lowercased()
            let content = "Todo 已可执行：\(todo.title)\n优先级：\(todo.priority)"
            try execute(
                """
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'todo_status', ?, 0, ?)
                """,
                [
                    .text(ownerUserID), .text(messageID), .text(room.id), .text(content),
                    .text(messageID), .integer(nowUnixMs),
                ]
            )
            try execute(
                """
                INSERT INTO project_agent_deliveries (
                    owner_user_id, id, room_id, message_id, root_message_id,
                    target_agent_id, trigger_kind, status, attempt, hop_count,
                    deduplication_key, response_message_id, last_error,
                    claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, 'todo_status', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
                """,
                [
                    .text(ownerUserID), .text(deliveryID), .text(room.id), .text(messageID),
                    .text(messageID), .text(agentID), .text(key), .integer(nowUnixMs),
                ]
            )
            guard let delivery = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ) else { throw AgentGroupChatError.storage("Todo ready delivery insert failed") }
            try insertTodoEventRecipient(
                ownerUserID: ownerUserID,
                eventKey: eventKey,
                todoID: todo.id,
                eventKind: "ready",
                recipientAgentID: agentID,
                deliveryID: delivery.id,
                messageID: messageID,
                nowUnixMs: nowUnixMs
            )
            return delivery
        }
    }

    public func enqueueReadyDependentAgentTodos(
        ownerUserID: String,
        prerequisiteTodoID: String,
        nowUnixMs: Int64
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(prerequisiteTodoID, field: "prerequisiteTodoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        let dependents = try AgentTodoRepository.pendingDependents(
            database,
            ownerUserID: ownerUserID,
            prerequisiteTodoID: prerequisiteTodoID,
            preparedStatement: recordPreparedStatement
        )
        var deliveries: [ProjectAgentDelivery] = []
        for dependent in dependents {
            if let delivery = try enqueueAgentTodoReady(
                ownerUserID: ownerUserID,
                agentID: dependent.agentID,
                todoID: dependent.todoID,
                nowUnixMs: nowUnixMs
            ) {
                deliveries.append(delivery)
            }
        }
        return deliveries
    }

    public func enqueueAgentTodoStatus(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        excludingAgentID: String?,
        nowUnixMs: Int64
    ) throws -> [ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard let statusTodo = try readTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        ), let team = try readRoom(
            ownerUserID: ownerUserID,
            roomID: statusTodo.teamRoomID
        ) else {
            throw AgentGroupChatError.conflict
        }
        // Legacy trusted callers may have Todo rows created before teams acquired an explicit
        // project-manager binding. Model-facing creation always supplies `creatorAgentID` and
        // therefore cannot reach this compatibility fallback.
        let projectManagerAgentID = team.projectManagerAgentID ?? agentID
        if let excludingAgentID {
            try AgentGroupChatValidation.identifier(
                excludingAgentID,
                field: "excludingAgentID"
            )
        }
        let recipientIDs = Array(Set([agentID, projectManagerAgentID]))
            .filter { $0 != excludingAgentID }
            .sorted()
        var roomsByAgentID: [String: ProjectAgentRoom] = [:]
        for recipientID in recipientIDs {
            roomsByAgentID[recipientID] = try openHumanAgentDirect(
                ownerUserID: ownerUserID,
                agentID: recipientID
            )
        }
        return try transaction {
            guard let todo = try readTodo(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID
            ), todo.status == .completed || todo.status == .blocked || todo.status == .cancelled else {
                throw AgentGroupChatError.conflict
            }
            let summary = todo.status == .completed ? todo.result : todo.blockedReason
            let content = "Todo 状态已更新：\(todo.title)\n状态：\(todo.status.rawValue)\n\(summary)"
            let eventKey = "status:\(todo.id):\(todo.status.rawValue):\(todo.updatedAtUnixMs)"
            var deliveries: [ProjectAgentDelivery] = []
            for recipientID in recipientIDs {
                guard let room = roomsByAgentID[recipientID] else {
                    throw AgentGroupChatError.storage("Todo status room is missing")
                }
                let key = "todo-status:\(todo.id):\(todo.status.rawValue):\(todo.updatedAtUnixMs):\(recipientID)"
                if let existing = try readDelivery(
                    ownerUserID: ownerUserID,
                    deduplicationKey: key
                ) {
                    try insertTodoEventRecipient(
                        ownerUserID: ownerUserID,
                        eventKey: eventKey,
                        todoID: todo.id,
                        eventKind: todo.status.rawValue,
                        recipientAgentID: recipientID,
                        deliveryID: existing.id,
                        messageID: existing.messageID,
                        nowUnixMs: nowUnixMs
                    )
                    deliveries.append(existing)
                    continue
                }
                let messageID = UUID().uuidString.lowercased()
                let deliveryID = UUID().uuidString.lowercased()
                try execute(
                    """
                    INSERT INTO project_agent_messages (
                        owner_user_id, id, room_id, sender_kind, sender_id, content,
                        reply_to_message_id, source_run_id, causation_id, root_message_id,
                        hop_count, created_at_unix_ms
                    ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, 'todo_status', ?, 0, ?)
                    """,
                    [
                        .text(ownerUserID), .text(messageID), .text(room.id), .text(content),
                        .text(messageID), .integer(nowUnixMs),
                    ]
                )
                try execute(
                    """
                    INSERT INTO project_agent_deliveries (
                        owner_user_id, id, room_id, message_id, root_message_id,
                        target_agent_id, trigger_kind, status, attempt, hop_count,
                        deduplication_key, response_message_id, last_error,
                        claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?, 'todo_status', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
                    """,
                    [
                        .text(ownerUserID), .text(deliveryID), .text(room.id), .text(messageID),
                        .text(messageID), .text(recipientID), .text(key), .integer(nowUnixMs),
                    ]
                )
                guard let delivery = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: deliveryID
                ) else { throw AgentGroupChatError.storage("Todo status delivery insert failed") }
                try insertTodoEventRecipient(
                    ownerUserID: ownerUserID,
                    eventKey: eventKey,
                    todoID: todo.id,
                    eventKind: todo.status.rawValue,
                    recipientAgentID: recipientID,
                    deliveryID: delivery.id,
                    messageID: messageID,
                    nowUnixMs: nowUnixMs
                )
                deliveries.append(delivery)
            }
            return deliveries
        }
    }

    private func insertTodoEventRecipient(
        ownerUserID: String,
        eventKey: String,
        todoID: String,
        eventKind: String,
        recipientAgentID: String,
        deliveryID: String,
        messageID: String,
        nowUnixMs: Int64
    ) throws {
        try execute(
            """
            INSERT OR IGNORE INTO local_agent_todo_event_recipients (
                owner_user_id, event_key, todo_id, event_kind, recipient_agent_id,
                delivery_id, message_id, created_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [
                .text(ownerUserID), .text(eventKey), .text(todoID), .text(eventKind),
                .text(recipientAgentID), .text(deliveryID), .text(messageID),
                .integer(nowUnixMs),
            ]
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

    /// Loads message context for a run list in one query instead of issuing one query per run.
    public func messages(
        ownerUserID: String,
        messageIDs: [String]
    ) throws -> [String: ProjectAgentMessage] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        let ids = Array(Set(messageIDs)).sorted()
        guard ids.count <= 500 else { throw AgentGroupChatError.invalidField("messageIDs") }
        for id in ids {
            try AgentGroupChatValidation.identifier(id, field: "messageID")
        }
        let messages = try AgentMessageRepository.findMany(
            database,
            ownerUserID: ownerUserID,
            messageIDs: ids,
            preparedStatement: recordPreparedStatement,
            row: readMessage
        )
        return Dictionary(uniqueKeysWithValues: messages.map { ($0.id, $0) })
    }

    public func claimNextDelivery(
        ownerUserID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try claimNextDelivery(
            ownerUserID: ownerUserID,
            roomID: nil,
            agentID: agentID,
            nowUnixMs: nowUnixMs
        )
    }

    public func claimNextDelivery(
        ownerUserID: String,
        roomID: String,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        return try claimNextDelivery(
            ownerUserID: ownerUserID,
            roomID: Optional(roomID),
            agentID: agentID,
            nowUnixMs: nowUnixMs
        )
    }

    private func claimNextDelivery(
        ownerUserID: String,
        roomID: String?,
        agentID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let id = try AgentDeliveryRepository.nextPendingDeliveryID(
                database,
                ownerUserID: ownerUserID,
                agentID: agentID,
                roomID: roomID,
                preparedStatement: recordPreparedStatement
            ) else { return nil }
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

    /// Loads delivery context for run lists in one query. UI refreshes must not scale as N+1
    /// SQLite round trips as historical runs accumulate.
    public func deliveries(
        ownerUserID: String,
        deliveryIDs: [String]
    ) throws -> [String: ProjectAgentDelivery] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        let ids = Array(Set(deliveryIDs)).sorted()
        guard ids.count <= 500 else { throw AgentGroupChatError.invalidField("deliveryIDs") }
        for id in ids {
            try AgentGroupChatValidation.identifier(id, field: "deliveryID")
        }
        guard !ids.isEmpty else { return [:] }
        let deliveries = try AgentDeliveryRepository.deliveries(
            database,
            ownerUserID: ownerUserID,
            deliveryIDs: ids,
            preparedStatement: recordPreparedStatement
        )
        return Dictionary(uniqueKeysWithValues: deliveries.map { ($0.id, $0) })
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

    public func completeHeartbeatDelivery(
        ownerUserID: String,
        deliveryID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentDelivery {
        try validateDeliveryMutation(
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            nowUnixMs: nowUnixMs
        )
        return try transaction {
            guard let delivery = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ), delivery.status == .running else {
                throw AgentGroupChatError.conflict
            }
            try execute(
                """
                UPDATE project_agent_deliveries
                SET status = 'completed', response_message_id = NULL, completed_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'running'
                """,
                [.integer(nowUnixMs), .text(ownerUserID), .text(deliveryID)]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try readDelivery(
                    ownerUserID: ownerUserID,
                    deliveryID: deliveryID
                  ) else { throw AgentGroupChatError.conflict }
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
            guard let delivery = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ), delivery.status == .running else {
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
            guard sqlite3_changes(database) == 1 else {
                throw AgentGroupChatError.conflict
            }
            if delivery.triggerKind == .todo,
               delivery.deduplicationKey.hasPrefix("todo:") {
                let todoID = String(delivery.deduplicationKey.dropFirst("todo:".count))
                try execute(
                    """
                    UPDATE local_agent_todos
                    SET status = 'blocked', blocked_reason = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND agent_id = ? AND id = ?
                    """,
                    [
                        .text(error), .integer(nowUnixMs), .text(ownerUserID),
                        .text(delivery.targetAgentID), .text(todoID),
                    ]
                )
            }
            guard let updated = try readDelivery(
                ownerUserID: ownerUserID,
                deliveryID: deliveryID
            ) else {
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
            let deliveryIDs = try AgentDeliveryRepository.outstandingDeliveryIDs(
                database,
                ownerUserID: ownerUserID,
                roomID: roomID,
                preparedStatement: recordPreparedStatement
            )
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
        return try AgentRunRepository.listUnfinished(
            database,
            ownerUserID: ownerUserID,
            projectID: projectID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    /// Trigger Runs belong to an Agent, independent of whether their source is a private chat,
    /// team message, heartbeat, Todo, or Todo status change.
    public func listAgentRuns(
        ownerUserID: String,
        agentID: String,
        limit: Int
    ) throws -> [LocalAgentGroupChatRun] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        return try AgentRunRepository.listForAgent(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    /// Recent execution history for a team, independent of member count.
    public func listRoomRuns(
        ownerUserID: String,
        roomID: String,
        limit: Int
    ) throws -> [LocalAgentGroupChatRun] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        return try AgentRunRepository.listForRoom(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readActiveRoom(ownerUserID: String, projectID: String) throws -> ProjectAgentRoom? {
        try AgentConversationRepository.activeProjectRoom(
            database,
            ownerUserID: ownerUserID,
            projectID: projectID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readDirectRoom(ownerUserID: String, directKey: String) throws -> ProjectAgentRoom? {
        try AgentConversationRepository.directRoom(
            database,
            ownerUserID: ownerUserID,
            directKey: directKey,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readRoom(ownerUserID: String, roomID: String) throws -> ProjectAgentRoom? {
        try AgentConversationRepository.room(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readAgent(ownerUserID: String, agentID: String) throws -> LocalAgentProfile? {
        try AgentProfileRepository.find(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readMember(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentRoomMember? {
        try AgentConversationRepository.member(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agentID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func insertConversation(_ room: ProjectAgentRoom) throws {
        try execute(
            """
            INSERT INTO project_agent_rooms (
                owner_user_id, id, project_id, name, goal, default_agent_id, status,
                created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                project_manager_agent_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [
                .text(room.ownerUserID), .text(room.id), .text(room.projectID),
                .text(room.draft.name), .text(room.draft.goal),
                .optionalText(room.defaultAgentID), .text(room.status.rawValue),
                .integer(room.createdAtUnixMs), .integer(room.updatedAtUnixMs),
                .text(room.conversationKind.rawValue), .optionalText(room.directKey),
                .optionalText(room.projectManagerAgentID),
            ]
        )
    }

    private func insertDirectMember(
        ownerUserID: String,
        roomID: String,
        agent: LocalAgentProfile,
        nowUnixMs: Int64
    ) throws {
        try execute(
            """
            INSERT INTO project_agent_room_members (
                owner_user_id, room_id, agent_id, role, responsibility,
                plugin_allowlist_json, status, joined_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, '[]', 'active', ?)
            """,
            [
                .text(ownerUserID), .text(roomID), .text(agent.id), .text(agent.draft.name),
                .text(agent.draft.description), .integer(nowUnixMs),
            ]
        )
    }

    private func readMessage(ownerUserID: String, messageID: String) throws -> ProjectAgentMessage? {
        try AgentMessageRepository.find(
            database,
            ownerUserID: ownerUserID,
            messageID: messageID,
            preparedStatement: recordPreparedStatement,
            row: readMessage
        )
    }

    private func readCursor(
        ownerUserID: String,
        roomID: String,
        agentID: String
    ) throws -> ProjectAgentReadCursor? {
        try AgentReadCursorRepository.find(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            agentID: agentID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readTodo(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> LocalAgentTodo? {
        try AgentTodoRepository.find(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String
    ) throws -> LocalAgentCreationProposal? {
        try AgentProposalRepository.creationProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String
    ) throws -> LocalProjectCreationProposal? {
        try AgentProposalRepository.projectProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposalID: String
    ) throws -> LocalAgentRemovalProposal? {
        try AgentProposalRepository.removalProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String
    ) throws -> LocalAgentTeamCreationProposal? {
        try AgentProposalRepository.teamProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposalID: String
    ) throws -> LocalAgentMembershipProposal? {
        try AgentProposalRepository.membershipProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposalID: proposalID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readTeamProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentTeamCreationProposal? {
        try AgentProposalRepository.teamProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readMembershipProposal(
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentMembershipProposal? {
        try AgentProposalRepository.membershipProposal(
            database,
            ownerUserID: ownerUserID,
            sourceRoomID: sourceRoomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readRemovalProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentRemovalProposal? {
        try AgentProposalRepository.removalProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readProjectProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalProjectCreationProposal? {
        try AgentProposalRepository.projectProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readProposal(
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String
    ) throws -> LocalAgentCreationProposal? {
        try AgentProposalRepository.creationProposal(
            database,
            ownerUserID: ownerUserID,
            roomID: roomID,
            proposerAgentID: proposerAgentID,
            sourceDeliveryID: sourceDeliveryID,
            requestKey: requestKey,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readDelivery(ownerUserID: String, deliveryID: String) throws -> ProjectAgentDelivery? {
        try AgentDeliveryRepository.delivery(
            database,
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readDelivery(
        ownerUserID: String,
        deduplicationKey: String
    ) throws -> ProjectAgentDelivery? {
        try AgentDeliveryRepository.delivery(
            database,
            ownerUserID: ownerUserID,
            deduplicationKey: deduplicationKey,
            preparedStatement: recordPreparedStatement
        )
    }

    private func readRun(
        ownerUserID: String,
        deliveryID: String
    ) throws -> LocalAgentGroupChatRun? {
        try AgentRunRepository.run(
            database,
            ownerUserID: ownerUserID,
            deliveryID: deliveryID,
            preparedStatement: recordPreparedStatement
        )
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

    private func readMessage(_ statement: OpaquePointer) throws -> ProjectAgentMessage {
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

    private func persistMessageAttachments(
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

    private func attachmentDirectoryURL(messageID: String) -> URL {
        attachmentsRootURL.appendingPathComponent(messageID, isDirectory: true)
    }

    private func attachmentFileURL(relativePath: String) throws -> URL {
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

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private typealias Value = AgentGroupChatDatabase.Value

    private func query<T>(
        _ sql: String,
        _ values: [Value] = [],
        row: (OpaquePointer) throws -> T
    ) throws -> [T] {
        recordPreparedStatement()
        return try AgentGroupChatDatabase.query(database, sql, values, row: row)
    }

    private func execute(_ sql: String, _ values: [Value] = []) throws {
        recordPreparedStatement()
        try AgentGroupChatDatabase.execute(database, sql, values)
    }

    private func scalarInt64(_ sql: String, _ values: [Value]) throws -> Int64 {
        recordPreparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(database, sql, values)
    }

    private func transaction<T>(_ body: () throws -> T) throws -> T {
        try AgentGroupChatDatabase.transaction(
            database,
            preparedStatement: recordPreparedStatement,
            body: body
        )
    }

    private func recordPreparedStatement() {
#if DEBUG
        debugPreparedStatementCount += 1
#endif
    }

    private func encodeStrings(_ values: [String]) throws -> String {
        String(decoding: try JSONEncoder().encode(values), as: UTF8.self)
    }

    private func encodeJSON<Value: Encodable>(_ value: Value) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return String(decoding: try encoder.encode(value), as: UTF8.self)
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

    private static let messageColumns = "owner_user_id, id, room_id, sender_kind, sender_id, content, reply_to_message_id, source_run_id, causation_id, root_message_id, hop_count, created_at_unix_ms"
    private static let todoColumns = "owner_user_id, id, agent_id, team_room_id, source_room_id, source_message_id, title, detail, priority, sort_order, request_key, status, blocked_reason, result, created_at_unix_ms, updated_at_unix_ms, execution_plan_json, execution_contract_json"
}
