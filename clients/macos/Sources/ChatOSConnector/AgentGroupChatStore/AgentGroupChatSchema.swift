enum AgentGroupChatSchema {
    static let definition = """
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
            thinking_level TEXT,
            profession_key TEXT NOT NULL DEFAULT 'general_member',
            default_plugin_ids_json TEXT NOT NULL,
            default_skill_ids_json TEXT NOT NULL,
            heartbeat_enabled INTEGER NOT NULL DEFAULT 0 CHECK(heartbeat_enabled IN (0, 1)),
            heartbeat_interval_seconds INTEGER NOT NULL DEFAULT 900
                CHECK(heartbeat_interval_seconds BETWEEN 60 AND 86400),
            heartbeat_prompt TEXT NOT NULL DEFAULT '',
            last_heartbeat_at_unix_ms INTEGER,
            next_heartbeat_at_unix_ms INTEGER,
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
            conversation_kind TEXT NOT NULL DEFAULT 'project_team'
                CHECK(conversation_kind IN ('project_team', 'human_agent_direct', 'agent_agent_direct')),
            direct_key TEXT,
            project_manager_agent_id TEXT,
            PRIMARY KEY(owner_user_id, id),
            FOREIGN KEY(owner_user_id, default_agent_id)
                REFERENCES local_agent_profiles(owner_user_id, id),
            FOREIGN KEY(owner_user_id, project_manager_agent_id)
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

        CREATE TABLE IF NOT EXISTS project_agent_message_attachments (
            owner_user_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            id TEXT NOT NULL,
            position INTEGER NOT NULL CHECK(position >= 0),
            name TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            size_bytes INTEGER NOT NULL CHECK(size_bytes > 0),
            kind TEXT NOT NULL CHECK(kind IN ('image', 'file', 'audio')),
            origin TEXT NOT NULL CHECK(origin IN ('file', 'pastedImage', 'pastedDocument', 'pastedText')),
            relative_path TEXT NOT NULL,
            sha256 TEXT,
            sync_status TEXT NOT NULL DEFAULT 'local_only'
                CHECK(sync_status IN ('local_only', 'queued', 'uploading', 'synced', 'failed')),
            artifact_id TEXT,
            storage_provider TEXT,
            bucket TEXT,
            object_key TEXT,
            remote_view_path TEXT,
            upload_error TEXT,
            synced_at_unix_ms INTEGER,
            upload_attempt INTEGER NOT NULL DEFAULT 0 CHECK(upload_attempt >= 0),
            next_retry_at_unix_ms INTEGER NOT NULL DEFAULT 0 CHECK(next_retry_at_unix_ms >= 0),
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, message_id, position),
            FOREIGN KEY(owner_user_id, message_id)
                REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS project_agent_read_cursors (
            owner_user_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            message_created_at_unix_ms INTEGER NOT NULL,
            updated_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, room_id, agent_id)
                REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, message_id)
                REFERENCES project_agent_messages(owner_user_id, id)
        );

        CREATE TABLE IF NOT EXISTS local_agent_todos (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            team_room_id TEXT NOT NULL,
            source_room_id TEXT,
            source_message_id TEXT,
            request_key TEXT NOT NULL,
            title TEXT NOT NULL,
            detail TEXT NOT NULL,
            priority INTEGER NOT NULL CHECK(priority BETWEEN 0 AND 100),
            sort_order INTEGER NOT NULL CHECK(sort_order >= 0),
            status TEXT NOT NULL CHECK(status IN (
                'pending', 'in_progress', 'blocked', 'completed', 'cancelled'
            )),
            blocked_reason TEXT NOT NULL,
            result TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL,
            updated_at_unix_ms INTEGER NOT NULL,
            execution_plan_json TEXT NOT NULL,
            execution_contract_json TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, agent_id, request_key),
            FOREIGN KEY(owner_user_id, agent_id)
                REFERENCES local_agent_profiles(owner_user_id, id),
            FOREIGN KEY(owner_user_id, team_room_id)
                REFERENCES project_agent_rooms(owner_user_id, id),
            FOREIGN KEY(owner_user_id, source_room_id)
                REFERENCES project_agent_rooms(owner_user_id, id),
            FOREIGN KEY(owner_user_id, source_message_id)
                REFERENCES project_agent_messages(owner_user_id, id)
        );
        CREATE INDEX IF NOT EXISTS local_agent_todos_queue
            ON local_agent_todos(owner_user_id, agent_id, status, priority DESC, sort_order);

        CREATE TABLE IF NOT EXISTS local_agent_todo_sources (
            owner_user_id TEXT NOT NULL,
            todo_id TEXT NOT NULL,
            conversation_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            relation TEXT NOT NULL CHECK(relation IN (
                'created', 'updated', 'reprioritized', 'blocked_context'
            )),
            created_at_unix_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_user_id, todo_id, conversation_id, message_id, relation),
            FOREIGN KEY(owner_user_id, todo_id)
                REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
            FOREIGN KEY(owner_user_id, conversation_id)
                REFERENCES project_agent_rooms(owner_user_id, id),
            FOREIGN KEY(owner_user_id, message_id)
                REFERENCES project_agent_messages(owner_user_id, id)
        );

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
        );

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
        );
        CREATE INDEX IF NOT EXISTS local_agent_todo_dependencies_prerequisite
            ON local_agent_todo_dependencies(owner_user_id, prerequisite_todo_id, todo_id);

        CREATE TABLE IF NOT EXISTS project_agent_deliveries (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            root_message_id TEXT NOT NULL,
            target_agent_id TEXT NOT NULL,
            trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('mention', 'default_agent', 'agent_mention', 'heartbeat', 'todo', 'todo_status')),
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
        CREATE UNIQUE INDEX IF NOT EXISTS one_running_manager_delivery_per_agent
            ON project_agent_deliveries(owner_user_id, target_agent_id)
            WHERE status = 'running' AND trigger_kind != 'todo';
        CREATE UNIQUE INDEX IF NOT EXISTS one_running_executor_delivery_per_agent
            ON project_agent_deliveries(owner_user_id, target_agent_id)
            WHERE status = 'running' AND trigger_kind = 'todo';

        CREATE TABLE IF NOT EXISTS local_agent_creation_proposals (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            proposer_agent_id TEXT NOT NULL,
            source_delivery_id TEXT NOT NULL,
            request_key TEXT NOT NULL,
            draft_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending', 'approved', 'rejected')),
            created_agent_id TEXT,
            created_at_unix_ms INTEGER NOT NULL,
            resolved_at_unix_ms INTEGER,
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, room_id, proposer_agent_id, source_delivery_id, request_key),
            FOREIGN KEY(owner_user_id, room_id, proposer_agent_id)
                REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, source_delivery_id)
                REFERENCES project_agent_deliveries(owner_user_id, id),
            FOREIGN KEY(owner_user_id, created_agent_id)
                REFERENCES local_agent_profiles(owner_user_id, id)
        );
        CREATE INDEX IF NOT EXISTS local_agent_creation_proposals_pending
            ON local_agent_creation_proposals(owner_user_id, room_id, status, created_at_unix_ms, id);

        CREATE TABLE IF NOT EXISTS local_agent_removal_proposals (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            proposer_agent_id TEXT NOT NULL,
            source_delivery_id TEXT NOT NULL,
            request_key TEXT NOT NULL,
            draft_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending', 'approved', 'rejected')),
            created_at_unix_ms INTEGER NOT NULL,
            resolved_at_unix_ms INTEGER,
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, room_id, proposer_agent_id, source_delivery_id, request_key),
            FOREIGN KEY(owner_user_id, room_id, proposer_agent_id)
                REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, source_delivery_id)
                REFERENCES project_agent_deliveries(owner_user_id, id)
        );
        CREATE INDEX IF NOT EXISTS local_agent_removal_proposals_pending
            ON local_agent_removal_proposals(owner_user_id, room_id, status, created_at_unix_ms, id);

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
        );
        CREATE INDEX IF NOT EXISTS local_agent_membership_proposals_pending
            ON local_agent_membership_proposals(
                owner_user_id, source_room_id, status, created_at_unix_ms, id
            );

        CREATE TABLE IF NOT EXISTS local_agent_team_creation_proposals (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            source_room_id TEXT NOT NULL,
            proposer_agent_id TEXT NOT NULL,
            source_delivery_id TEXT NOT NULL,
            request_key TEXT NOT NULL,
            draft_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending', 'approved', 'rejected')),
            created_room_id TEXT,
            created_at_unix_ms INTEGER NOT NULL,
            resolved_at_unix_ms INTEGER,
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, source_room_id, proposer_agent_id, source_delivery_id, request_key),
            FOREIGN KEY(owner_user_id, source_room_id, proposer_agent_id)
                REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, source_delivery_id)
                REFERENCES project_agent_deliveries(owner_user_id, id),
            FOREIGN KEY(owner_user_id, created_room_id)
                REFERENCES project_agent_rooms(owner_user_id, id)
        );
        CREATE INDEX IF NOT EXISTS local_agent_team_creation_proposals_pending
            ON local_agent_team_creation_proposals(owner_user_id, source_room_id, status, created_at_unix_ms, id);

        CREATE TABLE IF NOT EXISTS local_project_creation_proposals (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            proposer_agent_id TEXT NOT NULL,
            source_delivery_id TEXT NOT NULL,
            request_key TEXT NOT NULL,
            draft_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending', 'approved', 'rejected')),
            created_project_id TEXT,
            created_at_unix_ms INTEGER NOT NULL,
            resolved_at_unix_ms INTEGER,
            PRIMARY KEY(owner_user_id, id),
            UNIQUE(owner_user_id, room_id, proposer_agent_id, source_delivery_id, request_key),
            FOREIGN KEY(owner_user_id, room_id, proposer_agent_id)
                REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
            FOREIGN KEY(owner_user_id, source_delivery_id)
                REFERENCES project_agent_deliveries(owner_user_id, id)
        );
        CREATE INDEX IF NOT EXISTS local_project_creation_proposals_pending
            ON local_project_creation_proposals(owner_user_id, room_id, status, created_at_unix_ms, id);

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
            ON local_agent_team_assets(owner_user_id, team_room_id, status, category, updated_at_unix_ms);

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
        );

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
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (3);
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (4);
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (5);
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (6);
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (7);
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (10);
        INSERT OR IGNORE INTO local_agent_group_chat_schema_migrations(version) VALUES (11);
        COMMIT;
        """
}
