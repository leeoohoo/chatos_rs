
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS project_agent_rooms (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    goal TEXT NOT NULL DEFAULT '',
    default_agent_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    conversation_kind TEXT NOT NULL
        CHECK(conversation_kind IN ('project_team', 'human_agent_direct', 'agent_agent_direct')),
    direct_key TEXT,
    project_manager_agent_id TEXT,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, direct_key),
    FOREIGN KEY(owner_user_id, default_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE TABLE IF NOT EXISTS project_agent_room_members (
    owner_user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    role TEXT NOT NULL,
    responsibility TEXT NOT NULL DEFAULT '',
    plugin_allowlist_json TEXT NOT NULL DEFAULT '[]',
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
    sha256 TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'local_only'
        CHECK(sync_status IN ('local_only', 'queued', 'uploading', 'synced', 'failed')),
    source_document_ref TEXT,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, message_id, position),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE
);
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
CREATE TABLE IF NOT EXISTS windows_agent_chat_message_references (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    message_ref TEXT NOT NULL,
    room_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, run_id, message_ref),
    UNIQUE(owner_user_id, run_id, message_id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS windows_agent_chat_attachment_references (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    attachment_ref TEXT NOT NULL,
    room_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    attachment_id TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, run_id, attachment_ref),
    UNIQUE(owner_user_id, run_id, message_id, attachment_id),
    FOREIGN KEY(owner_user_id, message_id)
        REFERENCES project_agent_messages(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, attachment_id)
        REFERENCES project_agent_message_attachments(owner_user_id, id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS windows_agent_document_drafts (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    request_key TEXT NOT NULL,
    document_ref TEXT NOT NULL,
    name TEXT NOT NULL,
    title TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes > 0),
    sha256 TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('available', 'consumed')),
    created_at_unix_ms INTEGER NOT NULL,
    consumed_at_unix_ms INTEGER,
    PRIMARY KEY(owner_user_id, run_id, document_ref),
    UNIQUE(owner_user_id, run_id, request_key)
);
CREATE TABLE IF NOT EXISTS windows_agent_chat_read_receipts (
    owner_user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    request_key TEXT NOT NULL,
    requested_limit INTEGER NOT NULL CHECK(requested_limit BETWEEN 1 AND 500),
    output_json TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, run_id, request_key)
);
CREATE TABLE IF NOT EXISTS project_agent_deliveries (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    root_message_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    trigger_kind TEXT NOT NULL
        CHECK(trigger_kind IN ('mention', 'default_agent', 'agent_mention', 'heartbeat', 'todo', 'todo_status')),
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
CREATE TABLE IF NOT EXISTS local_agent_team_asset_reviews (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    team_room_id TEXT NOT NULL,
    todo_id TEXT NOT NULL,
    todo_event_id TEXT NOT NULL,
    suggestion_index INTEGER NOT NULL CHECK(suggestion_index >= 0),
    source_agent_id TEXT NOT NULL,
    category TEXT NOT NULL CHECK(category IN (
        'overview', 'current_progress', 'tech_stack', 'architecture',
        'conventions', 'decision', 'reference'
    )),
    title TEXT NOT NULL,
    markdown TEXT NOT NULL,
    rationale TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'applied', 'rejected')),
    resolved_asset_id TEXT,
    resolved_asset_revision INTEGER CHECK(resolved_asset_revision > 0),
    reviewed_by_agent_id TEXT,
    review_reason TEXT NOT NULL DEFAULT '',
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, todo_event_id, suggestion_index),
    CHECK(
        (status = 'pending' AND resolved_asset_id IS NULL
            AND resolved_asset_revision IS NULL
            AND reviewed_by_agent_id IS NULL AND review_reason = '')
        OR (status = 'applied' AND resolved_asset_id IS NOT NULL
            AND resolved_asset_revision IS NOT NULL
            AND reviewed_by_agent_id IS NOT NULL)
        OR (status = 'rejected' AND resolved_asset_id IS NULL
            AND resolved_asset_revision IS NULL
            AND reviewed_by_agent_id IS NOT NULL AND length(review_reason) > 0)
    ),
    FOREIGN KEY(owner_user_id, team_room_id)
        REFERENCES project_agent_rooms(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, todo_event_id)
        REFERENCES local_agent_todo_events(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, source_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id),
    FOREIGN KEY(owner_user_id, resolved_asset_id)
        REFERENCES local_agent_team_assets(owner_user_id, id),
    FOREIGN KEY(owner_user_id, reviewed_by_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
);
CREATE INDEX IF NOT EXISTS local_agent_team_asset_reviews_queue
    ON local_agent_team_asset_reviews(
        owner_user_id, team_room_id, status, created_at_unix_ms, id
    );
