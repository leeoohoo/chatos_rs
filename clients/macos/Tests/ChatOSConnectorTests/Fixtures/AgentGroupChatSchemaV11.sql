-- Historical fixture captured from commit 1ca6f3467 before migration 12 existed.
-- IDs and content are synthetic and intentionally contain no user data.
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
BEGIN IMMEDIATE;

CREATE TABLE local_agent_profiles (
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
    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id)
);

CREATE TABLE project_agent_rooms (
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
    PRIMARY KEY(owner_user_id, id),
    FOREIGN KEY(owner_user_id, default_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE UNIQUE INDEX one_active_agent_room_per_project
    ON project_agent_rooms(owner_user_id, project_id) WHERE status = 'active';

CREATE TABLE project_agent_room_members (
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

CREATE TABLE project_agent_messages (
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
CREATE INDEX project_agent_messages_room_order
    ON project_agent_messages(owner_user_id, room_id, created_at_unix_ms, id);

CREATE TABLE project_agent_message_mentions (
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

CREATE TABLE project_agent_message_attachments (
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
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, message_id, position),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE
);

CREATE TABLE project_agent_read_cursors (
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

CREATE TABLE project_agent_deliveries (
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
CREATE INDEX project_agent_deliveries_agent_queue
    ON project_agent_deliveries(owner_user_id, target_agent_id, status, created_at_unix_ms, id);
CREATE UNIQUE INDEX one_running_delivery_per_agent
    ON project_agent_deliveries(owner_user_id, target_agent_id) WHERE status = 'running';

CREATE TABLE local_agent_creation_proposals (
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
CREATE INDEX local_agent_creation_proposals_pending
    ON local_agent_creation_proposals(owner_user_id, room_id, status, created_at_unix_ms, id);

CREATE TABLE local_agent_removal_proposals (
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
CREATE INDEX local_agent_removal_proposals_pending
    ON local_agent_removal_proposals(owner_user_id, room_id, status, created_at_unix_ms, id);

CREATE TABLE local_agent_team_creation_proposals (
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
CREATE INDEX local_agent_team_creation_proposals_pending
    ON local_agent_team_creation_proposals(owner_user_id, source_room_id, status, created_at_unix_ms, id);

CREATE TABLE local_project_creation_proposals (
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
CREATE INDEX local_project_creation_proposals_pending
    ON local_project_creation_proposals(owner_user_id, room_id, status, created_at_unix_ms, id);

CREATE TABLE local_agent_group_chat_runs (
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
CREATE INDEX local_agent_group_chat_runs_status
    ON local_agent_group_chat_runs(owner_user_id, status, updated_at_unix_ms);

CREATE TABLE local_agent_group_chat_schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL
);
INSERT INTO local_agent_group_chat_schema_migrations(version)
VALUES (1), (2), (3), (4), (5), (6), (7), (8), (9), (10), (11);

INSERT INTO local_agent_profiles VALUES (
    'fixture-owner', 'fixture-agent', 'Fixture Agent', '', 'Preserve this role.',
    'fixture-model', 'medium', 'general_member', '[]', '[]', 'active', 100, 100
);
INSERT INTO project_agent_rooms VALUES (
    'fixture-owner', 'fixture-room', 'fixture-project', 'Fixture Room', 'Preserve this goal.',
    'fixture-agent', 'active', 100, 100, 'project_team', NULL
);
INSERT INTO project_agent_room_members VALUES (
    'fixture-owner', 'fixture-room', 'fixture-agent', 'member', '', '[]', 'active', 100
);
INSERT INTO project_agent_messages VALUES (
    'fixture-owner', 'fixture-message', 'fixture-room', 'human', 'fixture-owner',
    'Preserve this message.', NULL, NULL, NULL, 'fixture-message', 0, 100
);
INSERT INTO project_agent_message_attachments VALUES (
    'fixture-owner', 'fixture-message', 'fixture-attachment', 0, 'fixture.md',
    'text/markdown', 8, 'file', 'file', 'fixture-message/fixture-attachment'
);
INSERT INTO project_agent_deliveries VALUES (
    'fixture-owner', 'fixture-delivery', 'fixture-room', 'fixture-message',
    'fixture-message', 'fixture-agent', 'mention', 'pending', 0, 0,
    'fixture-deduplication', NULL, NULL, NULL, NULL, 100
);
INSERT INTO local_agent_group_chat_runs VALUES (
    'fixture-owner', 'fixture-run', 'fixture-delivery', 'fixture-room', 'fixture-project',
    'fixture-agent', 'ready', '{}', 100, 100
);

COMMIT;
