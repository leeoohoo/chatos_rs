-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE mcp_management_runtime_session_snapshots (
    session_id TEXT PRIMARY KEY,
    schema_version INTEGER NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    expires_at_unix BIGINT NOT NULL,
    execution_scope_hash TEXT NULL,
    nonce BYTEA NOT NULL,
    encrypted_snapshot BYTEA NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX mcp_runtime_sessions_expiry_idx
    ON mcp_management_runtime_session_snapshots(expires_at, session_id);

CREATE TABLE mcp_management_runtime_session_close_results (
    session_id TEXT PRIMARY KEY,
    caller_service TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    expires_at_unix BIGINT NOT NULL,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX mcp_runtime_session_close_expiry_idx
    ON mcp_management_runtime_session_close_results(expires_at, session_id);

CREATE TABLE mcp_management_runtime_invocations (
    invocation_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    request_id_key TEXT NOT NULL,
    caller_service TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    project_id TEXT NULL,
    device_id TEXT NULL,
    resource_id TEXT NOT NULL,
    status TEXT NOT NULL,
    mutation_may_have_started BOOLEAN NOT NULL DEFAULT false,
    cancel_supported BOOLEAN NOT NULL DEFAULT false,
    created_at_unix_ms BIGINT NOT NULL,
    started_at_unix_ms BIGINT NULL,
    completed_at_unix_ms BIGINT NULL,
    file_modification_outcome TEXT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    expires_at_unix BIGINT NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(session_id, request_id_key)
);
CREATE INDEX mcp_runtime_invocations_expiry_status_idx
    ON mcp_management_runtime_invocations(expires_at, status);
CREATE INDEX mcp_runtime_invocations_tenant_active_idx
    ON mcp_management_runtime_invocations(tenant_id, status, expires_at);
CREATE INDEX mcp_runtime_invocations_owner_active_idx
    ON mcp_management_runtime_invocations(owner_user_id, status, expires_at);
CREATE INDEX mcp_runtime_invocations_project_active_idx
    ON mcp_management_runtime_invocations(project_id, status, expires_at)
    WHERE project_id IS NOT NULL;
CREATE INDEX mcp_runtime_invocations_device_active_idx
    ON mcp_management_runtime_invocations(device_id, status, expires_at)
    WHERE device_id IS NOT NULL;
CREATE INDEX mcp_runtime_invocations_session_status_idx
    ON mcp_management_runtime_invocations(session_id, status, created_at_unix_ms, invocation_id);

CREATE TABLE mcp_management_runtime_execution_scopes (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    scope_kind TEXT NOT NULL,
    project_id TEXT NULL,
    run_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    generation BIGINT NOT NULL CHECK(generation > 0),
    status TEXT NOT NULL,
    terminal_status TEXT NULL,
    next_invocation_sequence BIGINT NOT NULL DEFAULT 0,
    running_invocation_id TEXT NULL,
    session_refs JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    expires_at_unix BIGINT NOT NULL
);
CREATE INDEX mcp_runtime_execution_scopes_expiry_idx
    ON mcp_management_runtime_execution_scopes(expires_at, id);
CREATE INDEX mcp_runtime_execution_scopes_run_idx
    ON mcp_management_runtime_execution_scopes(owner_user_id, run_id, provider, project_id);

CREATE TABLE mcp_management_runtime_execution_scope_queue_items (
    scope_id TEXT NOT NULL REFERENCES mcp_management_runtime_execution_scopes(id) ON DELETE CASCADE,
    invocation_id TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    batch_id TEXT NULL,
    call_index BIGINT NULL CHECK(call_index IS NULL OR call_index >= 0),
    status TEXT NOT NULL DEFAULT 'queued',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(scope_id, sequence),
    UNIQUE(invocation_id)
);
CREATE INDEX mcp_runtime_scope_queue_ready_idx
    ON mcp_management_runtime_execution_scope_queue_items(scope_id, status, sequence);
CREATE INDEX mcp_runtime_scope_queue_batch_idx
    ON mcp_management_runtime_execution_scope_queue_items(batch_id, call_index)
    WHERE batch_id IS NOT NULL;

CREATE TABLE mcp_management_runtime_tool_batches (
    batch_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    status TEXT NOT NULL,
    next_call_index BIGINT NOT NULL CHECK(next_call_index >= 0),
    pending_event_type TEXT NULL,
    revision BIGINT NOT NULL CHECK(revision >= 0),
    invocation_ids TEXT[] NOT NULL DEFAULT '{}',
    waiting_user_prompt_ids TEXT[] NOT NULL DEFAULT '{}',
    created_at_unix_ms BIGINT NOT NULL,
    updated_at_unix_ms BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    expires_at_unix BIGINT NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX mcp_runtime_tool_batches_expiry_idx
    ON mcp_management_runtime_tool_batches(expires_at, batch_id);
CREATE INDEX mcp_runtime_tool_batches_pending_idx
    ON mcp_management_runtime_tool_batches(pending_event_type, updated_at_unix_ms, batch_id)
    WHERE pending_event_type IS NOT NULL;
CREATE INDEX mcp_runtime_tool_batches_invocations_gin_idx
    ON mcp_management_runtime_tool_batches USING GIN(invocation_ids);
CREATE INDEX mcp_runtime_tool_batches_prompts_gin_idx
    ON mcp_management_runtime_tool_batches USING GIN(waiting_user_prompt_ids);

CREATE TABLE mcp_management_skill_activation_metadata (
    key TEXT PRIMARY KEY,
    fingerprint_sha256 TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE mcp_management_skill_activations (
    activation_ref TEXT PRIMARY KEY,
    runtime_session_id TEXT NOT NULL,
    equivalence_sha256 TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    expires_at_unix BIGINT NOT NULL,
    nonce BYTEA NOT NULL,
    encrypted_activation BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX mcp_skill_activations_expiry_idx
    ON mcp_management_skill_activations(expires_at, activation_ref);
CREATE INDEX mcp_skill_activations_session_equivalence_idx
    ON mcp_management_skill_activations(runtime_session_id, equivalence_sha256, expires_at);