CREATE INDEX IF NOT EXISTS local_agent_team_asset_reviews_todo
    ON local_agent_team_asset_reviews(owner_user_id, todo_id);
CREATE TABLE IF NOT EXISTS local_agent_project_dashboards (
    owner_user_id TEXT NOT NULL,
    team_room_id TEXT NOT NULL,
    dashboard_json TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    updated_by_agent_id TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL CHECK(updated_at_unix_ms >= 0),
    PRIMARY KEY(owner_user_id, team_room_id),
    FOREIGN KEY(owner_user_id, team_room_id)
        REFERENCES project_agent_rooms(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, updated_by_agent_id)
        REFERENCES local_agent_profiles(owner_user_id, id)
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
CREATE INDEX IF NOT EXISTS local_agent_todos_agent_queue
    ON local_agent_todos(owner_user_id, agent_id, status, priority DESC, sort_order);
CREATE INDEX IF NOT EXISTS local_agent_todos_team_queue
    ON local_agent_todos(owner_user_id, team_room_id, status, priority DESC, sort_order);
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
    asset_update_suggestions_json TEXT NOT NULL DEFAULT '[]',
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id),
    UNIQUE(owner_user_id, todo_id, sequence),
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE
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
CREATE TABLE IF NOT EXISTS windows_agent_delivery_runs (
    owner_user_id TEXT NOT NULL,
    delivery_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('ready', 'running', 'succeeded', 'failed', 'cancelled')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, delivery_id),
    UNIQUE(owner_user_id, turn_id),
    UNIQUE(owner_user_id, run_id),
    FOREIGN KEY(owner_user_id, delivery_id)
        REFERENCES project_agent_deliveries(owner_user_id, id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS windows_agent_delivery_runs_recent
    ON windows_agent_delivery_runs(owner_user_id, updated_at_unix_ms DESC, run_id DESC);
CREATE TABLE IF NOT EXISTS windows_agent_run_control_journal (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('cancel', 'retry')),
    team_room_id TEXT NOT NULL,
    todo_id TEXT NOT NULL,
    delivery_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'completed')),
    host_status TEXT,
    created_at_unix_ms INTEGER NOT NULL,
    completed_at_unix_ms INTEGER,
    PRIMARY KEY(owner_user_id, id),
    FOREIGN KEY(owner_user_id, todo_id)
        REFERENCES local_agent_todos(owner_user_id, id) ON DELETE CASCADE,
    FOREIGN KEY(owner_user_id, delivery_id)
        REFERENCES project_agent_deliveries(owner_user_id, id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS one_pending_windows_agent_run_control
    ON windows_agent_run_control_journal(owner_user_id, todo_id, action)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS windows_agent_run_control_pending
    ON windows_agent_run_control_journal(owner_user_id, status, created_at_unix_ms, id);
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
    UNIQUE(owner_user_id, source_room_id, proposer_agent_id, source_delivery_id, request_key),
    FOREIGN KEY(owner_user_id, source_room_id, proposer_agent_id)
        REFERENCES project_agent_room_members(owner_user_id, room_id, agent_id),
    FOREIGN KEY(owner_user_id, source_delivery_id)
        REFERENCES project_agent_deliveries(owner_user_id, id)
);
CREATE INDEX IF NOT EXISTS local_agent_membership_proposals_pending
    ON local_agent_membership_proposals(
        owner_user_id, source_room_id, status, created_at_unix_ms, id
    );
