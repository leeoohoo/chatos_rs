-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    default_model_config_id TEXT NULL,
    project_id TEXT NULL,
    task_profile TEXT NOT NULL,
    creator_user_id TEXT NULL,
    owner_user_id TEXT NULL,
    parent_task_id TEXT NULL,
    source_run_id TEXT NULL,
    source_session_id TEXT NULL,
    source_turn_id TEXT NULL,
    source_user_message_id TEXT NULL,
    schedule_mode TEXT NOT NULL,
    schedule_due_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    deleted_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL
);
CREATE INDEX tasks_status_updated_idx ON tasks(status, updated_at DESC);
CREATE INDEX tasks_owner_project_idx ON tasks(owner_user_id, project_id, updated_at DESC);
CREATE INDEX tasks_schedule_due_idx ON tasks(schedule_mode, schedule_due_at, id)
    WHERE schedule_mode <> 'manual' AND schedule_due_at IS NOT NULL;
CREATE INDEX tasks_parent_idx ON tasks(parent_task_id);
CREATE INDEX tasks_source_run_idx ON tasks(source_run_id);
CREATE INDEX tasks_source_session_message_idx
    ON tasks(source_session_id, source_user_message_id, task_profile);
CREATE INDEX tasks_source_turn_idx ON tasks(source_turn_id);
CREATE INDEX tasks_tags_gin_idx ON tasks USING GIN(tags);
CREATE INDEX tasks_search_gin_idx ON tasks USING GIN(data jsonb_path_ops);

CREATE TABLE runtime_settings (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);

CREATE TABLE task_runs (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    execution_lane_key TEXT NULL,
    model_config_id TEXT NOT NULL,
    status TEXT NOT NULL,
    model_phase_status TEXT NOT NULL,
    cancel_requested BOOLEAN NOT NULL DEFAULT false,
    cancel_event_pending BOOLEAN NOT NULL DEFAULT false,
    dispatch_paused BOOLEAN NOT NULL DEFAULT false,
    dispatch_event_pending BOOLEAN NOT NULL DEFAULT false,
    post_process_event_pending BOOLEAN NOT NULL DEFAULT false,
    post_process_event_enqueued BOOLEAN NOT NULL DEFAULT false,
    post_process_completed BOOLEAN NOT NULL DEFAULT false,
    post_process_dead_lettered BOOLEAN NOT NULL DEFAULT false,
    chatos_followup_processed BOOLEAN NOT NULL DEFAULT false,
    worker_id TEXT NULL,
    claim_token TEXT NULL,
    claim_until TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL
);
CREATE UNIQUE INDEX task_runs_one_active_per_task_idx
    ON task_runs(task_id) WHERE status IN ('queued', 'running');
CREATE UNIQUE INDEX task_runs_one_active_per_execution_lane_idx
    ON task_runs(execution_lane_key)
    WHERE execution_lane_key IS NOT NULL
      AND status IN ('queued', 'running')
      AND (status = 'running' OR dispatch_paused = false);
CREATE INDEX task_runs_task_created_idx ON task_runs(task_id, created_at DESC, id);
CREATE INDEX task_runs_status_claim_idx ON task_runs(status, claim_until, created_at, id);
CREATE INDEX task_runs_worker_claim_idx ON task_runs(worker_id, claim_token);
CREATE INDEX task_runs_model_created_idx ON task_runs(model_config_id, created_at DESC);
CREATE INDEX task_runs_cancel_outbox_idx
    ON task_runs(updated_at, id)
    WHERE status = 'running' AND cancel_requested AND cancel_event_pending;
CREATE INDEX task_runs_dispatch_outbox_idx
    ON task_runs(created_at, id)
    WHERE status = 'queued' AND dispatch_event_pending AND NOT dispatch_paused;
CREATE INDEX task_runs_post_process_outbox_idx
    ON task_runs(updated_at, id)
    WHERE post_process_event_pending AND NOT post_process_dead_lettered;
