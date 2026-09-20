import ChatOSCore
import SQLite3

enum AgentGroupChatMigrations {
    static func migrateConversationSchema(_ handle: OpaquePointer?) throws {
        guard let handle else { throw AgentGroupChatError.storage("database unavailable") }
        func hasColumn(_ name: String, table: String = "project_agent_rooms") -> Bool {
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(handle, "PRAGMA table_info(\(table))", -1, &statement, nil) == SQLITE_OK,
                  let statement else { return false }
            defer { sqlite3_finalize(statement) }
            while sqlite3_step(statement) == SQLITE_ROW {
                guard let value = sqlite3_column_text(statement, 1) else { continue }
                if String(cString: value) == name { return true }
            }
            return false
        }
        func execute(_ sql: String) throws {
            guard sqlite3_exec(handle, sql, nil, nil, nil) == SQLITE_OK else {
                throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(handle)))
            }
        }
        func hasMigration(_ version: Int) -> Bool {
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(
                handle,
                "SELECT 1 FROM local_agent_group_chat_schema_migrations WHERE version = ? LIMIT 1",
                -1,
                &statement,
                nil
            ) == SQLITE_OK, let statement else { return false }
            defer { sqlite3_finalize(statement) }
            guard sqlite3_bind_int64(statement, 1, Int64(version)) == SQLITE_OK else {
                return false
            }
            return sqlite3_step(statement) == SQLITE_ROW
        }
        if !hasColumn("conversation_kind") {
            try execute(
                "ALTER TABLE project_agent_rooms ADD COLUMN conversation_kind TEXT NOT NULL DEFAULT 'project_team'"
            )
        }
        if !hasColumn("direct_key") {
            try execute("ALTER TABLE project_agent_rooms ADD COLUMN direct_key TEXT")
        }
        if !hasColumn("profession_key", table: "local_agent_profiles") {
            try execute(
                "ALTER TABLE local_agent_profiles ADD COLUMN profession_key TEXT NOT NULL DEFAULT 'general_member'"
            )
        }
        if !hasColumn("thinking_level", table: "local_agent_profiles") {
            try execute("ALTER TABLE local_agent_profiles ADD COLUMN thinking_level TEXT")
        }
        if !hasColumn("heartbeat_enabled", table: "local_agent_profiles") {
            try execute(
                "ALTER TABLE local_agent_profiles ADD COLUMN heartbeat_enabled INTEGER NOT NULL DEFAULT 0"
            )
        }
        if !hasColumn("heartbeat_interval_seconds", table: "local_agent_profiles") {
            try execute(
                "ALTER TABLE local_agent_profiles ADD COLUMN heartbeat_interval_seconds INTEGER NOT NULL DEFAULT 900"
            )
        }
        if !hasColumn("heartbeat_prompt", table: "local_agent_profiles") {
            try execute(
                "ALTER TABLE local_agent_profiles ADD COLUMN heartbeat_prompt TEXT NOT NULL DEFAULT ''"
            )
        }
        if !hasColumn("last_heartbeat_at_unix_ms", table: "local_agent_profiles") {
            try execute("ALTER TABLE local_agent_profiles ADD COLUMN last_heartbeat_at_unix_ms INTEGER")
        }
        if !hasColumn("next_heartbeat_at_unix_ms", table: "local_agent_profiles") {
            try execute("ALTER TABLE local_agent_profiles ADD COLUMN next_heartbeat_at_unix_ms INTEGER")
        }
        try execute(
            """
            CREATE UNIQUE INDEX IF NOT EXISTS one_active_direct_conversation_per_pair
            ON project_agent_rooms(owner_user_id, direct_key)
            WHERE status = 'active' AND direct_key IS NOT NULL
            """
        )
        try execute(
            "INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (8)"
        )
        try execute(
            "INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (9)"
        )
        try execute(
            "INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (10)"
        )
        try execute(
            "INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (11)"
        )
        if !hasMigration(12) {
            try execute(
                """
                UPDATE local_agent_profiles AS recruited
                SET thinking_level = (
                    SELECT proposer.thinking_level
                    FROM local_agent_creation_proposals AS proposal
                    JOIN local_agent_profiles AS proposer
                      ON proposer.owner_user_id = proposal.owner_user_id
                     AND proposer.id = proposal.proposer_agent_id
                    WHERE proposal.owner_user_id = recruited.owner_user_id
                      AND proposal.created_agent_id = recruited.id
                      AND proposal.status = 'approved'
                      AND proposer.model_config_id = recruited.model_config_id
                      AND proposer.thinking_level IS NOT NULL
                    ORDER BY proposal.resolved_at_unix_ms DESC
                    LIMIT 1
                )
                WHERE recruited.thinking_level IS NULL
                  AND EXISTS (
                    SELECT 1
                    FROM local_agent_creation_proposals AS proposal
                    JOIN local_agent_profiles AS proposer
                      ON proposer.owner_user_id = proposal.owner_user_id
                     AND proposer.id = proposal.proposer_agent_id
                    WHERE proposal.owner_user_id = recruited.owner_user_id
                      AND proposal.created_agent_id = recruited.id
                      AND proposal.status = 'approved'
                      AND proposer.model_config_id = recruited.model_config_id
                      AND proposer.thinking_level IS NOT NULL
                  )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (12)"
            )
        }
        if !hasMigration(13) {
            try execute("PRAGMA foreign_keys = OFF")
            do {
                try execute("BEGIN IMMEDIATE")
                try execute(
                    """
                    CREATE TABLE project_agent_deliveries_v13 (
                        owner_user_id TEXT NOT NULL,
                        id TEXT NOT NULL,
                        room_id TEXT NOT NULL,
                        message_id TEXT NOT NULL,
                        root_message_id TEXT NOT NULL,
                        target_agent_id TEXT NOT NULL,
                        trigger_kind TEXT NOT NULL CHECK(trigger_kind IN (
                            'mention', 'default_agent', 'agent_mention', 'heartbeat', 'todo'
                        )),
                        status TEXT NOT NULL CHECK(status IN (
                            'pending', 'running', 'completed', 'failed', 'cancelled'
                        )),
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
                    )
                    """
                )
                try execute(
                    """
                    INSERT INTO project_agent_deliveries_v13
                    SELECT * FROM project_agent_deliveries
                    """
                )
                try execute("DROP TABLE project_agent_deliveries")
                try execute(
                    "ALTER TABLE project_agent_deliveries_v13 RENAME TO project_agent_deliveries"
                )
                try execute(
                    """
                    CREATE INDEX project_agent_deliveries_agent_queue
                    ON project_agent_deliveries(
                        owner_user_id, target_agent_id, status, created_at_unix_ms, id
                    )
                    """
                )
                try execute(
                    """
                    CREATE UNIQUE INDEX one_running_delivery_per_agent
                    ON project_agent_deliveries(owner_user_id, target_agent_id)
                    WHERE status = 'running'
                    """
                )
                try execute(
                    "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (13)"
                )
                try execute("COMMIT")
            } catch {
                try? execute("ROLLBACK")
                try? execute("PRAGMA foreign_keys = ON")
                throw error
            }
            try execute("PRAGMA foreign_keys = ON")
        }
        if !hasMigration(14) {
            try execute("PRAGMA foreign_keys = OFF")
            do {
                try execute("BEGIN IMMEDIATE")
                try execute(
                    """
                    CREATE TABLE project_agent_deliveries_v14 (
                        owner_user_id TEXT NOT NULL,
                        id TEXT NOT NULL,
                        room_id TEXT NOT NULL,
                        message_id TEXT NOT NULL,
                        root_message_id TEXT NOT NULL,
                        target_agent_id TEXT NOT NULL,
                        trigger_kind TEXT NOT NULL CHECK(trigger_kind IN (
                            'mention', 'default_agent', 'agent_mention', 'heartbeat', 'todo'
                        )),
                        status TEXT NOT NULL CHECK(status IN (
                            'pending', 'running', 'completed', 'failed', 'cancelled'
                        )),
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
                    )
                    """
                )
                try execute(
                    """
                    INSERT INTO project_agent_deliveries_v14
                    SELECT * FROM project_agent_deliveries
                    """
                )
                try execute("DROP TABLE project_agent_deliveries")
                try execute(
                    "ALTER TABLE project_agent_deliveries_v14 RENAME TO project_agent_deliveries"
                )
                try execute(
                    """
                    CREATE INDEX project_agent_deliveries_agent_queue
                    ON project_agent_deliveries(
                        owner_user_id, target_agent_id, status, created_at_unix_ms, id
                    )
                    """
                )
                try execute(
                    """
                    CREATE UNIQUE INDEX one_running_delivery_per_agent
                    ON project_agent_deliveries(owner_user_id, target_agent_id)
                    WHERE status = 'running'
                    """
                )
                try execute(
                    "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (14)"
                )
                try execute("COMMIT")
            } catch {
                try? execute("ROLLBACK")
                try? execute("PRAGMA foreign_keys = ON")
                throw error
            }
            try execute("PRAGMA foreign_keys = ON")
        }
        if !hasMigration(15) {
            if !hasColumn("team_room_id", table: "local_agent_todos") {
                try execute("ALTER TABLE local_agent_todos ADD COLUMN team_room_id TEXT")
            }
            if !hasColumn("execution_plan_json", table: "local_agent_todos") {
                try execute(
                    """
                    ALTER TABLE local_agent_todos ADD COLUMN execution_plan_json TEXT NOT NULL
                    DEFAULT '{"requiresExecution":true,"builtinCapabilities":["project_read"],"plugins":[],"selectionRevision":"local-v1","selectedAtUnixMs":0}'
                    """
                )
            }
            try execute(
                """
                UPDATE local_agent_todos
                SET team_room_id = source_room_id
                WHERE team_room_id IS NULL AND source_room_id IN (
                    SELECT id FROM project_agent_rooms
                    WHERE owner_user_id = local_agent_todos.owner_user_id
                      AND conversation_kind = 'project_team'
                )
                """
            )
            try execute("PRAGMA foreign_keys = OFF")
            do {
                try execute("BEGIN IMMEDIATE")
                try execute(
                    """
                    CREATE TABLE project_agent_deliveries_v15 (
                        owner_user_id TEXT NOT NULL,
                        id TEXT NOT NULL,
                        room_id TEXT NOT NULL,
                        message_id TEXT NOT NULL,
                        root_message_id TEXT NOT NULL,
                        target_agent_id TEXT NOT NULL,
                        trigger_kind TEXT NOT NULL CHECK(trigger_kind IN (
                            'mention', 'default_agent', 'agent_mention', 'heartbeat', 'todo',
                            'todo_status'
                        )),
                        status TEXT NOT NULL CHECK(status IN (
                            'pending', 'running', 'completed', 'failed', 'cancelled'
                        )),
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
                    )
                    """
                )
                try execute("INSERT INTO project_agent_deliveries_v15 SELECT * FROM project_agent_deliveries")
                try execute("DROP TABLE project_agent_deliveries")
                try execute(
                    "ALTER TABLE project_agent_deliveries_v15 RENAME TO project_agent_deliveries"
                )
                try execute(
                    """
                    CREATE INDEX project_agent_deliveries_agent_queue
                    ON project_agent_deliveries(
                        owner_user_id, target_agent_id, status, created_at_unix_ms, id
                    )
                    """
                )
                try execute(
                    """
                    CREATE UNIQUE INDEX one_running_manager_delivery_per_agent
                    ON project_agent_deliveries(owner_user_id, target_agent_id)
                    WHERE status = 'running' AND trigger_kind != 'todo'
                    """
                )
                try execute(
                    """
                    CREATE UNIQUE INDEX one_running_executor_delivery_per_agent
                    ON project_agent_deliveries(owner_user_id, target_agent_id)
                    WHERE status = 'running' AND trigger_kind = 'todo'
                    """
                )
                try execute(
                    """
                    CREATE TABLE IF NOT EXISTS local_agent_todo_sources (
                        owner_user_id TEXT NOT NULL,
                        todo_id TEXT NOT NULL,
                        conversation_id TEXT NOT NULL,
                        message_id TEXT NOT NULL,
                        relation TEXT NOT NULL CHECK(relation IN (
                            'created', 'updated', 'reprioritized', 'blocked_context'
                        )),
                        created_at_unix_ms INTEGER NOT NULL,
                        PRIMARY KEY(
                            owner_user_id, todo_id, conversation_id, message_id, relation
                        ),
                        FOREIGN KEY(owner_user_id, todo_id)
                            REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
                        FOREIGN KEY(owner_user_id, conversation_id)
                            REFERENCES project_agent_rooms(owner_user_id, id),
                        FOREIGN KEY(owner_user_id, message_id)
                            REFERENCES project_agent_messages(owner_user_id, id)
                    )
                    """
                )
                try execute(
                    """
                    INSERT OR IGNORE INTO local_agent_todo_sources (
                        owner_user_id, todo_id, conversation_id, message_id, relation,
                        created_at_unix_ms
                    )
                    SELECT owner_user_id, id, source_room_id, source_message_id, 'created',
                           created_at_unix_ms
                    FROM local_agent_todos
                    WHERE source_room_id IS NOT NULL AND source_message_id IS NOT NULL
                    """
                )
                try execute(
                    """
                    CREATE TABLE IF NOT EXISTS local_agent_todo_events (
                        owner_user_id TEXT NOT NULL,
                        id TEXT NOT NULL,
                        todo_id TEXT NOT NULL,
                        sequence INTEGER NOT NULL CHECK(sequence > 0),
                        run_id TEXT,
                        kind TEXT NOT NULL CHECK(kind IN (
                            'started', 'progress', 'blocked', 'completed', 'cancelled'
                        )),
                        stage TEXT NOT NULL,
                        detail TEXT NOT NULL,
                        created_at_unix_ms INTEGER NOT NULL,
                        PRIMARY KEY(owner_user_id, id),
                        UNIQUE(owner_user_id, todo_id, sequence),
                        FOREIGN KEY(owner_user_id, todo_id)
                            REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE
                    )
                    """
                )
                try execute(
                    "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (15)"
                )
                try execute("COMMIT")
            } catch {
                try? execute("ROLLBACK")
                try? execute("PRAGMA foreign_keys = ON")
                throw error
            }
            try execute("PRAGMA foreign_keys = ON")
        }
        if !hasMigration(16) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_todo_dependencies (
                    owner_user_id TEXT NOT NULL,
                    todo_id TEXT NOT NULL,
                    prerequisite_todo_id TEXT NOT NULL,
                    created_at_unix_ms INTEGER NOT NULL,
                    PRIMARY KEY(owner_user_id, todo_id, prerequisite_todo_id),
                    CHECK(todo_id != prerequisite_todo_id),
                    FOREIGN KEY(owner_user_id, todo_id)
                        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
                    FOREIGN KEY(owner_user_id, prerequisite_todo_id)
                        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE
                )
                """
            )
            try execute(
                """
                CREATE INDEX IF NOT EXISTS local_agent_todo_dependencies_prerequisite
                ON local_agent_todo_dependencies(
                    owner_user_id, prerequisite_todo_id, todo_id
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (16)"
            )
        }
        if !hasMigration(17) {
            if !hasColumn("project_manager_agent_id") {
                // SQLite cannot add one column with a composite foreign key. Rebuild the room
                // table so the manager reference remains account-scoped instead of accidentally
                // accepting an Agent ID owned by another account.
                try execute("PRAGMA foreign_keys = OFF")
                do {
                    try execute("BEGIN IMMEDIATE")
                    try execute(
                        """
                        CREATE TABLE project_agent_rooms_v17 (
                            owner_user_id TEXT NOT NULL,
                            id TEXT NOT NULL,
                            project_id TEXT NOT NULL,
                            name TEXT NOT NULL,
                            goal TEXT NOT NULL,
                            default_agent_id TEXT,
                            status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
                            created_at_unix_ms INTEGER NOT NULL,
                            updated_at_unix_ms INTEGER NOT NULL,
                            conversation_kind TEXT NOT NULL DEFAULT 'project_team'
                                CHECK(conversation_kind IN (
                                    'project_team', 'human_agent_direct', 'agent_agent_direct'
                                )),
                            direct_key TEXT,
                            project_manager_agent_id TEXT,
                            PRIMARY KEY(owner_user_id, id),
                            FOREIGN KEY(owner_user_id, default_agent_id)
                                REFERENCES local_agent_profiles(owner_user_id, id),
                            FOREIGN KEY(owner_user_id, project_manager_agent_id)
                                REFERENCES local_agent_profiles(owner_user_id, id)
                        )
                        """
                    )
                    try execute(
                        """
                        INSERT INTO project_agent_rooms_v17 (
                            owner_user_id, id, project_id, name, goal, default_agent_id,
                            status, created_at_unix_ms, updated_at_unix_ms,
                            conversation_kind, direct_key, project_manager_agent_id
                        )
                        SELECT owner_user_id, id, project_id, name, goal, default_agent_id,
                               status, created_at_unix_ms, updated_at_unix_ms,
                               conversation_kind, direct_key, NULL
                        FROM project_agent_rooms
                        """
                    )
                    try execute("DROP TABLE project_agent_rooms")
                    try execute(
                        "ALTER TABLE project_agent_rooms_v17 RENAME TO project_agent_rooms"
                    )
                    try execute(
                        """
                        CREATE UNIQUE INDEX one_active_agent_room_per_project
                        ON project_agent_rooms(owner_user_id, project_id)
                        WHERE status = 'active'
                        """
                    )
                    try execute(
                        """
                        CREATE UNIQUE INDEX one_active_direct_conversation_per_pair
                        ON project_agent_rooms(owner_user_id, direct_key)
                        WHERE status = 'active' AND direct_key IS NOT NULL
                        """
                    )
                    try execute("COMMIT")
                } catch {
                    try? execute("ROLLBACK")
                    try? execute("PRAGMA foreign_keys = ON")
                    throw error
                }
                try execute("PRAGMA foreign_keys = ON")
            }
            // Existing teams are migrated only when there is exactly one active member whose
            // explicit profession is Project Manager. Join order and default routing are never
            // used to infer management authority.
            try execute(
                """
                UPDATE project_agent_rooms AS room
                SET project_manager_agent_id = (
                    SELECT member.agent_id
                    FROM project_agent_room_members member
                    JOIN local_agent_profiles profile
                      ON profile.owner_user_id = member.owner_user_id
                     AND profile.id = member.agent_id
                    WHERE member.owner_user_id = room.owner_user_id
                      AND member.room_id = room.id
                      AND member.status = 'active'
                      AND profile.status = 'active'
                      AND profile.profession_key = 'project_manager'
                    LIMIT 1
                )
                WHERE room.conversation_kind = 'project_team'
                  AND room.project_manager_agent_id IS NULL
                  AND 1 = (
                    SELECT COUNT(*)
                    FROM project_agent_room_members member
                    JOIN local_agent_profiles profile
                      ON profile.owner_user_id = member.owner_user_id
                     AND profile.id = member.agent_id
                    WHERE member.owner_user_id = room.owner_user_id
                      AND member.room_id = room.id
                      AND member.status = 'active'
                      AND profile.status = 'active'
                      AND profile.profession_key = 'project_manager'
                  )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (17)"
            )
        }
        if !hasMigration(18) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_membership_proposals (
                    owner_user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    source_room_id TEXT NOT NULL,
                    proposer_agent_id TEXT NOT NULL,
                    source_delivery_id TEXT NOT NULL,
                    request_key TEXT NOT NULL,
                    draft_json TEXT NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('pending', 'approved', 'rejected')),
                    created_at_unix_ms INTEGER NOT NULL,
                    resolved_at_unix_ms INTEGER,
                    PRIMARY KEY(owner_user_id, id),
                    UNIQUE(
                        owner_user_id, source_room_id, proposer_agent_id,
                        source_delivery_id, request_key
                    ),
                    FOREIGN KEY(owner_user_id, source_room_id, proposer_agent_id)
                        REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
                    FOREIGN KEY(owner_user_id, source_delivery_id)
                        REFERENCES project_agent_deliveries(owner_user_id, id)
                )
                """
            )
            try execute(
                """
                CREATE INDEX IF NOT EXISTS local_agent_membership_proposals_pending
                ON local_agent_membership_proposals(
                    owner_user_id, source_room_id, status, created_at_unix_ms, id
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (18)"
            )
        }
        if !hasMigration(19) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_team_assets (
                    owner_user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    team_room_id TEXT NOT NULL,
                    category TEXT NOT NULL CHECK(category IN (
                        'overview', 'current_progress', 'tech_stack', 'architecture',
                        'conventions', 'decision', 'reference'
                    )),
                    title TEXT NOT NULL,
                    markdown TEXT NOT NULL,
                    revision INTEGER NOT NULL CHECK(revision > 0),
                    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
                    created_by_agent_id TEXT,
                    updated_by_agent_id TEXT,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL,
                    PRIMARY KEY(owner_user_id, id),
                    FOREIGN KEY(owner_user_id, team_room_id)
                        REFERENCES project_agent_rooms(owner_user_id, id),
                    FOREIGN KEY(owner_user_id, created_by_agent_id)
                        REFERENCES local_agent_profiles(owner_user_id, id),
                    FOREIGN KEY(owner_user_id, updated_by_agent_id)
                        REFERENCES local_agent_profiles(owner_user_id, id)
                );
                CREATE INDEX IF NOT EXISTS local_agent_team_assets_team
                    ON local_agent_team_assets(
                        owner_user_id, team_room_id, status, category, updated_at_unix_ms
                    );
                CREATE TABLE IF NOT EXISTS local_agent_team_asset_revisions (
                    owner_user_id TEXT NOT NULL,
                    asset_id TEXT NOT NULL,
                    revision INTEGER NOT NULL CHECK(revision > 0),
                    title TEXT NOT NULL,
                    markdown TEXT NOT NULL,
                    editor_agent_id TEXT,
                    created_at_unix_ms INTEGER NOT NULL,
                    PRIMARY KEY(owner_user_id, asset_id, revision),
                    FOREIGN KEY(owner_user_id, asset_id)
                        REFERENCES local_agent_team_assets(owner_user_id, id) ON DELETE CASCADE,
                    FOREIGN KEY(owner_user_id, editor_agent_id)
                        REFERENCES local_agent_profiles(owner_user_id, id)
                );
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (19)"
            )
        }
        if !hasMigration(20) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_todo_asset_snapshots (
                    owner_user_id TEXT NOT NULL,
                    todo_id TEXT NOT NULL,
                    asset_id TEXT NOT NULL,
                    team_room_id TEXT NOT NULL,
                    category TEXT NOT NULL,
                    title TEXT NOT NULL,
                    markdown TEXT NOT NULL,
                    revision INTEGER NOT NULL CHECK(revision > 0),
                    captured_at_unix_ms INTEGER NOT NULL,
                    PRIMARY KEY(owner_user_id, todo_id, asset_id),
                    FOREIGN KEY(owner_user_id, todo_id)
                        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
                    FOREIGN KEY(owner_user_id, asset_id)
                        REFERENCES local_agent_team_assets(owner_user_id, id)
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (20)"
            )
        }
        // Never trust only the migration marker for columns introduced through ALTER TABLE.
        // Older builds inserted marker 21 from the bootstrap SQL even when CREATE TABLE IF NOT
        // EXISTS kept an older local_agent_todos table unchanged.
        if !hasColumn("execution_contract_json", table: "local_agent_todos") {
            try execute(
                "ALTER TABLE local_agent_todos ADD COLUMN execution_contract_json TEXT NOT NULL DEFAULT '{}'"
            )
        }
        if !hasMigration(21) {
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (21)"
            )
        }
        if !hasMigration(22) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_todo_event_recipients (
                    owner_user_id TEXT NOT NULL,
                    event_key TEXT NOT NULL,
                    todo_id TEXT NOT NULL,
                    event_kind TEXT NOT NULL CHECK(event_kind IN (
                        'ready', 'blocked', 'completed', 'cancelled'
                    )),
                    recipient_agent_id TEXT NOT NULL,
                    delivery_id TEXT NOT NULL,
                    message_id TEXT NOT NULL,
                    created_at_unix_ms INTEGER NOT NULL,
                    PRIMARY KEY(owner_user_id, event_key, recipient_agent_id),
                    UNIQUE(owner_user_id, delivery_id),
                    FOREIGN KEY(owner_user_id, todo_id)
                        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
                    FOREIGN KEY(owner_user_id, recipient_agent_id)
                        REFERENCES local_agent_profiles(owner_user_id, id),
                    FOREIGN KEY(owner_user_id, delivery_id)
                        REFERENCES project_agent_deliveries(owner_user_id, id),
                    FOREIGN KEY(owner_user_id, message_id)
                        REFERENCES project_agent_messages(owner_user_id, id)
                );
                CREATE INDEX IF NOT EXISTS local_agent_todo_event_recipients_todo
                    ON local_agent_todo_event_recipients(
                        owner_user_id, todo_id, created_at_unix_ms, recipient_agent_id
                    );
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (22)"
            )
        }
        if !hasColumn("sha256", table: "project_agent_message_attachments") {
            try execute("ALTER TABLE project_agent_message_attachments ADD COLUMN sha256 TEXT")
        }
        if !hasColumn("sync_status", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_status IN ('local_only', 'queued', 'uploading', 'synced', 'failed'))"
            )
        }
        for column in [
            "artifact_id", "storage_provider", "bucket", "object_key", "remote_view_path",
            "upload_error",
        ] where !hasColumn(column, table: "project_agent_message_attachments") {
            try execute("ALTER TABLE project_agent_message_attachments ADD COLUMN \(column) TEXT")
        }
        if !hasColumn("synced_at_unix_ms", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN synced_at_unix_ms INTEGER"
            )
        }
        if !hasColumn("upload_attempt", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN upload_attempt INTEGER NOT NULL DEFAULT 0 CHECK(upload_attempt >= 0)"
            )
        }
        if !hasColumn("next_retry_at_unix_ms", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN next_retry_at_unix_ms INTEGER NOT NULL DEFAULT 0 CHECK(next_retry_at_unix_ms >= 0)"
            )
        }
        try execute(
            """
            CREATE INDEX IF NOT EXISTS project_agent_attachment_sync_outbox
            ON project_agent_message_attachments(
                owner_user_id, sync_status, next_retry_at_unix_ms, id
            )
            """
        )
        if !hasMigration(23) {
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (23)"
            )
        }
        if !hasMigration(24) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_communication_metrics (
                    owner_user_id TEXT NOT NULL,
                    metric_name TEXT NOT NULL,
                    dimension TEXT NOT NULL,
                    event_count INTEGER NOT NULL CHECK(event_count > 0),
                    total_value INTEGER NOT NULL CHECK(total_value >= 0),
                    maximum_value INTEGER NOT NULL CHECK(maximum_value >= 0),
                    updated_at_unix_ms INTEGER NOT NULL CHECK(updated_at_unix_ms >= 0),
                    PRIMARY KEY(owner_user_id, metric_name, dimension)
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (24)"
            )
        }
        if !hasMigration(25) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_message_sequence_events (
                    owner_user_id TEXT NOT NULL,
                    run_fingerprint TEXT NOT NULL,
                    sequence INTEGER NOT NULL CHECK(sequence > 0),
                    character_count INTEGER NOT NULL CHECK(character_count >= 0),
                    document_count INTEGER NOT NULL CHECK(document_count >= 0),
                    created_at_unix_ms INTEGER NOT NULL CHECK(created_at_unix_ms >= 0),
                    PRIMARY KEY(owner_user_id, run_fingerprint, sequence)
                )
                """
            )
            try execute(
                """
                CREATE INDEX IF NOT EXISTS local_agent_message_sequence_events_window
                ON local_agent_message_sequence_events(
                    owner_user_id, run_fingerprint, created_at_unix_ms, sequence
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (25)"
            )
        }
    }
}
