extension AgentGroupChatMigrations {
    static let todoEventRecipientsDefinition = """
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

    static let teamAssetsDefinition = """
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

    static func deliveryTableDefinition(name: String, triggerKinds: String) -> String {
        """
        CREATE TABLE \(name) (
            owner_user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            root_message_id TEXT NOT NULL,
            target_agent_id TEXT NOT NULL,
            trigger_kind TEXT NOT NULL CHECK(trigger_kind IN (\(triggerKinds))),
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
    }
}