CREATE INDEX task_runs_callback_state_idx ON task_runs USING GIN(data jsonb_path_ops);

CREATE TABLE task_run_events (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(run_id, id)
);
CREATE INDEX task_run_events_run_cursor_idx ON task_run_events(run_id, created_at, id);
CREATE INDEX task_run_events_run_type_idx ON task_run_events(run_id, event_type, created_at, id);

CREATE TABLE task_run_terminal_subscriptions (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
    parent_run_id TEXT NOT NULL,
    worker_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(run_id, parent_run_id, worker_id)
);
CREATE INDEX task_run_terminal_subscriptions_run_idx
    ON task_run_terminal_subscriptions(run_id, created_at, id);

CREATE TABLE ask_user_prompts (
    id TEXT PRIMARY KEY,
    task_id TEXT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    run_id TEXT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    resolution_event_pending BOOLEAN NOT NULL DEFAULT false,
    conversation_id TEXT NOT NULL,
    conversation_turn_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL
);
CREATE INDEX ask_user_prompts_task_status_idx ON ask_user_prompts(task_id, status, updated_at DESC);
CREATE INDEX ask_user_prompts_run_status_idx ON ask_user_prompts(run_id, status, updated_at DESC);
CREATE INDEX ask_user_prompts_resolution_outbox_idx
    ON ask_user_prompts(updated_at, id) WHERE resolution_event_pending;
CREATE INDEX ask_user_prompts_expiry_idx ON ask_user_prompts(expires_at)
    WHERE expires_at IS NOT NULL;

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    username_normalized TEXT NOT NULL UNIQUE,
    enabled BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX task_runner_users_updated_idx ON users(updated_at DESC, id);

CREATE TABLE task_prerequisites (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    prerequisite_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    PRIMARY KEY(task_id, prerequisite_task_id),
    CHECK(task_id <> prerequisite_task_id)
);
CREATE INDEX task_prerequisites_reverse_idx
    ON task_prerequisites(prerequisite_task_id, task_id);

CREATE TABLE task_dependency_graph_revisions (
    scope TEXT PRIMARY KEY,
    revision BIGINT NOT NULL CHECK(revision >= 0),
    updated_at TIMESTAMPTZ NOT NULL
);
INSERT INTO task_dependency_graph_revisions(scope, revision, updated_at)
VALUES ('global', 0, now());

CREATE TABLE cloud_agent_lanes (
    ordering_lane_key TEXT PRIMARY KEY,
    next_lane_seq BIGINT NOT NULL CHECK(next_lane_seq >= 0),
    active_lane_seq BIGINT NOT NULL CHECK(active_lane_seq > 0),
    version BIGINT NOT NULL CHECK(version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE cloud_agent_runs (
    agent_run_id TEXT PRIMARY KEY,
    ordering_lane_key TEXT NOT NULL REFERENCES cloud_agent_lanes(ordering_lane_key),
    lane_seq BIGINT NOT NULL CHECK(lane_seq > 0),
    generation BIGINT NOT NULL CHECK(generation > 0),
    step_seq BIGINT NOT NULL CHECK(step_seq > 0),
    status TEXT NOT NULL,
    phase TEXT NOT NULL,
    version BIGINT NOT NULL CHECK(version > 0),
    claim_token TEXT NULL,
    claim_until TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(ordering_lane_key, lane_seq)
);
CREATE INDEX cloud_agent_runs_claim_idx ON cloud_agent_runs(status, claim_until);
CREATE TABLE cloud_agent_outbox (
    event_id TEXT PRIMARY KEY,
    agent_run_id TEXT NOT NULL REFERENCES cloud_agent_runs(agent_run_id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    publish_attempts INTEGER NOT NULL DEFAULT 0 CHECK(publish_attempts >= 0),
    last_error TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX cloud_agent_outbox_ready_idx ON cloud_agent_outbox(status, available_at, event_id);
