-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE local_connector_devices (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    status TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE UNIQUE INDEX local_connector_devices_active_identity_unique
    ON local_connector_devices(owner_user_id, public_key)
    WHERE status IN ('registered', 'online', 'offline');
CREATE INDEX local_connector_devices_owner_updated_idx
    ON local_connector_devices(owner_user_id, updated_at DESC);
CREATE INDEX local_connector_devices_status_idx ON local_connector_devices(status);

CREATE TABLE local_connector_workspaces (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    device_id TEXT NOT NULL REFERENCES local_connector_devices(id) ON DELETE RESTRICT,
    status TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX local_connector_workspaces_owner_updated_idx
    ON local_connector_workspaces(owner_user_id, updated_at DESC);
CREATE INDEX local_connector_workspaces_device_idx ON local_connector_workspaces(device_id);

CREATE TABLE local_connector_project_bindings (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    mode TEXT NOT NULL,
    device_id TEXT NOT NULL REFERENCES local_connector_devices(id) ON DELETE RESTRICT,
    workspace_id TEXT NOT NULL REFERENCES local_connector_workspaces(id) ON DELETE RESTRICT,
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(owner_user_id, project_id, mode)
);
CREATE INDEX local_connector_project_bindings_workspace_idx
    ON local_connector_project_bindings(workspace_id);
CREATE INDEX local_connector_project_bindings_owner_updated_idx
    ON local_connector_project_bindings(owner_user_id, updated_at DESC);

CREATE TABLE local_connector_sandbox_pairings (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL,
    device_id TEXT NOT NULL REFERENCES local_connector_devices(id) ON DELETE RESTRICT,
    workspace_id TEXT NOT NULL REFERENCES local_connector_workspaces(id) ON DELETE RESTRICT,
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE(owner_user_id, device_id, workspace_id)
);
CREATE INDEX local_connector_sandbox_pairings_owner_updated_idx
    ON local_connector_sandbox_pairings(owner_user_id, updated_at DESC);
CREATE INDEX local_connector_sandbox_pairings_workspace_idx
    ON local_connector_sandbox_pairings(workspace_id);

CREATE TABLE local_connector_active_sessions (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL UNIQUE,
    device_id TEXT NOT NULL REFERENCES local_connector_devices(id) ON DELETE RESTRICT,
    status TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX local_connector_active_sessions_device_status_idx
    ON local_connector_active_sessions(device_id, status);
CREATE INDEX local_connector_active_sessions_expires_idx
    ON local_connector_active_sessions(expires_at);

CREATE TABLE local_connector_managed_requirements_policies (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX local_connector_managed_requirements_policies_enabled_idx
    ON local_connector_managed_requirements_policies(enabled, updated_at DESC);

CREATE TABLE local_connector_managed_requirements_assignments (
    id TEXT PRIMARY KEY,
    policy_id TEXT NOT NULL REFERENCES local_connector_managed_requirements_policies(id) ON DELETE RESTRICT,
    scope TEXT NOT NULL,
    subject TEXT NULL,
    priority INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE UNIQUE INDEX local_connector_managed_requirements_assignment_unique
    ON local_connector_managed_requirements_assignments(policy_id, scope, COALESCE(subject, ''));
CREATE INDEX local_connector_managed_requirements_assignment_lookup_idx
    ON local_connector_managed_requirements_assignments(enabled, scope, subject, priority);
